//! Translating the selection in another app.
//!
//! The hook (`hl_win::hook`) reports double taps, mouse-button shortcuts,
//! double Ctrl+C and the popup's keys; plain chords come from the
//! global-shortcut plugin. Either way an action ends up here: the selection
//! is copied through the clipboard (and the clipboard put back), then shown
//! in the popup next to it, copied, pasted over, or sent to Ultra.
//!
//! The popup never takes focus, so the app keeps its selection and caret.
//! Its page reports its size (`popup_layout`) and Rust places it under the
//! selection — or above it when there's no room below.

use std::sync::mpsc::channel;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use hl_core::RequestId;
use hl_pipeline::Request;
use hl_win::hook::{self, Combo, HookEvent};
use hl_win::window::{self, Anchor};
use hl_win::{Hwnd, clipboard};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

use crate::actions::Action;
use crate::settings::{Placement, PopupCtrlC, Settings};
use crate::state::AppState;
use crate::windows::{self, POPUP};

/// Where the popup goes (set when it's shown, used on every re-layout).
static ANCHOR: Mutex<Option<Anchor>> = Mutex::new(None);
/// The bubble is pinned: clicks elsewhere don't close it, and it stays
/// where the user put it.
pub static PINNED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// What the popup page gets.
#[derive(Clone, Serialize)]
#[serde(untagged)]
pub enum PopupShow {
    Text { text: String },
    Error { error: &'static str },
}

/// Hook binding ids are action indexes.
const HOOK_ACTIONS: [Action; 8] = [
    Action::TranslateSelection,
    Action::Quick,
    Action::Translator,
    Action::Ultra,
    Action::CopyTranslation,
    Action::Screenshot,
    Action::PasteTranslation,
    Action::WindowCapture,
];

/// Starts the hook (once) and its event loop.
pub fn start(app: &AppHandle) {
    let (tx, rx) = channel::<HookEvent>();
    hook::start(tx);
    let app = app.clone();
    std::thread::Builder::new()
        .name("hook-events".into())
        .spawn(move || {
            while let Ok(e) = rx.recv() {
                handle(&app, e);
            }
        })
        .expect("spawn hook event thread");
}

fn handle(app: &AppHandle, e: HookEvent) {
    let state = app.state::<Arc<AppState>>();
    match e {
        HookEvent::Trigger(id) => {
            if state.active()
                && let Some(a) = HOOK_ACTIONS.get(id as usize)
            {
                crate::actions::run(app, *a);
            }
        }
        HookEvent::DoubleCopy => {
            if !state.active() {
                return;
            }
            // The app copies first; give it a moment to finish.
            std::thread::sleep(Duration::from_millis(60));
            let text = clipboard::get_text().filter(|t| !t.trim().is_empty());
            let anchor = window::selection_anchor();
            show_popup(app, anchor, text);
        }
        HookEvent::PopupEscape => hide_popup(app),
        // A pinned bubble stays; so does one the user keeps open on purpose.
        HookEvent::ClickAway => {
            let keep = PINNED.load(std::sync::atomic::Ordering::Relaxed)
                || !app.state::<Arc<AppState>>().settings.read().unwrap().bubble.close_on_outside;
            if !keep {
                hide_popup(app);
            }
        }
        HookEvent::PopupCopy => {
            let _ = app.emit_to(POPUP, "popup:key", "copy");
        }
    }
}

/// Points the hook at the current shortcut settings.
pub fn configure_hook(_app: &AppHandle, s: &Settings) {
    let h = &s.hotkeys;
    let strings = [
        &h.translate_selection,
        &h.quick,
        &h.translator,
        &h.ultra,
        &h.copy_translation,
        &h.screenshot,
        &h.paste_translation,
        &h.window_capture,
    ];
    let bindings = strings.iter().enumerate().filter_map(|(i, s)| Combo::parse(s).map(|c| (i as u32, c))).collect();
    hook::configure(hook::Config {
        bindings,
        double_tap: Duration::from_millis(s.double_tap_ms.into()),
        double_copy: h.double_copy,
        popup_ctrl_c: s.popup_ctrl_c != PopupCtrlC::Ignore,
        popup_esc: s.bubble.close_on_outside,
        paused: false,
        ignored_apps: s.ignored_apps.clone(),
        skip_fullscreen: s.skip_fullscreen,
    });
}

/// Captures the selection and runs `action` on it (off the caller's thread:
/// copying waits for the app to answer Ctrl+C).
pub fn run(app: &AppHandle, action: Action) {
    let app = app.clone();
    std::thread::spawn(move || {
        let fg = window::foreground();
        let ours = window::process_id(fg) == std::process::id();
        let state = app.state::<Arc<AppState>>();
        let target = if ours { *state.target.lock().unwrap() } else { fg };
        if !ours {
            *state.target.lock().unwrap() = fg;
        }
        // From our own window (tray menu): give focus back to the app first.
        if ours && !target.is_null() {
            window::force_foreground(target);
            std::thread::sleep(Duration::from_millis(80));
        }
        let anchor = window::selection_anchor();
        // UI Automation first (the clipboard stays untouched), then Ctrl+C.
        let text = hl_win::uia::selected_text(Duration::from_millis(250))
            .or_else(|| clipboard::copy_selection(window::is_terminal(target), Duration::from_millis(450)));
        match action {
            Action::TranslateSelection => show_popup(&app, anchor, text),
            Action::Ultra => match text {
                Some(t) if state.settings.read().unwrap().ultra_window => {
                    windows::show_with(&app, windows::ULTRA, Some(t));
                }
                Some(t) => {
                    let app2 = app.clone();
                    tauri::async_runtime::spawn(async move { translate_then(&app2, t, target, true).await });
                }
                None => show_popup(&app, anchor, None),
            },
            Action::CopyTranslation | Action::PasteTranslation => match text {
                Some(t) => {
                    let paste = action == Action::PasteTranslation;
                    let app2 = app.clone();
                    tauri::async_runtime::spawn(async move { translate_then(&app2, t, target, paste).await });
                }
                None => show_popup(&app, anchor, None),
            },
            _ => {}
        }
    });
}

/// Translates `text` without a window, then pastes it over the selection
/// in `target` or copies it.
async fn translate_then(app: &AppHandle, text: String, target: Hwnd, paste: bool) {
    let state = app.state::<Arc<AppState>>().inner().clone();
    let req = {
        let s = state.settings.read().unwrap();
        Request {
            text,
            source: s.source_lang.clone(),
            target: s.target_lang.clone(),
            second_lang: s.second_lang.clone(),
            engine: String::new(),
            programmer_mode: s.programmer_mode,
            context: Vec::new(),
        }
    };
    let router = state.router(None);
    match state.pipeline.run(RequestId(0), req, router, state.online(), |_| {}).await {
        Ok(done) => {
            let text = done.text;
            tauri::async_runtime::spawn_blocking(move || {
                if paste {
                    hl_win::paste_into(target, &text);
                } else {
                    clipboard::set_text(&text, false);
                }
            });
        }
        Err(e) => tracing::info!("translate-and-{}: {e}", if paste { "paste" } else { "copy" }),
    }
}

/// Shows the popup for `text` (or "nothing selected") near `anchor`. The
/// page lays itself out and calls `popup_layout`, which places and shows
/// the window.
pub fn show_popup(app: &AppHandle, anchor: Anchor, text: Option<String>) {
    *ANCHOR.lock().unwrap() = Some(anchor);
    let msg = match text {
        Some(text) => PopupShow::Text { text },
        None => PopupShow::Error { error: "nothingSelected" },
    };
    let _ = app.emit_to(POPUP, "popup:show", msg);
}

pub fn hide_popup(app: &AppHandle) {
    let state = app.state::<Arc<AppState>>();
    state.requests.cancel(hl_core::Surface::Popup);
    if let Some(w) = app.get_webview_window(POPUP)
        && let Ok(h) = w.hwnd()
    {
        let h = Hwnd(h.0 as isize);
        hook::set_popup(h, None);
        window::hide(h);
    }
    *ANCHOR.lock().unwrap() = None;
}

/// The popup page's size and its card inside it (CSS pixels).
#[derive(Deserialize)]
pub struct Layout {
    pub width: f64,
    pub height: f64,
    pub card_x: f64,
    pub card_y: f64,
    pub card_w: f64,
    pub card_h: f64,
}

/// Where the pinned bubble's card is (screen pixels, top-left).
static PIN_SPOT: Mutex<Option<(i32, i32)>> = Mutex::new(None);
/// The card's offset inside the window at the last layout (pixels).
static CARD_OFFSET: Mutex<(i32, i32)> = Mutex::new((0, 0));
/// The card's size at the last layout (pixels).
static CARD_SIZE: Mutex<(i32, i32)> = Mutex::new((0, 0));

/// Pins or unpins the bubble at its current place.
pub fn pin(app: &AppHandle, on: bool) {
    PINNED.store(on, std::sync::atomic::Ordering::Relaxed);
    let spot = if on {
        let pos = app.get_webview_window(POPUP).and_then(|w| w.outer_position().ok());
        let (cx, cy) = *CARD_OFFSET.lock().unwrap();
        pos.map(|p| (p.x + cx, p.y + cy))
    } else {
        None
    };
    *PIN_SPOT.lock().unwrap() = spot;
    save_pin(app, spot);
}

/// The bubble moved (placed by us, or dragged): the hook's idea of where
/// the card is follows, and a pinned bubble remembers its new place.
pub fn moved(app: &AppHandle, x: i32, y: i32) {
    let (cx, cy) = *CARD_OFFSET.lock().unwrap();
    let (cw, ch) = *CARD_SIZE.lock().unwrap();
    if hook::POPUP_VISIBLE.load(std::sync::atomic::Ordering::Relaxed)
        && let Some(w) = app.get_webview_window(POPUP)
        && let Ok(raw) = w.hwnd()
    {
        let (l, t) = (x + cx, y + cy);
        hook::set_popup(Hwnd(raw.0 as isize), Some((l, t, l + cw, t + ch)));
    }
    if !PINNED.load(std::sync::atomic::Ordering::Relaxed) {
        return;
    }
    let spot = Some((x + cx, y + cy));
    // Our own placing moves the window too: save only real changes.
    if std::mem::replace(&mut *PIN_SPOT.lock().unwrap(), spot) != spot {
        save_pin(app, spot);
    }
}

/// «Запоминать место закреплённого бабла»: the place survives a restart.
fn save_pin(app: &AppHandle, spot: Option<(i32, i32)>) {
    let remember = app.state::<Arc<AppState>>().settings.read().unwrap().bubble.remember_pinned;
    if !remember && spot.is_some() {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let patch = serde_json::json!({ "bubble": { "pinnedAt": spot } });
        if let Err(e) = crate::apply::update(&app, patch) {
            tracing::warn!("pinned place: {e}");
        }
    });
}

/// At start: a remembered pinned place pins the bubble again.
pub fn restore_pin(s: &Settings) {
    if s.bubble.remember_pinned
        && let Some(spot) = s.bubble.pinned_at
    {
        PINNED.store(true, std::sync::atomic::Ordering::Relaxed);
        *PIN_SPOT.lock().unwrap() = Some(spot);
    }
}

/// Places the popup and shows it without taking focus: at the pinned spot,
/// or by the «Где показывать» setting — under the selection (above it if
/// there's no room), at the pointer, or in the middle of the screen.
pub fn layout(app: &AppHandle, l: Layout) {
    let Some(anchor) = *ANCHOR.lock().unwrap() else { return };
    let Some(w) = app.get_webview_window(POPUP) else { return };
    let Ok(raw) = w.hwnd() else { return };
    let h = Hwnd(raw.0 as isize);
    let placement = app.state::<Arc<AppState>>().settings.read().unwrap().bubble.placement;
    let pinned = PINNED.load(std::sync::atomic::Ordering::Relaxed).then(|| *PIN_SPOT.lock().unwrap()).flatten();
    let (ax, ay) = pinned.unwrap_or((anchor.x, anchor.bottom));
    let (left, top, right, bottom, scale) = window::work_area_at(ax, ay);
    let s = |v: f64| (v * scale as f64).round() as i32;
    let (ww, wh) = (s(l.width), s(l.height));
    let _ = w.set_size(tauri::PhysicalSize::new(ww as u32, wh as u32));
    let (cx, cy, cw, ch) = (s(l.card_x), s(l.card_y), s(l.card_w), s(l.card_h));
    *CARD_OFFSET.lock().unwrap() = (cx, cy);
    *CARD_SIZE.lock().unwrap() = (cw, ch);
    let gap = s(6.0);
    let (mut card_left, mut card_top) = match (pinned, placement) {
        (Some(spot), _) => spot,
        (None, Placement::Center) => (left + (right - left - cw) / 2, top + (bottom - top) / 3 - ch / 2),
        (None, Placement::Cursor) => {
            let (px, py) = window::cursor_pos();
            let below = py + s(18.0);
            (px - s(16.0), if below + ch <= bottom { below } else { py - s(8.0) - ch })
        }
        // The card's top-left goes 6 px below the line, its left edge at
        // the selection's x.
        (None, Placement::Selection) => {
            let below = anchor.bottom + gap;
            (anchor.x - s(16.0), if below + ch <= bottom { below } else { anchor.top - gap - ch })
        }
    };
    card_left = card_left.clamp(left + s(4.0), (right - cw - s(4.0)).max(left));
    card_top = card_top.clamp(top + s(4.0), (bottom - ch).max(top));
    // The window is offset by the card's position in it.
    let (x, y) = (card_left - cx, card_top - cy);
    window::make_no_activate(h);
    window::show_no_activate(h, x, y);
    hook::set_popup(h, Some((card_left, card_top, card_left + cw, card_top + ch)));
}
