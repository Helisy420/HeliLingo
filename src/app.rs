//! Application state: wires the input hook, the translation engine, the
//! tray icon and the four surfaces (popup, tray menu, settings, intro).
//!
//! The root eframe viewport *is* the popup. It is a borderless, transparent,
//! always-on-top tool window that never takes focus, so the user's
//! selection in the source app survives while the popup is open. It is
//! larger than any card; the area around the card is click-through.

mod quick;
mod ultra;

use std::sync::{Arc, Mutex};
use std::sync::atomic::Ordering;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant};

use egui::{Align2, Rect, ViewportBuilder, ViewportCommand, ViewportId, pos2};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use tray_icon::{Icon, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

use crate::icons;
use crate::settings::{HotkeySlot, ProviderKind, Settings, Shortcut, lang_name};
use crate::theme;
use crate::translate::{self, Engine, Error, Translation};
use crate::ui::popup::{self, Action, Content, Feedback, SentenceView, WordView};
use crate::ui::settings_view::SettingsAction;
use crate::ui::tray_menu::{self, Page, TrayAction, TrayView};
use crate::ui::welcome;
use crate::win::{self, Anchor, Hwnd, Monitor, clipboard, hook, hook::HookEvent, speech};

mod main_win;

const POPUP_TITLE: &str = "HeliLingo popup";
const TRAY_TITLE: &str = "HeliLingo menu";
const SETTINGS_TITLE: &str = "HeliLingo — settings";
const WELCOME_TITLE: &str = "HeliLingo — how it works";

/// Popup window size (points) and where the card sits inside it.
const POPUP_W: f32 = 540.0;
const POPUP_H: f32 = 480.0;
const POPUP_MX: f32 = 24.0;
const POPUP_MT: f32 = 16.0;
const POPUP_MB: f32 = 36.0;

/// How long to wait for the source app to put the selection on the clipboard.
const COPY_TIMEOUT: Duration = Duration::from_millis(300);

mod settings_win;
use settings_win::SettingsWindow;

pub fn popup_viewport() -> ViewportBuilder {
    ViewportBuilder::default()
        .with_title(POPUP_TITLE)
        .with_inner_size([POPUP_W, POPUP_H])
        .with_decorations(false)
        .with_transparent(true)
        .with_resizable(false)
        .with_always_on_top()
        .with_taskbar(false)
        .with_active(false)
        .with_visible(false)
}

fn window_icon() -> Arc<egui::IconData> {
    Arc::new(egui::IconData {
        rgba: icons::app_icon_rgba(64),
        width: 64,
        height: 64,
    })
}

enum WorkMsg {
    /// The selection was read; show the popup (with a cached result if any).
    Grabbed {
        generation: u64,
        text: String,
        anchor: Anchor,
        target: Hwnd,
        cached: Option<Arc<Translation>>,
    },
    Done {
        generation: u64,
        result: Result<Arc<Translation>, Error>,
    },
    /// Translate and paste: the translation replaced the selection.
    Pasted(Arc<Translation>),
    /// Compact popup: another provider's translation (`None` = it failed).
    Variant {
        generation: u64,
        provider: ProviderKind,
        text: Option<String>,
    },
    /// Translate and paste failed: say why in the popup.
    PasteFailed {
        text: String,
        error: Error,
        anchor: Anchor,
        target: Hwnd,
    },
}

/// Where a translation's text comes from.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Grab {
    /// Copy the foreground app's selection (synthetic Ctrl+C).
    Selection,
    /// Ctrl+C ×2: the user just copied it; read the clipboard.
    Clipboard,
}

struct Request {
    text: String,
    from: String,
    to: String,
}

struct Popup {
    /// For Esc: the most recently opened window closes first.
    opened_at: Instant,
    content: Content,
    request: Request,
    /// The app the text was selected in (for Replace).
    target: Hwnd,
    anchor: Anchor,
    monitor: Monitor,
    above: bool,
    placed_width: f32,
    /// The Wikipedia card was open when the popup was last placed.
    placed_wiki: bool,
    card: Rect,
    frames: u32,
    shown: bool,
    /// When the window appeared (for the slide-in animation).
    shown_at: Option<Instant>,
    /// Window position (physical pixels) once placed.
    pos: Option<(i32, i32)>,
    settled_at: Instant,
    hovered_at: Instant,
    click_through: Option<bool>,
    feedback: Feedback,
}

/// A secondary window (tray menu, settings, intro) we position ourselves.
struct Floating {
    hwnd: Option<Hwnd>,
    shown: bool,
    opened: Instant,
}

impl Floating {
    fn new() -> Self {
        Self { hwnd: None, shown: false, opened: Instant::now() }
    }

    /// Finds the native window after its first frame and shows it at the
    /// position computed by `place` (physical pixels, given the scale).
    fn reveal(&mut self, title: &str, place: impl FnOnce() -> (i32, i32)) {
        if self.shown {
            return;
        }
        if self.hwnd.is_none() {
            self.hwnd = win::find_window(title);
        }
        if let Some(h) = self.hwnd {
            win::strip_chrome(h);
            let (x, y) = place();
            win::move_window(h, x, y);
            win::show_and_focus(h);
            self.shown = true;
            self.opened = Instant::now();
        }
    }
}

/// Events from the secondary windows (they render on their own).
enum UiEvent {
    Tray(TrayAction),
    CloseTray,
    Settings(SettingsAction, Box<Settings>),
    WelcomeDone,
    CloseQuick,
    /// The selection for Ultra mode was read.
    UltraGrabbed { text: String, target: Hwnd },
    Ultra(crate::ui::ultra::UltraAction),
}

#[derive(Default)]
struct TrayModel {
    page: Page,
    /// Language search on the "Translate to" page.
    query: String,
    keys: Vec<String>,
    pair: String,
    target: String,
    paused: bool,
}

struct TrayMenu {
    win: Floating,
    click: (i32, i32),
    model: Arc<Mutex<TrayModel>>,
}

impl TrayMenu {
    fn new(click: (i32, i32)) -> Self {
        Self { win: Floating::new(), click, model: Arc::default() }
    }
}

struct WelcomeWindow {
    win: Floating,
    card_w: f32,
}

pub struct App {
    ctx: egui::Context,
    settings: Settings,
    engine: Arc<Engine>,
    hook_rx: Receiver<HookEvent>,
    work_tx: Sender<WorkMsg>,
    work_rx: Receiver<WorkMsg>,
    tray_rx: Receiver<TrayIconEvent>,
    tray: Option<TrayIcon>,
    ui_tx: Sender<UiEvent>,
    ui_rx: Receiver<UiEvent>,
    popup_hwnd: Hwnd,
    popup: Option<Popup>,
    /// The popup was dismissed; hide the window after one empty frame so a
    /// stale card never flashes on the next show.
    hide_armed: bool,
    first_popup: bool,
    generation: u64,
    ppp: f32,
    last_detected: Option<String>,
    /// The last popup translation failed (tray shows the error state).
    last_failed: bool,
    paused_until: Option<Instant>,
    tray_menu: Option<TrayMenu>,
    tray_menu_closed_at: Option<Instant>,
    settings_window: Option<SettingsWindow>,
    welcome: Option<WelcomeWindow>,
    quick: Option<quick::QuickWindow>,
    ultra: Option<ultra::UltraWindow>,
    /// `--preview` mode: popups stay until dismissed.
    preview: bool,
    /// `--preview trigger`: run the shortcut path once at this time.
    pending_trigger: Option<Instant>,
    /// `--preview quick-warm`: open the (already warm) Quick window then.
    pending_quick: Option<Instant>,
    /// Last cursor position forwarded to the popup (see `popup_frame`).
    last_forwarded: Option<(i32, i32)>,
    /// Main translator window and the screen-area overlay (`app::main_win`).
    main: main_win::MainWin,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, settings: Settings) -> Self {
        let ctx = cc.egui_ctx.clone();
        theme::install_fonts(&ctx);
        theme::install_style(&ctx);
        theme::set_animations(&ctx, settings.animations && settings.hardware_acceleration);
        egui_extras::install_image_loaders(&ctx);

        let popup_hwnd = match cc.window_handle().map(|h| h.as_raw()) {
            Ok(RawWindowHandle::Win32(h)) => Hwnd(h.hwnd.get()),
            _ => Hwnd::default(),
        };
        win::make_tool_popup(popup_hwnd);
        // Shown but off-screen while idle, so wake-ups aren't throttled.
        win::park(popup_hwnd);
        // Every later window (settings, translator, …) is fixed up at creation.
        win::install_chrome_hook();

        let (hook_tx, hook_rx) = channel();
        hook::start(hook_tx, ctx.clone());
        hook::configure(&settings);

        let engine = Engine::new((&settings).into());
        engine.warm_up();

        let (tray_tx, tray_rx) = channel();
        let repaint = ctx.clone();
        TrayIconEvent::set_event_handler(Some(move |e| {
            let _ = tray_tx.send(e);
            repaint.request_repaint();
        }));
        let tray = Icon::from_rgba(icons::tray_icon_rgba(icons::TrayState::Active, 32), 32, 32)
            .ok()
            .and_then(|icon| {
                TrayIconBuilder::new()
                    .with_icon(icon)
                    .with_tooltip(tooltip(&settings.shortcut, false))
                    .build()
                    .ok()
            });

        let (work_tx, work_rx) = channel();
        let (ui_tx, ui_rx) = channel();
        let welcome = (!settings.onboarded).then(|| WelcomeWindow { win: Floating::new(), card_w: 0.0 });
        let main = main_win::MainWin::new(&settings.target);

        Self {
            ctx,
            settings,
            engine,
            hook_rx,
            work_tx,
            work_rx,
            tray_rx,
            tray,
            ui_tx,
            ui_rx,
            popup_hwnd,
            popup: None,
            hide_armed: false,
            first_popup: true,
            generation: 0,
            ppp: 1.0,
            last_detected: None,
            last_failed: false,
            paused_until: None,
            tray_menu: None,
            tray_menu_closed_at: None,
            settings_window: None,
            welcome,
            quick: None,
            ultra: None,
            preview: false,
            last_forwarded: None,
            pending_trigger: None,
            pending_quick: None,
            main,
        }
    }

    fn paused(&self) -> bool {
        self.paused_until.is_some_and(|t| t > Instant::now())
    }

    fn apply_settings(&mut self) {
        hook::configure(&self.settings);
        crate::i18n::set_lang(&self.settings.ui_lang);
        crate::history::set_enabled(self.settings.save_history);
        theme::set_animations(&self.ctx, self.settings.animations && self.settings.hardware_acceleration);
        crate::stats::set_enabled(self.settings.keep_stats);
        crate::wiki::set_enabled(self.settings.show_wiki);
        self.engine.configure((&self.settings).into());
        self.settings.save();
        self.update_tooltip();
    }

    /// Tray tooltip and icon state (active / paused / last translation failed).
    fn update_tooltip(&self) {
        if let Some(t) = &self.tray {
            let _ = t.set_tooltip(Some(tooltip(&self.settings.shortcut, self.paused())));
            let state = if self.paused() {
                icons::TrayState::Paused
            } else if self.last_failed {
                icons::TrayState::Error
            } else {
                icons::TrayState::Active
            };
            if let Ok(icon) = Icon::from_rgba(icons::tray_icon_rgba(state, 32), 32, 32) {
                let _ = t.set_icon(Some(icon));
            }
        }
    }

    fn lang_pair(&self) -> String {
        let from = if self.settings.source == "auto" {
            self.last_detected
                .as_deref()
                .map(lang_name)
                .unwrap_or_else(|| crate::i18n::tr("Detect").to_owned())
        } else {
            lang_name(&self.settings.source)
        };
        format!("{from} → {}", lang_name(&self.settings.target))
    }

    // ---------------------------------------------------------------- events

    fn pump(&mut self) {
        while let Ok(ev) = self.hook_rx.try_recv() {
            self.on_hook(ev);
        }
        while let Ok(msg) = self.work_rx.try_recv() {
            self.on_work(msg);
        }
        while let Ok(ev) = self.tray_rx.try_recv() {
            self.on_tray(ev);
        }
        while let Ok(ev) = self.ui_rx.try_recv() {
            self.on_ui(ev);
        }
        if let Some(at) = self.pending_quick {
            if Instant::now() >= at {
                self.pending_quick = None;
                self.open_quick();
                if let Some(q) = &self.quick {
                    q.preview("Where is the nearest pharmacy?");
                }
            } else {
                self.ctx.request_repaint_after(at - Instant::now());
            }
        }
        if let Some(at) = self.pending_trigger {
            if Instant::now() >= at {
                self.pending_trigger = None;
                self.trigger(None);
            } else {
                self.ctx.request_repaint_after(at - Instant::now());
            }
        }
        if let Some(until) = self.paused_until {
            let now = Instant::now();
            if until <= now {
                self.paused_until = None;
                hook::PAUSED.store(false, Ordering::Relaxed);
                self.update_tooltip();
            } else {
                self.ctx.request_repaint_after(until - now);
            }
        }
    }

    fn on_hook(&mut self, ev: HookEvent) {
        match ev {
            HookEvent::Trigger(slot) => self.on_hotkey(slot),
            HookEvent::Escape => self.on_escape(),
            HookEvent::PopupCopy => self.popup_ctrl_c(),
            HookEvent::MouseDown { x, y } => {
                if self.popup.as_ref().is_some_and(|p| p.shown && !self.card_contains(p, x, y)) {
                    self.hide_popup();
                }
            }
            HookEvent::Captured(combo) => self.on_captured(combo),
        }
    }

    fn on_hotkey(&mut self, slot: HotkeySlot) {
        match slot {
            HotkeySlot::Translate => self.trigger(None),
            HotkeySlot::Quick => {
                crate::timing("app: hotkey Quick");
                self.open_quick();
            }
            HotkeySlot::Ultra => self.start_ultra(),
            HotkeySlot::ScreenArea => self.start_screen_capture(),
            HotkeySlot::WholeScreen => self.capture_whole_screen(),
            HotkeySlot::MainWindow => self.open_main_window(),
            HotkeySlot::TranslatePaste => self.translate_paste(),
            HotkeySlot::CopyTwice => self.trigger_with(None, Grab::Clipboard),
        }
    }

    /// Ctrl+C with the popup open: copy the translation, and close the
    /// popup unless the setting says "copy only".
    fn popup_ctrl_c(&mut self) {
        let Some(text) = self.popup.as_ref().and_then(|p| p.content.copy_text()) else { return };
        if self.settings.popup_ctrl_c == crate::settings::PopupCtrlC::CopyAndClose {
            self.hide_popup();
            std::thread::spawn(move || clipboard::set_text(&text, false));
        } else {
            self.on_action(Action::Copy(text, popup::CopyKind::All));
        }
    }

    /// Esc (from anywhere): the most recently opened of the popup, Quick
    /// and Ultra windows handles it.
    fn on_escape(&mut self) {
        let popup = self.popup.as_ref().map(|p| p.opened_at);
        let quick = self.quick_open_since();
        let ultra = self.ultra.as_ref().map(|u| u.opened);
        let latest = [popup, quick, ultra].into_iter().flatten().max();
        match latest {
            Some(t) if Some(t) == ultra => self.ultra_escape(),
            Some(t) if Some(t) == quick => self.quick_escape(),
            Some(_) => self.hide_popup(),
            None => {}
        }
        self.update_esc();
    }

    /// Tells the hook whether Esc has something of ours to close.
    fn update_esc(&self) {
        let open = self.popup.is_some() || self.quick_open_since().is_some() || self.ultra.is_some();
        hook::ESC_CLOSES.store(open, Ordering::Relaxed);
    }

    fn on_tray(&mut self, ev: TrayIconEvent) {
        if let TrayIconEvent::Click { button_state: MouseButtonState::Up, position, .. } = ev {
            // A click on the icon while the menu is open first defocuses
            // (and closes) the menu; don't immediately reopen it.
            let just_closed = self
                .tray_menu_closed_at
                .is_some_and(|t| t.elapsed() < Duration::from_millis(350));
            if self.tray_menu.is_some() {
                self.tray_menu = None;
            } else if !just_closed {
                self.hide_popup();
                self.tray_menu = Some(TrayMenu::new((position.x as i32, position.y as i32)));
            }
        }
    }

    fn on_work(&mut self, msg: WorkMsg) {
        match msg {
            WorkMsg::Grabbed { generation, text, anchor, target, cached } => {
                if generation != self.generation {
                    return;
                }
                let content = match cached {
                    Some(t) => {
                        self.on_result(&t);
                        self.view_for(t)
                    }
                    None => Content::Loading {
                        compact: self.settings.popup_style == crate::settings::PopupStyle::Compact,
                        source: text.clone(),
                    },
                };
                let request = Request {
                    text,
                    from: self.settings.source.clone(),
                    to: self.settings.target.clone(),
                };
                self.open_popup(content, request, anchor, target);
            }
            WorkMsg::Done { generation, result } => {
                if generation != self.generation {
                    return;
                }
                let content = match result {
                    Ok(t) => {
                        self.last_detected = t.src_lang.clone();
                        if self.last_failed {
                            self.last_failed = false;
                            self.update_tooltip();
                        }
                        self.on_result(&t);
                        self.view_for(t)
                    }
                    Err(error) => match &self.popup {
                        Some(_) if !self.last_failed => {
                            self.last_failed = true;
                            self.update_tooltip();
                            let p = self.popup.as_ref().unwrap();
                            Content::Error { source: p.request.text.clone(), error, to: p.request.to.clone() }
                        }
                        Some(p) => Content::Error { source: p.request.text.clone(), error, to: p.request.to.clone() },
                        None => return,
                    },
                };
                if let Some(p) = &mut self.popup {
                    p.content = content;
                    p.settled_at = Instant::now();
                }
            }
            WorkMsg::Variant { generation, provider, text } => {
                if generation != self.generation {
                    return;
                }
                if let Some(Popup { content: Content::Compact(v), .. }) = &mut self.popup {
                    v.set_variant(provider, text);
                }
            }
            WorkMsg::Pasted(t) => {
                self.last_detected = t.src_lang.clone();
                if self.last_failed {
                    self.last_failed = false;
                    self.update_tooltip();
                }
                let from = t.src_lang.as_deref().unwrap_or("auto");
                crate::history::add(&t.source, &t.text, from, &t.tgt_lang, t.provider, crate::history::Mode::Popup);
            }
            WorkMsg::PasteFailed { text, error, anchor, target } => {
                if !self.last_failed {
                    self.last_failed = true;
                    self.update_tooltip();
                }
                let to = self.settings.target.clone();
                let request = Request { text: text.clone(), from: self.settings.source.clone(), to: to.clone() };
                self.open_popup(Content::Error { source: text, error, to }, request, anchor, target);
            }
        }
    }

    /// A popup translation arrived: read it aloud if asked, keep it in history.
    fn on_result(&self, t: &Translation) {
        if self.settings.speak_results {
            speech::speak(&t.text, &t.tgt_lang);
        }
        let from = t.src_lang.as_deref().unwrap_or("auto");
        crate::history::add(&t.source, &t.text, from, &t.tgt_lang, t.provider, crate::history::Mode::Popup);
    }

    /// Compact style shows words and sentences alike as the compact pill,
    /// with the other services' translations when enabled.
    fn view_for(&self, t: Arc<Translation>) -> Content {
        if self.settings.popup_style == crate::settings::PopupStyle::Compact {
            let style = self.settings.compact_variants;
            let others = if style == crate::settings::CompactVariants::Off {
                Vec::new()
            } else {
                self.engine.variant_providers(t.provider, crate::settings::COMPACT_VARIANTS_MAX)
            };
            self.fetch_variants(&t, &others);
            Content::Compact(popup::CompactView::new(t, style, &others))
        } else if !t.is_word {
            Content::Sentence(SentenceView::new(t))
        } else {
            Content::Word(WordView::new(t))
        }
    }

    /// Asks each of `others` for its own translation of `t`, in parallel;
    /// results arrive as [`WorkMsg::Variant`] for the current popup.
    fn fetch_variants(&self, t: &Translation, others: &[ProviderKind]) {
        let generation = self.generation;
        let from = t.src_lang.clone().unwrap_or_else(|| "auto".into());
        for &provider in others {
            let (engine, tx, ctx) = (self.engine.clone(), self.work_tx.clone(), self.ctx.clone());
            let (text, from, to) = (t.source.clone(), from.clone(), t.tgt_lang.clone());
            std::thread::spawn(move || {
                let text = engine.translate_by(provider, &text, &from, &to).ok().map(|t| t.text);
                let _ = tx.send(WorkMsg::Variant { generation, provider, text });
                ctx.request_repaint();
            });
        }
    }

    /// Reads the selection of the foreground app on a worker thread and
    /// translates it. `refocus` = bring this window to the front first
    /// (used by the tray menu, which has taken focus from the app).
    fn trigger(&mut self, refocus: Option<Hwnd>) {
        self.trigger_with(refocus, Grab::Selection);
    }

    /// [`App::trigger`] with the text taken from `grab`.
    fn trigger_with(&mut self, refocus: Option<Hwnd>, grab: Grab) {
        if self.paused() {
            return;
        }
        // Ctrl+C ×2: the app is putting the selection on the clipboard right
        // now; wait for the clipboard to change from this point.
        let seq_before = clipboard::sequence();
        self.generation += 1;
        let generation = self.generation;
        let engine = self.engine.clone();
        let tx = self.work_tx.clone();
        let ctx = self.ctx.clone();
        let (from, to) = (self.settings.source.clone(), self.settings.target.clone());
        let programmer = self.settings.programmer_mode;
        engine.warm_up();

        std::thread::spawn(move || {
            let send = |m: WorkMsg| {
                let _ = tx.send(m);
                ctx.request_repaint();
            };
            if let Some(h) = refocus.filter(|h| !h.is_null()) {
                win::focus(h);
                std::thread::sleep(Duration::from_millis(150));
            }
            let target = win::foreground();
            let anchor = win::caret_anchor().unwrap_or_else(win::cursor_anchor);
            let raw = match grab {
                Grab::Selection => clipboard::copy_selection(target, COPY_TIMEOUT),
                Grab::Clipboard => read_copied(seq_before),
            };
            let Some(raw) = raw else {
                return;
            };
            // Programmer mode keeps code exactly as selected (indentation
            // included) and translates only its strings and comments.
            let code = programmer && crate::code::is_code(&raw);
            let text = if code {
                raw.chars().take(translate::MAX_CHARS).collect::<String>().trim_end().to_owned()
            } else {
                translate::normalize(&raw)
            };
            if text.trim().is_empty() {
                return;
            }
            let cached = engine.cached(&text, &from, &to);
            let done = cached.is_some();
            send(WorkMsg::Grabbed { generation, text: text.clone(), anchor, target, cached });
            if !done {
                let result = match code.then(|| crate::code::translate(&engine, &text, &from, &to, None)).flatten() {
                    Some(r) => r,
                    None => engine.translate(&text, &from, &to),
                };
                send(WorkMsg::Done { generation, result });
            }
        });
    }

    /// Translate and paste: copies the selection, translates it and pastes
    /// the translation over it. No window opens; only a failure shows the
    /// popup (with the reason and Retry).
    fn translate_paste(&mut self) {
        if self.paused() {
            return;
        }
        let (engine, tx, ctx) = (self.engine.clone(), self.work_tx.clone(), self.ctx.clone());
        let (from, to) = (self.settings.source.clone(), self.settings.target.clone());
        let programmer = self.settings.programmer_mode;
        engine.warm_up();
        std::thread::spawn(move || {
            let send = |m: WorkMsg| {
                let _ = tx.send(m);
                ctx.request_repaint();
            };
            let target = win::foreground();
            let anchor = win::caret_anchor().unwrap_or_else(win::cursor_anchor);
            let Some(raw) = clipboard::copy_selection(target, COPY_TIMEOUT) else {
                return;
            };
            // Translate the text inside the selection's outer whitespace and
            // put that whitespace back, so the paste fits where it was.
            let core = raw.trim();
            if core.is_empty() {
                return;
            }
            let lead = &raw[..raw.len() - raw.trim_start().len()];
            let trail = &raw[raw.trim_end().len()..];
            let text: String = core.chars().take(translate::MAX_CHARS).collect();
            let code = programmer && crate::code::is_code(&text);
            let result = match code.then(|| crate::code::translate(&engine, &text, &from, &to, None)).flatten() {
                Some(r) => r,
                None => engine.translate(&text, &from, &to),
            };
            match result {
                Ok(t) => {
                    clipboard::paste_text(target, &format!("{lead}{}{trail}", t.text));
                    send(WorkMsg::Pasted(t));
                }
                Err(error) => send(WorkMsg::PasteFailed { text, error, anchor, target }),
            }
        });
    }

    fn retry(&mut self) {
        let Some(p) = &mut self.popup else { return };
        self.generation += 1;
        let generation = self.generation;
        p.content = Content::Loading { source: p.request.text.clone(), compact: false };
        let (text, from, to) = (p.request.text.clone(), p.request.from.clone(), p.request.to.clone());
        let (engine, tx, ctx) = (self.engine.clone(), self.work_tx.clone(), self.ctx.clone());
        std::thread::spawn(move || {
            let result = engine.translate(&text, &from, &to);
            let _ = tx.send(WorkMsg::Done { generation, result });
            ctx.request_repaint();
        });
    }

    // ----------------------------------------------------------------- popup

    fn open_popup(&mut self, content: Content, request: Request, anchor: Anchor, target: Hwnd) {
        // Re-triggered while visible: hide first so the old card can't flash
        // at the new position.
        if !win::is_parked(self.popup_hwnd) {
            win::park(self.popup_hwnd);
        }
        self.hide_armed = false;
        let now = Instant::now();
        let mut p = Popup {
            opened_at: now,
            content,
            request,
            target,
            anchor,
            monitor: win::monitor_at(anchor.x, anchor.bottom),
            above: false,
            placed_width: 0.0,
            placed_wiki: false,
            card: Rect::NOTHING,
            frames: 0,
            shown: false,
            shown_at: None,
            pos: None,
            settled_at: now,
            hovered_at: now,
            click_through: None,
            feedback: Feedback::default(),
        };
        self.place(&mut p);
        self.popup = Some(p);
        self.ctx.request_repaint();
    }

    fn hide_popup(&mut self) {
        if self.popup.take().is_some() {
            hook::POPUP_VISIBLE.store(false, Ordering::Relaxed);
            self.generation += 1;
            self.ctx.request_repaint();
        }
    }

    /// Positions the popup window so the card sits just below the anchor
    /// (or above it near the bottom of the screen), clamped to the monitor.
    fn place(&self, p: &mut Popup) {
        let s = p.monitor.scale;
        let work = p.monitor.work;
        let card_w = p
            .content
            .card_width()
            .unwrap_or(if p.card.is_positive() { p.card.width() } else { 160.0 });
        let card_h = p.content.estimated_height();
        let gap = 6.0 * s;
        let edge = 8.0 * s;

        let left = (p.anchor.x as f32 - 18.0 * s)
            .min(work.right as f32 - edge - card_w * s)
            .max(work.left as f32 + edge);
        let below_top = p.anchor.bottom as f32 + gap;
        let fits_below = below_top + card_h * s <= work.bottom as f32 - edge;
        let fits_above = p.anchor.top as f32 - gap - card_h * s >= work.top as f32 + edge;
        p.above = !fits_below && fits_above;

        let x = left - POPUP_MX * s;
        let y = if p.above {
            p.anchor.top as f32 - gap - (POPUP_H - POPUP_MB) * s
        } else {
            below_top - POPUP_MT * s
        };
        // Until the card is drawn the window stays parked; it moves into
        // place when shown (see `popup_frame`).
        p.pos = Some((x.round() as i32, y.round() as i32));
        if p.shown {
            win::move_window(self.popup_hwnd, x.round() as i32, y.round() as i32);
        }
        p.placed_width = card_w;
        p.placed_wiki = p.content.wiki_open();
    }

    fn card_contains(&self, p: &Popup, x: i32, y: i32) -> bool {
        let r = win::window_rect(self.popup_hwnd);
        let pt = pos2((x - r.left) as f32 / self.ppp, (y - r.top) as f32 / self.ppp);
        p.card.expand(4.0).contains(pt)
    }

    fn popup_frame(&mut self, ctx: &egui::Context) {
        let Some(mut p) = self.popup.take() else {
            if !win::is_parked(self.popup_hwnd) {
                if self.hide_armed {
                    win::park(self.popup_hwnd);
                    self.hide_armed = false;
                } else {
                    self.hide_armed = true;
                    ctx.request_repaint();
                }
            }
            return;
        };

        let (mut origin, pivot) = if p.above {
            (pos2(POPUP_MX, POPUP_H - POPUP_MB), Align2::LEFT_BOTTOM)
        } else {
            (pos2(POPUP_MX, POPUP_MT), Align2::LEFT_TOP)
        };
        // Slide in 6px towards the selection (Settings → Animations).
        let anim = ctx.style_of(egui::Theme::Dark).animation_time;
        if anim > 0.0
            && let Some(at) = p.shown_at
        {
            let t = (at.elapsed().as_secs_f32() / anim).min(1.0);
            let ease = 1.0 - (1.0 - t).powi(3);
            let dir = if p.above { 1.0 } else { -1.0 };
            origin.y += dir * 6.0 * (1.0 - ease);
            if t < 1.0 {
                ctx.request_repaint();
            }
        }
        let (action, card) = popup::show(ctx, &mut p.content, origin, pivot, &p.feedback);
        p.card = card;
        p.frames += 1;

        // Card width changed (loading → result, or measured compact pill):
        // re-place, and don't reveal a frame drawn at the old position.
        let width = p.content.card_width().unwrap_or(card.width());
        let mut moved = false;
        if (width - p.placed_width).abs() > 1.0 || p.content.wiki_open() != p.placed_wiki {
            let was_above = p.above;
            self.place(&mut p);
            moved = !p.shown || was_above != p.above;
        }

        let needed = if self.first_popup { 2 } else { 1 };
        if !p.shown {
            if p.frames >= needed && !moved {
                win::make_tool_popup(self.popup_hwnd);
                if let Some((x, y)) = p.pos {
                    win::move_window(self.popup_hwnd, x, y);
                }
                win::show_no_activate(self.popup_hwnd);
                hook::POPUP_VISIBLE.store(true, Ordering::Relaxed);
                p.shown = true;
                p.shown_at = Some(Instant::now());
                self.first_popup = false;
            } else {
                ctx.request_repaint();
            }
        }

        // Click-through outside the card; hovering keeps it open.
        let mut close = false;
        if p.shown {
            let (x, y) = win::cursor_pos();
            let over = self.card_contains(&p, x, y);
            if p.click_through != Some(!over) {
                win::set_click_through(self.popup_hwnd, !over);
                p.click_through = Some(!over);
            }
            if over {
                p.hovered_at = Instant::now();
                // While click-through (or if the cursor sat still as the popup
                // appeared under it) the window got no mouse-move, and
                // egui-winit drops clicks without a known position.
                if self.last_forwarded != Some((x, y)) {
                    win::post_mouse_move(self.popup_hwnd, x, y);
                    self.last_forwarded = Some((x, y));
                }
            }
            let hide_after = self.settings.hide_after_secs;
            if hide_after > 0 && !self.preview && p.content.is_settled() && !p.feedback.active() {
                let last = p.settled_at.max(p.hovered_at);
                close = last.elapsed() > Duration::from_secs(hide_after as u64);
            }
        }
        // Polls the cursor for click-through and the auto-hide timer.
        ctx.request_repaint_after(Duration::from_millis(50));

        self.popup = Some(p);
        if close {
            self.hide_popup();
        }
        if let Some(a) = action {
            self.on_action(a);
        }
    }

    fn on_action(&mut self, action: Action) {
        match action {
            Action::Copy(text, kind) => {
                if let Some(p) = &mut self.popup {
                    p.feedback.copied(kind);
                    p.settled_at = Instant::now();
                }
                std::thread::spawn(move || clipboard::set_text(&text, false));
            }
            Action::Replace(text) => {
                let target = self.popup.as_ref().map(|p| p.target).unwrap_or_default();
                self.hide_popup();
                std::thread::spawn(move || {
                    std::thread::sleep(Duration::from_millis(40));
                    clipboard::paste_text(target, &text);
                });
            }
            Action::Speak(text, lang) => speech::speak(&text, &lang),
            Action::Retry => self.retry(),
        }
    }

    // ------------------------------------------------------------- tray menu

    fn tray_menu_frame(&mut self, ctx: &egui::Context) {
        let Some(mut menu) = self.tray_menu.take() else { return };
        {
            let mut m = menu.model.lock().unwrap();
            m.keys = self.settings.shortcut.keys();
            m.pair = self.lang_pair();
            m.target = self.settings.target.clone();
            m.paused = self.paused();
        }
        let builder = ViewportBuilder::default()
            .with_title(TRAY_TITLE)
            .with_inner_size(tray_menu::WINDOW)
            .with_decorations(false)
            .with_transparent(true)
            .with_resizable(false)
            .with_always_on_top()
            .with_taskbar(false)
            .with_visible(false);
        let (model, tx) = (menu.model.clone(), self.ui_tx.clone());
        ctx.show_viewport_deferred(ViewportId::from_hash_of("qt-tray-menu"), builder, move |ui, _| {
            let mut guard = model.lock().unwrap();
            let m = &mut *guard;
            let view = TrayView { keys: m.keys.clone(), pair: m.pair.clone(), target: &m.target, paused: m.paused };
            let action = tray_menu::show(ui, &mut m.page, &mut m.query, &view);
            let escape = ui.input(|i| i.key_pressed(egui::Key::Escape));
            if let Some(a) = action {
                send_ui(ui.ctx(), &tx, UiEvent::Tray(a));
            } else if escape {
                send_ui(ui.ctx(), &tx, UiEvent::CloseTray);
            }
        });

        let (cx, cy) = menu.click;
        menu.win.reveal(TRAY_TITLE, || {
            let m = win::monitor_at(cx, cy);
            let s = m.scale;
            let (card_w, card_h) = (tray_menu::CARD_W * s, tray_menu::CARD_H * s);
            let right = (cx as f32 + 40.0 * s).min(m.work.right as f32 - 8.0 * s);
            let top = if cy >= m.work.bottom - 1 {
                m.work.bottom as f32 - 8.0 * s - card_h
            } else if cy <= m.work.top {
                m.work.top as f32 + 8.0 * s
            } else {
                (cy as f32 - card_h).max(m.work.top as f32 + 8.0 * s)
            };
            (
                (right - card_w - tray_menu::MARGIN_X * s) as i32,
                (top - tray_menu::MARGIN_TOP * s) as i32,
            )
        });

        let lost_focus = !self.preview
            && menu.win.shown
            && menu.win.opened.elapsed() > Duration::from_millis(250)
            && menu.win.hwnd.is_some_and(|h| win::foreground() != h);
        if lost_focus {
            self.tray_menu_closed_at = Some(Instant::now());
        } else {
            self.tray_menu = Some(menu);
            ctx.request_repaint_after(Duration::from_millis(50));
        }
    }

    fn on_ui(&mut self, ev: UiEvent) {
        match ev {
            UiEvent::CloseTray => self.tray_menu = None,
            UiEvent::Tray(action) => {
                if !matches!(action, TrayAction::SetTarget(_)) {
                    self.tray_menu = None;
                }
                match action {
                    TrayAction::TranslateSelection => {
                        let last = Hwnd(hook::LAST_APP.load(Ordering::Relaxed));
                        self.trigger(Some(last));
                    }
                    TrayAction::SetTarget(code) => {
                        self.settings.target = code;
                        self.apply_settings();
                    }
                    TrayAction::TogglePause => {
                        self.paused_until = if self.paused() {
                            None
                        } else {
                            Some(Instant::now() + Duration::from_secs(3600))
                        };
                        hook::PAUSED.store(self.paused(), Ordering::Relaxed);
                        self.update_tooltip();
                    }
                    TrayAction::OpenSettings => self.open_settings(),
                    TrayAction::OpenTranslator => self.open_main_window(),
                    TrayAction::OpenHistory => self.open_history(),
                    TrayAction::Quit => {
                        crate::offline::shutdown();
                        self.ctx.send_viewport_cmd_to(ViewportId::ROOT, ViewportCommand::Close);
                    }
                }
            }
            UiEvent::Settings(action, edited) => self.on_settings_action(action, *edited),
            UiEvent::CloseQuick => self.close_quick(),
            UiEvent::UltraGrabbed { text, target } => self.open_ultra(&text, target),
            UiEvent::Ultra(action) => self.on_ultra_action(action),
            UiEvent::WelcomeDone => {
                self.welcome = None;
                self.settings.onboarded = true;
                self.settings.save();
            }
        }
    }

    // ----------------------------------------------------------------- intro

    fn welcome_frame(&mut self, ctx: &egui::Context) {
        let Some(w) = &mut self.welcome else { return };
        if w.card_w == 0.0 {
            let (x, y) = win::cursor_pos();
            let m = win::monitor_at(x, y);
            let work_w = (m.work.right - m.work.left) as f32 / m.scale;
            w.card_w = welcome::MAX_CARD_W.min(work_w - 80.0).max(760.0);
        }
        let size = welcome::window_size(w.card_w);
        let builder = ViewportBuilder::default()
            .with_title(WELCOME_TITLE)
            .with_icon(window_icon())
            .with_inner_size(size)
            .with_decorations(false)
            .with_transparent(true)
            .with_resizable(false)
            .with_visible(false);
        let (card_w, shortcut, tx) = (w.card_w, self.settings.shortcut, self.ui_tx.clone());
        ctx.show_viewport_deferred(ViewportId::from_hash_of("qt-welcome"), builder, move |ui, _| {
            let dismissed = welcome::show(ui, card_w, &shortcut);
            if dismissed || ui.input(|i| i.viewport().close_requested()) {
                send_ui(ui.ctx(), &tx, UiEvent::WelcomeDone);
            }
        });
        w.win.reveal(WELCOME_TITLE, || centered(size));
    }
}

/// Sends an event from a secondary window to the app and wakes the root.
fn send_ui(ctx: &egui::Context, tx: &Sender<UiEvent>, ev: UiEvent) {
    let _ = tx.send(ev);
    ctx.request_repaint_of(ViewportId::ROOT);
}

impl App {
    /// Shows one surface with sample data from the design, for visual checks:
    /// `helilingo --preview word|sentence|compact|loading|offline|settings|tray|languages|welcome|trigger`.
    pub fn preview(&mut self, what: &str) {
        use crate::translate::DictGroup;
        self.preview = true;
        self.welcome = None;
        // Sample translations don't belong in the user's history.
        crate::history::set_enabled(false);
        crate::stats::set_enabled(false);
        let word = Arc::new(Translation {
            source: "effectively".into(),
            text: "эффективно".into(),
            src_lang: Some("en".into()),
            tgt_lang: "ru".into(),
            is_word: true,
            provider: crate::settings::ProviderKind::Google,
            dict: vec![
                DictGroup {
                    pos: "adverb".into(),
                    terms: ["эффективно", "результативно", "действенно", "эффектно", "продуктивно"]
                        .map(String::from)
                        .to_vec(),
                },
                DictGroup {
                    pos: "adjective".into(),
                    terms: ["фактически", "по существу"].map(String::from).to_vec(),
                },
            ],
        });
        let sentence = Arc::new(Translation {
            source: "Our teams worked together far more effectively last quarter than anyone had expected.".into(),
            text: "Наши команды работали вместе гораздо эффективнее в прошлом квартале, чем кто-либо ожидал.".into(),
            src_lang: Some("en".into()),
            tgt_lang: "ru".into(),
            is_word: false,
            dict: Vec::new(),
            provider: crate::settings::ProviderKind::Google,
        });
        let content = match what {
            "word" => Content::Word(WordView::new(word.clone())),
            // The Word popup with the Wikipedia card open (fetched live).
            "wiki" => {
                let car = Translation {
                    source: "car".into(),
                    text: "автомобиль".into(),
                    dict: vec![DictGroup {
                        pos: "noun".into(),
                        terms: ["автомобиль", "машина", "вагон"].map(String::from).to_vec(),
                    }],
                    ..(*word).clone()
                };
                let mut v = WordView::new(Arc::new(car));
                v.open_wiki();
                Content::Word(v)
            }
            "sentence" => Content::Sentence(SentenceView::new(sentence.clone())),
            // Sample other services' answers (no requests in a preview).
            "compact" | "compact-sentence" | "compact-menu" => {
                use crate::settings::CompactVariants;
                let style = if what == "compact-menu" { CompactVariants::Menu } else { CompactVariants::List };
                let t = if what == "compact-sentence" { sentence.clone() } else { word.clone() };
                let others = [ProviderKind::Yandex, ProviderKind::DeepL, ProviderKind::Nllb600];
                let mut v = popup::CompactView::new(t, style, &others);
                let samples: [Option<&str>; 3] = if what == "compact-sentence" {
                    [Some("Наши команды работали гораздо эффективнее в прошлом квартале, чем кто-либо ожидал."), None, Some("Наши команды сотрудничали эффективнее, чем ожидалось.")]
                } else {
                    [Some("результативно"), Some("действенно"), None]
                };
                for (p, text) in others.into_iter().zip(samples) {
                    if let Some(text) = text {
                        v.set_variant(p, Some(text.to_owned()));
                    }
                }
                Content::Compact(v)
            }
            "loading" => Content::Loading { source: "effectively".into(), compact: false },
            "offline" => Content::Error { source: "effectively".into(), error: Error::Offline, to: "ru".into() },
            s if self.preview_settings(s) => return,
            "ultra" => {
                let sample = "Please send me the report by Friday. Die Besprechung wurde auf Montag verschoben.
Let’s keep the same terms as before. Gracias por tu ayuda.
Это уже по-русски.";
                return self.open_ultra(sample, Hwnd::default());
            }
            "quick-warm" => {
                self.warm_quick();
                self.pending_quick = Some(Instant::now() + Duration::from_secs(2));
                return;
            }
            "quick" => {
                self.open_quick();
                if let Some(q) = &self.quick {
                    q.preview("Where is the nearest pharmacy?");
                }
                return;
            }
            "main" | "main-image" | "main-image-result" | "main-image-zoom" | "main-history" | "capture" | "capture-window" => {
                return self.preview_main(what);
            }
            // Same path as the keyboard shortcut, for testing with a real selection.
            "trigger" => {
                self.pending_trigger = Some(Instant::now() + Duration::from_millis(1500));
                return;
            }
            "welcome" => {
                self.welcome = Some(WelcomeWindow { win: Floating::new(), card_w: 0.0 });
                return;
            }
            "tray" | "languages" => {
                let m = win::monitor_at(0, 0);
                let menu = TrayMenu::new((m.work.right - 90, m.work.bottom + 10));
                if what == "languages" {
                    let mut model = menu.model.lock().unwrap();
                    model.page = Page::Languages;
                    model.query = "an".into();
                }
                self.tray_menu = Some(menu);
                return;
            }
            _ => return,
        };
        let m = win::monitor_at(0, 0);
        let (x, y) = ((m.work.left + m.work.right) / 2 - 150, (m.work.top + m.work.bottom) / 2 - 150);
        let anchor = Anchor { x, top: y - 20, bottom: y };
        let request = Request { text: sentence.source.clone(), from: "auto".into(), to: "ru".into() };
        self.open_popup(content, request, anchor, Hwnd::default());
    }
}

/// Ctrl+C ×2: the text the user just copied. The app handles its Ctrl+C
/// after our hook has seen it, so wait (briefly) for the clipboard to
/// change; if it doesn't (the same text copied twice), take what is there.
fn read_copied(seq_before: u32) -> Option<String> {
    let start = Instant::now();
    while clipboard::sequence() == seq_before && start.elapsed() < Duration::from_millis(250) {
        std::thread::sleep(Duration::from_millis(8));
    }
    clipboard::get_text().filter(|t| !t.trim().is_empty())
}

/// Top-left (physical) that centres a window of `size` points on the
/// monitor under the cursor.
fn centered(size: [f32; 2]) -> (i32, i32) {
    let (x, y) = win::cursor_pos();
    let m = win::monitor_at(x, y);
    let (w, h) = (size[0] * m.scale, size[1] * m.scale);
    let cx = (m.work.left + m.work.right) as f32 / 2.0;
    let cy = (m.work.top + m.work.bottom) as f32 / 2.0;
    ((cx - w / 2.0) as i32, (cy - h / 2.0) as i32)
}

fn tooltip(shortcut: &Shortcut, paused: bool) -> String {
    if paused {
        crate::i18n::tr("HeliLingo — paused").to_owned()
    } else {
        crate::i18n::trf("HeliLingo — {s} to translate", &[("s", &shortcut.label())])
    }
}

impl eframe::App for App {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0; 4]
    }

    fn logic(&mut self, _ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.pump();
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        if self.quick_opening() {
            crate::timing("root: frame");
        }
        let ctx = ui.ctx().clone();
        self.ppp = ctx.pixels_per_point();
        self.pump();
        self.popup_frame(&ctx);
        self.tray_menu_frame(&ctx);
        self.settings_frame(&ctx);
        self.welcome_frame(&ctx);
        self.update_esc();
        // The Quick window exists from the start (hidden), so it opens fast.
        if !self.preview || self.quick.is_some() {
            self.warm_quick();
        }
        self.quick_frame(&ctx);
        self.ultra_frame(&ctx);
        self.main_window_frame(&ctx);
        self.capture_frame(&ctx);
    }
}
