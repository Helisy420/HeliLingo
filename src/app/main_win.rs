//! The main translator window and the "Screen area" overlay: their
//! viewports, the workers behind them (debounced text translation; image
//! decoding, OCR and block translation) and the window's actions.
//!
//! Entry points for the hotkeys and the tray: [`App::open_main_window`],
//! [`App::open_history`], [`App::start_screen_capture`].

use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use egui::{ViewportBuilder, ViewportCommand, ViewportId};
use windows::Win32::UI::WindowsAndMessaging::{
    GWL_STYLE, GetWindowLongPtrW, HWND_TOPMOST, IsIconic, SW_RESTORE, SWP_NOACTIVATE,
    SetForegroundWindow, SetWindowLongPtrW, SetWindowPos, ShowWindow, WS_MINIMIZEBOX,
};

use super::{App, Floating, centered, window_icon};
use crate::history::{self, Mode};
use crate::i18n::tr;
use crate::settings::ProviderKind;
use crate::translate::{self, Engine, Error, Translation};
use crate::ui::main_window::capture::{self as overlay, CaptureAction, CaptureView};
use crate::ui::main_window::image_tab::{
    self, Failure, ImageResult, RenderJob, TranslatedBlock, block_colors,
};
use crate::ui::main_window::{
    self as mw, Env, ImageDoc, ImageSlot, MainAction, MainState, Stage, Tab, View,
};
use crate::win::image::{self as wimage, LoadError, Pasted, RgbaImage};
use crate::win::{self, Hwnd, capture, clipboard, ocr, speech};

const CAPTURE_TITLE: &str = "HeliLingo — screen area";
/// Translate once typing pauses this long.
const DEBOUNCE: Duration = Duration::from_millis(400);
/// A result that stayed this long goes into history.
const STABLE: Duration = Duration::from_secs(2);

fn main_id() -> ViewportId {
    ViewportId::from_hash_of("qt-main")
}

fn capture_id() -> ViewportId {
    ViewportId::from_hash_of("qt-capture")
}

/// What an image job starts from.
enum ImageInput {
    Image(RgbaImage),
    File(PathBuf),
    Clipboard,
}

/// Results of helper threads that the app has to act on.
enum Internal {
    LoadFile(PathBuf),
}

struct CaptureWin {
    view: Arc<Mutex<CaptureView>>,
    origin: (i32, i32),
    size: (i32, i32),
    hwnd: Option<Hwnd>,
    shown: bool,
    opened: Instant,
}

/// State of the main window and the capture overlay, owned by [`App`].
pub(super) struct MainWin {
    model: Arc<Mutex<MainState>>,
    tx: Sender<MainAction>,
    rx: Receiver<MainAction>,
    int_tx: Sender<Internal>,
    int_rx: Receiver<Internal>,
    cap_tx: Sender<CaptureAction>,
    cap_rx: Receiver<CaptureAction>,
    open: bool,
    win: Floating,
    title: String,
    minimize_box: bool,
    /// The window was hidden so it isn't in the screenshot.
    hidden_for_capture: bool,
    capture: Option<CaptureWin>,
}

impl MainWin {
    pub(super) fn new(target: &str) -> Self {
        let (tx, rx) = channel();
        let (int_tx, int_rx) = channel();
        let (cap_tx, cap_rx) = channel();
        Self {
            model: Arc::new(Mutex::new(MainState::new(target))),
            tx,
            rx,
            int_tx,
            int_rx,
            cap_tx,
            cap_rx,
            open: false,
            win: Floating::new(),
            title: String::new(),
            minimize_box: false,
            hidden_for_capture: false,
            capture: None,
        }
    }
}

/// Brings a (possibly minimised or hidden) window back to the front.
fn restore(h: Hwnd) {
    unsafe {
        if IsIconic(h.raw()).as_bool() {
            let _ = ShowWindow(h.raw(), SW_RESTORE);
        }
    }
    win::show_and_focus(h);
}

/// `strip_chrome` drops WS_MINIMIZEBOX; without it a click on the taskbar
/// button doesn't minimise the window.
fn allow_minimize(h: Hwnd) {
    unsafe {
        let style = GetWindowLongPtrW(h.raw(), GWL_STYLE);
        SetWindowLongPtrW(h.raw(), GWL_STYLE, style | WS_MINIMIZEBOX.0 as isize);
    }
}

/// Our own top-level window with this title (another instance, e.g. a
/// preview, may have one with the same title).
fn find_own_window(title: &str) -> Option<Hwnd> {
    use windows::Win32::UI::WindowsAndMessaging::FindWindowExW;
    let title = windows::core::HSTRING::from(title);
    let mut after = None;
    loop {
        let h = unsafe { FindWindowExW(None, after, None, &title) }.ok().filter(|h| !h.is_invalid())?;
        if win::is_own_window(h) {
            return Some(Hwnd::from_raw(h));
        }
        after = Some(h);
    }
}

fn place_exact(h: Hwnd, (x, y): (i32, i32), (w, hgt): (i32, i32)) {
    unsafe {
        let _ = SetWindowPos(h.raw(), Some(HWND_TOPMOST), x, y, w, hgt, SWP_NOACTIVATE);
    }
}

impl App {
    /// Opens the translator window, or brings it to the front.
    pub fn open_main_window(&mut self) {
        if self.main.open {
            if let Some(h) = self.main.win.hwnd {
                restore(h);
            }
            return;
        }
        self.main.open = true;
        self.main.win = Floating::new();
        self.main.minimize_box = false;
        self.main.title = format!("{} — {}", crate::APP_NAME, tr("translator"));
        let mut m = self.main.model.lock().unwrap();
        m.frames = 0;
        if m.input.is_empty() && m.result.is_none() {
            m.to = self.settings.target.clone();
        }
        drop(m);
        self.ctx.request_repaint();
    }

    /// Opens the translator window on the History view.
    pub fn open_history(&mut self) {
        self.main.model.lock().unwrap().view = View::History;
        self.open_main_window();
    }

    /// "Screen area": freezes the screen and lets the user drag a
    /// rectangle; the crop is recognised and translated in the Images tab.
    pub fn start_screen_capture(&mut self) {
        if self.main.capture.is_some() {
            return;
        }
        self.hide_popup();
        self.hide_main_for_capture();
        let Some(shot) = capture::capture_screen() else {
            self.capture_failed();
            return;
        };
        let (cx, cy) = win::cursor_pos();
        let m = win::monitor_at(cx, cy);
        let (ox, oy) = shot.origin;
        let hint = (
            ((m.work.left + m.work.right) / 2 - ox) as f32,
            (m.work.top - oy) as f32 + 24.0 * m.scale,
        );
        // Window rects and the monitor in screenshot pixels (the virtual
        // screen may start at negative coordinates).
        let to_px = |l: i32, t: i32, r: i32, b: i32| {
            egui::Rect::from_min_max(egui::pos2((l - ox) as f32, (t - oy) as f32), egui::pos2((r - ox) as f32, (b - oy) as f32))
        };
        let windows = capture::window_rects().iter().map(|r| to_px(r.left, r.top, r.right, r.bottom)).collect();
        let (mx, my, mw, mh) = capture::monitor_rect_at(cx, cy);
        let monitor = to_px(mx, my, mx + mw, my + mh);
        let size = (shot.image.width as i32, shot.image.height as i32);
        self.main.capture = Some(CaptureWin {
            view: Arc::new(Mutex::new(CaptureView::new(Arc::new(shot.image), hint, windows, monitor))),
            origin: shot.origin,
            size,
            hwnd: None,
            shown: false,
            opened: Instant::now(),
        });
        self.ctx.request_repaint();
    }

    /// "Entire screen": the monitor under the cursor in one action (no
    /// overlay), recognised and translated in the Images tab.
    pub fn capture_whole_screen(&mut self) {
        if self.main.capture.is_some() {
            return;
        }
        self.hide_popup();
        self.hide_main_for_capture();
        let (cx, cy) = win::cursor_pos();
        let (x, y, w, h) = capture::monitor_rect_at(cx, cy);
        let Some(shot) = capture::capture_rect(x, y, w, h) else {
            self.capture_failed();
            return;
        };
        self.main.hidden_for_capture = false;
        self.open_main_window();
        self.start_image(ImageInput::Image(shot.image));
    }

    /// Keeps our own window out of the screenshot.
    fn hide_main_for_capture(&mut self) {
        if self.main.open
            && let Some(h) = self.main.win.hwnd
            && win::is_visible(h)
        {
            win::hide(h);
            self.main.hidden_for_capture = true;
            std::thread::sleep(Duration::from_millis(200));
        }
    }

    fn capture_failed(&mut self) {
        self.main.hidden_for_capture = false;
        self.main.model.lock().unwrap().image = ImageSlot::Failed(Failure::Capture);
        self.open_main_window();
    }

    // ------------------------------------------------------------ frames

    pub(super) fn main_window_frame(&mut self, ctx: &egui::Context) {
        while let Ok(a) = self.main.rx.try_recv() {
            self.on_main_action(a);
        }
        while let Ok(i) = self.main.int_rx.try_recv() {
            match i {
                Internal::LoadFile(p) => self.start_image(ImageInput::File(p)),
            }
        }
        if !self.main.open {
            return;
        }
        self.update_env();
        self.main_timers(ctx);

        let card_h = self.main.model.lock().unwrap().card_h();
        let builder = ViewportBuilder::default()
            .with_title(self.main.title.clone())
            .with_icon(window_icon())
            .with_inner_size(mw::window_size(card_h))
            .with_decorations(false)
            .with_transparent(true)
            .with_resizable(false)
            .with_drag_and_drop(true)
            .with_visible(false);
        let (model, tx) = (self.main.model.clone(), self.main.tx.clone());
        ctx.show_viewport_deferred(main_id(), builder, move |ui, _| {
            let mut st = model.lock().unwrap();
            let actions = mw::show(ui, &mut st);
            let wake = !actions.is_empty() || st.edited_at.is_some();
            for a in actions {
                if let MainAction::Minimize = a {
                    ui.ctx().send_viewport_cmd(ViewportCommand::Minimized(true));
                } else {
                    let _ = tx.send(a);
                }
            }
            drop(st);
            if wake {
                ui.ctx().request_repaint_of(ViewportId::ROOT);
            }
        });
        let size = mw::window_size(card_h);
        if self.main.win.hwnd.is_none() {
            self.main.win.hwnd = find_own_window(&self.main.title);
        }
        self.main.win.reveal(&self.main.title, || centered(size));
        if !self.main.win.shown {
            // The window appears after its first frame; come back for it.
            ctx.request_repaint_after(Duration::from_millis(16));
        }
        if self.main.win.shown
            && !self.main.minimize_box
            && let Some(h) = self.main.win.hwnd
        {
            allow_minimize(h);
            self.main.minimize_box = true;
        }
    }

    fn update_env(&self) {
        let s = &self.settings;
        let env = Env {
            providers: s
                .providers
                .iter()
                .filter(|p| p.enabled && s.keys.usable(p.kind))
                .map(|p| p.kind)
                .collect(),
            shortcut: s.shortcut.label(),
            screen_keys: s.hotkeys.screen_area.map(|c| c.keys()).unwrap_or_default(),
            whole_screen_keys: s.hotkeys.whole_screen.map(|c| c.keys()).unwrap_or_default(),
            wiki: crate::wiki::enabled(),
        };
        self.main.model.lock().unwrap().env = env;
    }

    /// Debounced translation and the history timer.
    fn main_timers(&mut self, ctx: &egui::Context) {
        let (due, record) = {
            let m = self.main.model.lock().unwrap();
            let due = m.edited_at.map(|t| t.elapsed());
            let record = match (&m.result, m.result_at) {
                (Some(Ok(_)), Some(at)) if !m.recorded && !m.loading && m.edited_at.is_none() => Some(at.elapsed()),
                _ => None,
            };
            (due, record)
        };
        match due {
            Some(e) if e >= DEBOUNCE => self.translate_text(),
            Some(e) => ctx.request_repaint_after(DEBOUNCE - e),
            None => {}
        }
        match record {
            Some(e) if e >= STABLE => self.record_text(),
            Some(e) => ctx.request_repaint_after(STABLE - e),
            None => {}
        }
    }

    pub(super) fn capture_frame(&mut self, ctx: &egui::Context) {
        while let Ok(a) = self.main.cap_rx.try_recv() {
            self.on_capture_action(a);
        }
        let Some(c) = &mut self.main.capture else { return };
        let primary = win::monitor_at(0, 0).scale;
        let builder = ViewportBuilder::default()
            .with_title(CAPTURE_TITLE)
            .with_inner_size([c.size.0 as f32 / primary, c.size.1 as f32 / primary])
            .with_decorations(false)
            .with_resizable(false)
            .with_always_on_top()
            .with_taskbar(false)
            .with_visible(false);
        let (view, tx) = (c.view.clone(), self.main.cap_tx.clone());
        ctx.show_viewport_deferred(capture_id(), builder, move |ui, _| {
            let mut v = view.lock().unwrap();
            if let Some(a) = overlay::show(ui, &mut v) {
                let _ = tx.send(a);
                ui.ctx().request_repaint_of(ViewportId::ROOT);
            }
        });
        if c.hwnd.is_none() {
            c.hwnd = find_own_window(CAPTURE_TITLE);
        }
        let Some(h) = c.hwnd else {
            ctx.request_repaint_after(Duration::from_millis(16));
            return;
        };
        // Cover the whole virtual screen in physical pixels (winit may
        // resize the window when it crosses monitors with another DPI).
        let r = win::window_rect(h);
        if (r.left, r.top, r.right - r.left, r.bottom - r.top) != (c.origin.0, c.origin.1, c.size.0, c.size.1) {
            place_exact(h, c.origin, c.size);
        }
        if !c.shown {
            win::strip_chrome(h);
            place_exact(h, c.origin, c.size);
            win::show_and_focus(h);
            unsafe {
                let _ = SetForegroundWindow(h.raw());
            }
            c.shown = true;
            c.opened = Instant::now();
        } else if !self.preview && c.opened.elapsed() > Duration::from_millis(700) && win::foreground() != h {
            // Switched away (Alt+Tab, Win key): never leave a topmost
            // fullscreen window behind.
            self.on_capture_action(CaptureAction::Cancel);
            return;
        }
        ctx.request_repaint_after(Duration::from_millis(100));
    }

    fn on_capture_action(&mut self, a: CaptureAction) {
        let Some(c) = self.main.capture.take() else { return };
        let image = c.view.lock().unwrap().image.clone();
        match a {
            CaptureAction::Cancel => {
                if self.main.hidden_for_capture {
                    self.main.hidden_for_capture = false;
                    if let Some(h) = self.main.win.hwnd {
                        restore(h);
                    }
                }
            }
            CaptureAction::Select(x, y, w, h) => {
                self.main.hidden_for_capture = false;
                let crop = image.crop(x, y, w, h);
                self.open_main_window();
                self.start_image(ImageInput::Image(crop));
            }
        }
    }

    // ----------------------------------------------------------- actions

    fn on_main_action(&mut self, a: MainAction) {
        match a {
            MainAction::Close => {
                self.record_text();
                self.main.open = false;
                self.main.win = Floating::new();
            }
            MainAction::Minimize => self.ctx.send_viewport_cmd_to(main_id(), ViewportCommand::Minimized(true)),
            MainAction::OpenSettings => self.open_settings(),
            MainAction::TranslateNow => self.translate_text(),
            MainAction::Record => self.record_text(),
            MainAction::Speak(text, lang) => speech::speak(&text, &lang),
            MainAction::Copy(text) => {
                std::thread::spawn(move || clipboard::set_text(&text, false));
            }
            MainAction::ChooseFile => {
                let (owner, tx, ctx) = (self.main.win.hwnd.unwrap_or_default(), self.main.int_tx.clone(), self.ctx.clone());
                std::thread::spawn(move || {
                    if let Some(p) = wimage::pick_image(owner, tr("Choose an image"), tr("Images")) {
                        let _ = tx.send(Internal::LoadFile(p));
                        ctx.request_repaint_of(ViewportId::ROOT);
                    }
                });
            }
            MainAction::ScreenArea => self.start_screen_capture(),
            MainAction::WholeScreen => self.capture_whole_screen(),
            MainAction::PasteImage => self.start_image(ImageInput::Clipboard),
            MainAction::LoadFile(p) => self.start_image(ImageInput::File(p)),
            MainAction::RetranslateImage => self.retranslate_image(),
            MainAction::CopyPicture(job) => {
                std::thread::spawn(move || {
                    if let Some(img) = image_tab::render_picture(&job) {
                        wimage::copy_image(&img);
                    }
                });
            }
            MainAction::SavePicture(job) => {
                let owner = self.main.win.hwnd.unwrap_or_default();
                std::thread::spawn(move || save_picture(owner, &job));
            }
            MainAction::SaveText(text) => {
                let owner = self.main.win.hwnd.unwrap_or_default();
                std::thread::spawn(move || {
                    let name = format!("{}.txt", tr("translation"));
                    if let Some(p) = wimage::pick_save_path(owner, tr("Save the text"), &name, tr("Text"), "txt") {
                        let _ = std::fs::write(p, text.replace('\n', "\r\n"));
                    }
                });
            }
        }
    }

    // -------------------------------------------------------- text worker

    fn translate_text(&mut self) {
        let mut m = self.main.model.lock().unwrap();
        m.edited_at = None;
        m.text_gen += 1;
        let generation = m.text_gen;
        let text = translate::normalize(&m.input);
        if text.is_empty() {
            m.result = None;
            m.loading = false;
            return;
        }
        m.loading = true;
        let (from, to, prefer) = (m.from.clone(), m.to.clone(), m.prefer);
        drop(m);
        let (model, engine, ctx) = (self.main.model.clone(), self.engine.clone(), self.ctx.clone());
        let speak = self.settings.speak_results;
        let programmer = self.settings.programmer_mode;
        engine.warm_up();
        std::thread::spawn(move || {
            // Programmer mode: in code, only strings and comments.
            let code = programmer.then(|| crate::code::translate(&engine, &text, &from, &to, prefer)).flatten();
            let result = code.unwrap_or_else(|| match engine.translate_using(&text, &from, &to, prefer) {
                // A cold connection sometimes times out; try once more.
                Err(Error::Offline) => engine.translate_using(&text, &from, &to, prefer),
                r => r,
            });
            let mut m = model.lock().unwrap();
            if m.text_gen != generation {
                return;
            }
            if speak && let Ok(t) = &result {
                speech::speak(&t.text, &t.tgt_lang);
            }
            m.loading = false;
            m.result = Some(result);
            m.result_at = Some(Instant::now());
            m.recorded = false;
            drop(m);
            ctx.request_repaint_of(main_id());
            ctx.request_repaint_of(ViewportId::ROOT);
        });
    }

    /// Puts the current text result into history (once).
    fn record_text(&self) {
        let mut m = self.main.model.lock().unwrap();
        if m.recorded {
            return;
        }
        if let Some(Ok(t)) = &m.result {
            let from = t.src_lang.clone().unwrap_or_else(|| m.from.clone());
            history::add(&t.source, &t.text, &from, &t.tgt_lang, t.provider, Mode::Main);
        }
        m.recorded = true;
    }

    // ------------------------------------------------------- image worker

    fn start_image(&mut self, input: ImageInput) {
        let generation = {
            let mut m = self.main.model.lock().unwrap();
            m.image_gen += 1;
            m.tab = Tab::Image;
            m.view = View::Translator;
            m.image = ImageSlot::Loading;
            m.image_gen
        };
        let job = self.image_job(generation);
        std::thread::spawn(move || {
            wimage::ensure_mta();
            let image = match input {
                ImageInput::Image(img) => Ok(img),
                ImageInput::File(p) => wimage::load_file(&p),
                ImageInput::Clipboard => match wimage::clipboard_image() {
                    Some(Pasted::Bytes(b)) => {
                        if b.len() as u64 > wimage::MAX_BYTES {
                            Err(LoadError::TooLarge)
                        } else {
                            wimage::decode(&b)
                        }
                    }
                    Some(Pasted::File(p)) => wimage::load_file(&p),
                    None => {
                        job.set_slot(ImageSlot::Failed(Failure::EmptyClipboard));
                        return;
                    }
                },
            };
            match image {
                Ok(img) => {
                    let img = Arc::new(img);
                    job.set_slot(ImageSlot::Doc(ImageDoc::new(img.clone())));
                    job.run(img);
                }
                Err(e) => job.set_slot(ImageSlot::Failed(Failure::Load(e))),
            }
        });
        self.ctx.request_repaint_of(main_id());
    }

    /// Languages or provider changed: recognise and translate the same
    /// picture again.
    fn retranslate_image(&mut self) {
        let (generation, img) = {
            let mut m = self.main.model.lock().unwrap();
            m.image_gen += 1;
            let generation = m.image_gen;
            let ImageSlot::Doc(doc) = &mut m.image else { return };
            doc.stage = Stage::Recognizing;
            (generation, doc.image.clone())
        };
        let job = self.image_job(generation);
        std::thread::spawn(move || {
            wimage::ensure_mta();
            job.run(img);
        });
    }

    fn image_job(&self, generation: u64) -> ImageJob {
        let m = self.main.model.lock().unwrap();
        ImageJob {
            model: self.main.model.clone(),
            ctx: self.ctx.clone(),
            engine: self.engine.clone(),
            generation,
            from: m.from.clone(),
            to: m.to.clone(),
            prefer: m.prefer,
        }
    }

    // ----------------------------------------------------------- preview

    /// `--preview main | main-image | main-image-result | main-history |
    /// capture | capture-window`.
    pub(super) fn preview_main(&mut self, what: &str) {
        match what {
            "capture" => self.start_screen_capture(),
            "capture-window" => {
                self.start_screen_capture();
                if let Some(c) = &self.main.capture {
                    c.view.lock().unwrap().mode = overlay::CaptureMode::Window;
                }
            }
            "main-history" => self.open_history(),
            _ => {
                self.open_main_window();
                let mut m = self.main.model.lock().unwrap();
                match what {
                    "main-image" => m.tab = Tab::Image,
                    "main-image-result" | "main-image-zoom" => {
                        m.zoom_when_done = what == "main-image-zoom";
                        drop(m);
                        match image_tab::sample_picture() {
                            Some(img) => self.start_image(ImageInput::Image(img)),
                            None => self.main.model.lock().unwrap().tab = Tab::Image,
                        }
                    }
                    _ => {
                        // Sample text from the Figma frame, translated live.
                        m.input = "I just need to do one more thing".into();
                        m.edited_at = Some(Instant::now() - DEBOUNCE);
                    }
                }
            }
        }
    }
}

/// Everything an image worker needs; results are dropped if the user has
/// moved on (`generation`).
struct ImageJob {
    model: Arc<Mutex<MainState>>,
    ctx: egui::Context,
    engine: Arc<Engine>,
    generation: u64,
    from: String,
    to: String,
    prefer: Option<ProviderKind>,
}

impl ImageJob {
    fn set_slot(&self, slot: ImageSlot) {
        let mut m = self.model.lock().unwrap();
        if m.image_gen == self.generation {
            m.image = slot;
        }
        drop(m);
        self.ctx.request_repaint_of(main_id());
    }

    fn set_stage(&self, stage: Stage) {
        let mut m = self.model.lock().unwrap();
        if m.image_gen == self.generation
            && let ImageSlot::Doc(doc) = &mut m.image
        {
            doc.stage = stage;
        }
        drop(m);
        self.ctx.request_repaint_of(main_id());
    }

    fn current(&self) -> bool {
        self.model.lock().unwrap().image_gen == self.generation
    }

    /// One block; a network hiccup is retried once.
    fn translate_block(&self, b: &ocr::Block) -> Result<Arc<Translation>, Error> {
        let text: String = b.text.trim().chars().take(translate::MAX_CHARS).collect();
        match self.engine.translate_using(&text, &self.from, &self.to, self.prefer) {
            Err(Error::Offline) => self.engine.translate_using(&text, &self.from, &self.to, self.prefer),
            r => r,
        }
    }

    /// OCR, then every block translated (a few at a time).
    fn run(&self, img: Arc<RgbaImage>) {
        let lang = (self.from != "auto").then_some(self.from.as_str());
        let rec = match ocr::recognize(&img, lang) {
            Ok(r) => r,
            Err(e) => return self.set_stage(Stage::Failed(Failure::Ocr(e))),
        };
        if rec.blocks.is_empty() {
            return self.set_stage(Stage::Failed(Failure::NoText));
        }
        self.set_stage(Stage::Translating(rec.blocks.clone()));

        // The first block alone (opens the connection), then the rest a
        // few at a time.
        let mut results: Vec<Option<Result<Arc<Translation>, Error>>> = vec![None; rec.blocks.len()];
        results[0] = Some(self.translate_block(&rec.blocks[0]));
        for (chunk_i, chunk) in rec.blocks[1..].chunks(6).enumerate() {
            if !self.current() {
                return;
            }
            std::thread::scope(|s| {
                let handles: Vec<_> = chunk.iter().map(|b| s.spawn(move || self.translate_block(b))).collect();
                for (i, h) in handles.into_iter().enumerate() {
                    results[1 + chunk_i * 6 + i] = h.join().ok();
                }
            });
        }
        let first_err = results.iter().find_map(|r| match r {
            Some(Err(e)) => Some(e.clone()),
            _ => None,
        });
        let ok: Vec<&Arc<Translation>> = results.iter().filter_map(|r| r.as_ref()?.as_ref().ok()).collect();
        if ok.is_empty() {
            let e = first_err.unwrap_or(Error::Other("Empty response".into()));
            return self.set_stage(Stage::Failed(Failure::Translate(e)));
        }
        let src_lang = ok.iter().find_map(|t| t.src_lang.clone()).or_else(|| lang.map(str::to_owned));
        let provider = ok.first().map(|t| t.provider);
        let blocks = rec
            .blocks
            .iter()
            .zip(&results)
            .map(|(b, r)| {
                let text = match r {
                    Some(Ok(t)) => t.text.clone(),
                    _ => b.text.clone(),
                };
                TranslatedBlock::new(b.clone(), text, block_colors(&img, &b.bounds))
            })
            .collect();
        let result = ImageResult { blocks, ocr_lang: rec.language, src_lang, provider, to: self.to.clone() };
        if let Some(p) = result.provider {
            let from = result.src_lang.clone().unwrap_or_else(|| self.from.clone());
            history::add(&result.source_text(), &result.translated_text(), &from, &result.to, p, Mode::Image);
        }
        self.set_stage(Stage::Done(result));
    }
}

fn save_picture(owner: Hwnd, job: &RenderJob) {
    let name = format!("{}.png", tr("translated"));
    let Some(path) = wimage::pick_save_path(owner, tr("Save the picture"), &name, "PNG", "png") else {
        return;
    };
    if let Some(png) = image_tab::render_picture(job).and_then(|img| img.encode_png()) {
        let _ = std::fs::write(path, png);
    }
}
