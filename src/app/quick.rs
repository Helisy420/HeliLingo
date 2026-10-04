//! The Quick window's lifecycle: opened by its shortcut, translates as the
//! user types (debounced), closes on Esc, Enter (after copying) or when it
//! loses focus.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use egui::{ViewportBuilder, ViewportCommand, ViewportId};

use super::{App, Floating, UiEvent, send_ui, window_icon};
use crate::history::{self, Mode};
use crate::settings::{ProviderKind, Settings};
use crate::translate::Engine;
use crate::ui::quick::{self, QuickAction, QuickState, QuickView};
use crate::win::{self, clipboard, speech};

const QUICK_TITLE: &str = "HeliLingo — quick window";
/// Pause after the last keystroke before translating.
const DEBOUNCE: Duration = Duration::from_millis(300);

fn viewport_id() -> ViewportId {
    ViewportId::from_hash_of("qt-quick")
}

/// The Quick window is created once when the app starts and parked
/// off-screen between uses (see `win::park`: hidden windows repaint at most
/// every 100 ms). On open it draws a frame, then moves itself into place
/// from that frame, without waiting for the app's own frame; the open
/// animation starts on the first frame after that.
pub(super) struct QuickWindow {
    win: Floating,
    model: Arc<Mutex<QuickModel>>,
}

struct QuickModel {
    /// Shown to the user (else kept hidden, warm, for the next open).
    open: bool,
    /// The native window was shown for this open.
    shown: bool,
    /// The native window (known once created).
    hwnd: Option<win::Hwnd>,
    state: QuickState,
    engine: Arc<Engine>,
    providers: Vec<ProviderKind>,
    keys: Vec<String>,
    speak: bool,
    programmer: bool,
    /// Open animation on (app setting and Windows' "Show animations").
    animate: bool,
    /// Text, languages and provider of the last request sent.
    sent: Option<(String, String, String, Option<ProviderKind>)>,
    last_input: String,
    changed_at: Instant,
    generation: u64,
    /// The window had focus at least once (so losing it means "click away").
    was_focused: bool,
    opened: Instant,
    /// Window size last requested, to resize only on change.
    size: [f32; 2],
}

/// Providers the chip offers: enabled and usable, in settings order.
fn usable_providers(s: &Settings) -> Vec<ProviderKind> {
    s.providers
        .iter()
        .filter(|p| p.enabled && s.keys.usable(p.kind))
        .map(|p| p.kind)
        .collect()
}

impl QuickWindow {
    /// `--preview quick`: start with the text from the design.
    pub(super) fn preview(&self, text: &str) {
        let mut m = self.model.lock().unwrap();
        m.state.input = text.to_owned();
        // Preview windows stay open when the terminal keeps the focus.
        m.was_focused = false;
        m.opened = Instant::now() + Duration::from_secs(3600);
    }
}

impl App {
    /// A fresh Quick window state from the current settings.
    fn quick_model(&self, open: bool) -> QuickModel {
        let s = &self.settings;
        let keys = s.hotkeys.quick.map(|c| c.keys()).unwrap_or_default();
        QuickModel {
            open,
            shown: false,
            hwnd: None,
            state: QuickState::new(&s.source, &s.target),
            engine: self.engine.clone(),
            providers: usable_providers(s),
            keys,
            speak: s.speak_results,
            programmer: s.programmer_mode,
            animate: s.animations && s.hardware_acceleration && win::system_animations(),
            sent: None,
            last_input: String::new(),
            changed_at: Instant::now(),
            generation: 0,
            was_focused: false,
            opened: Instant::now(),
            size: [0.0, 0.0],
        }
    }

    /// Creates the Quick window hidden, so its first open is instant.
    pub(super) fn warm_quick(&mut self) {
        if self.quick.is_none() {
            let model = self.quick_model(false);
            self.quick = Some(QuickWindow { win: Floating::new(), model: Arc::new(Mutex::new(model)) });
        }
    }

    pub(super) fn open_quick(&mut self) {
        self.warm_quick();
        let fresh = self.quick_model(true);
        let Some(q) = &mut self.quick else { return };
        {
            let mut m = q.model.lock().unwrap();
            if m.open {
                drop(m);
                if let Some(h) = q.win.hwnd {
                    win::show_and_focus(h);
                }
                return;
            }
            // Keep the window's handle and measured size; everything else
            // starts over.
            let (size, hwnd) = (m.size, m.hwnd);
            *m = fresh;
            m.size = size;
            m.hwnd = hwnd;
        }
        q.win.shown = false;
        self.hide_popup();
        self.ctx.request_repaint_of(viewport_id());
    }

    /// Open, and since when (for Esc: the most recent window closes first).
    pub(super) fn quick_open_since(&self) -> Option<Instant> {
        let q = self.quick.as_ref()?;
        let m = q.model.lock().unwrap();
        m.open.then_some(m.opened)
    }

    /// Esc: close a menu or the Wikipedia card, else the window.
    pub(super) fn quick_escape(&mut self) {
        let Some(q) = &self.quick else { return };
        if q.model.lock().unwrap().state.escape() {
            self.ctx.request_repaint_of(viewport_id());
            return;
        }
        self.close_quick();
    }

    /// The Quick window is opening (for timing logs).
    pub(super) fn quick_opening(&self) -> bool {
        self.quick.as_ref().is_some_and(|q| {
            let m = q.model.lock().unwrap();
            m.open && !m.shown
        })
    }

    pub(super) fn close_quick(&mut self) {
        let Some(q) = &mut self.quick else { return };
        let mut m = q.model.lock().unwrap();
        if !m.open {
            return;
        }
        m.open = false;
        if let Some(Ok(t)) = &m.state.result {
            let from = t.src_lang.as_deref().unwrap_or(&m.state.from);
            history::add(&m.state.input, &t.text, from, &t.tgt_lang, t.provider, Mode::Quick);
        }
        let hwnd = m.hwnd;
        drop(m);
        if let Some(h) = hwnd {
            win::park(h);
        }
        // Back to the app the user was in.
        let last = win::Hwnd(crate::win::hook::LAST_APP.load(std::sync::atomic::Ordering::Relaxed));
        if !last.is_null() {
            win::focus(last);
        }
    }

    pub(super) fn quick_frame(&mut self, ctx: &egui::Context) {
        let Some(q) = &mut self.quick else { return };
        let initial = {
            let m = q.model.lock().unwrap();
            if m.size[0] > 0.0 { m.size } else { initial_size() }
        };
        let builder = ViewportBuilder::default()
            .with_title(QUICK_TITLE)
            .with_icon(window_icon())
            .with_inner_size(initial)
            .with_decorations(false)
            .with_transparent(true)
            .with_resizable(false)
            .with_always_on_top()
            .with_taskbar(false)
            .with_visible(false);
        let (model, tx) = (q.model.clone(), self.ui_tx.clone());
        let open = q.model.lock().unwrap().open;
        ctx.show_viewport_deferred(viewport_id(), builder, move |ui, _| {
            let ctx = ui.ctx().clone();
            let mut guard = model.lock().unwrap();
            let m = &mut *guard;
            // Hidden and waiting: draw nothing.
            if !m.open {
                return;
            }
            let view = QuickView { providers: &m.providers, keys: m.keys.clone(), animate: m.animate, shown: m.shown };
            crate::timing(&format!("quick: frame (shown {})", m.shown));
            let action = quick::show(ui, &mut m.state, &view);
            // First frame of this open: move the window into place now,
            // from this frame (no hop through the app's frame).
            if !m.shown {
                if m.hwnd.is_none() {
                    m.hwnd = win::find_window(QUICK_TITLE);
                }
                if let Some(h) = m.hwnd {
                    win::strip_chrome(h);
                    let (x, y) = place(m.size);
                    win::move_window(h, x, y);
                    win::show_and_focus(h);
                    m.shown = true;
                    m.opened = Instant::now();
                    crate::timing("quick: window shown");
                    ctx.request_repaint();
                }
            }
            match action {
                Some(QuickAction::Close) => send_ui(&ctx, &tx, UiEvent::CloseQuick),
                Some(QuickAction::CopyAndClose(text)) => {
                    std::thread::spawn(move || clipboard::set_text(&text, false));
                    send_ui(&ctx, &tx, UiEvent::CloseQuick);
                }
                Some(QuickAction::Copy(text)) => {
                    std::thread::spawn(move || clipboard::set_text(&text, false));
                }
                Some(QuickAction::Speak(text, lang)) => speech::speak(&text, &lang),
                Some(QuickAction::Retranslate) => m.sent = None,
                None => {}
            }
            drive(m, &model, &ctx);

            // Close when the user clicks away to another app.
            let focused = ui.input(|i| i.viewport().focused).unwrap_or(false);
            if focused {
                m.was_focused = true;
            } else if m.was_focused && m.opened.elapsed() > Duration::from_millis(300) {
                send_ui(&ctx, &tx, UiEvent::CloseQuick);
            }

            // Fit the window to the card (plus room for an open menu).
            let size = [
                quick::CARD_W + 2.0 * quick::MARGIN_X,
                m.state.card_h + quick::MARGIN_TOP + quick::MARGIN_BOTTOM + quick::MENU_ROOM,
            ];
            if m.state.card_h > 0.0 && (size[1] - m.size[1]).abs() > 0.5 {
                m.size = size;
                ctx.send_viewport_cmd(ViewportCommand::InnerSize(size.into()));
            }
        });
        if !open {
            // Warm and parked off-screen until it's needed.
            if q.win.hwnd.is_none() {
                q.win.hwnd = win::find_window(QUICK_TITLE);
                if let Some(h) = q.win.hwnd {
                    win::strip_chrome(h);
                    win::park(h);
                    q.model.lock().unwrap().hwnd = Some(h);
                }
            }
        }
    }
}

/// Where the Quick window opens: centred on the monitor under the cursor,
/// a fifth of the way down.
fn place(size: [f32; 2]) -> (i32, i32) {
    let size = if size[0] > 0.0 { size } else { initial_size() };
    let (x, y) = win::cursor_pos();
    let mon = win::monitor_at(x, y);
    let w = size[0] * mon.scale;
    let cx = (mon.work.left + mon.work.right) as f32 / 2.0;
    let top = mon.work.top as f32 + (mon.work.bottom - mon.work.top) as f32 * 0.22;
    ((cx - w / 2.0) as i32, top as i32)
}

fn initial_size() -> [f32; 2] {
    [
        quick::CARD_W + 2.0 * quick::MARGIN_X,
        220.0 + quick::MARGIN_TOP + quick::MARGIN_BOTTOM + quick::MENU_ROOM,
    ]
}

/// Debounce: translate once the text (or a language/provider) has been
/// unchanged for [`DEBOUNCE`]; stale answers are dropped by generation.
fn drive(m: &mut QuickModel, model: &Arc<Mutex<QuickModel>>, ctx: &egui::Context) {
    if m.state.input != m.last_input {
        m.last_input = m.state.input.clone();
        m.changed_at = Instant::now();
    }
    let text = crate::translate::normalize(&m.state.input);
    if text.is_empty() {
        m.state.result = None;
        m.state.loading = false;
        m.sent = None;
        return;
    }
    let want = (text.clone(), m.state.from.clone(), m.state.to.clone(), m.state.prefer);
    if m.sent.as_ref() == Some(&want) {
        return;
    }
    let wait = DEBOUNCE.saturating_sub(m.changed_at.elapsed());
    if !wait.is_zero() {
        ctx.request_repaint_after(wait);
        return;
    }
    m.sent = Some(want.clone());
    m.generation += 1;
    m.state.loading = true;
    let (generation, engine, model, ctx, speak, programmer) =
        (m.generation, m.engine.clone(), model.clone(), ctx.clone(), m.speak, m.programmer);
    std::thread::spawn(move || {
        let (text, from, to, prefer) = want;
        let code = programmer.then(|| crate::code::translate(&engine, &text, &from, &to, prefer)).flatten();
        let result = code.unwrap_or_else(|| match engine.translate_using(&text, &from, &to, prefer) {
            // A cold connection sometimes times out; try once more.
            Err(crate::translate::Error::Offline) => engine.translate_using(&text, &from, &to, prefer),
            r => r,
        });
        let mut m = model.lock().unwrap();
        if m.generation != generation {
            return;
        }
        if speak && let Ok(t) = &result {
            speech::speak(&t.text, &t.tgt_lang);
        }
        m.state.result = Some(result);
        m.state.loading = false;
        drop(m);
        ctx.request_repaint_of(viewport_id());
    });
}
