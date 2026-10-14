//! The app's windows.
//!
//! The ones a shortcut opens — Quick, the popup and the tray menu — are
//! created hidden at start, so a shortcut never waits for a webview to load.
//! The bigger ones (translator, settings, history, Ultra, welcome) are
//! created on first use and only hidden afterwards: each webview costs a
//! renderer process, and most sessions never open them.

use std::sync::Arc;

use tauri::{AppHandle, Emitter, Manager, Result, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

use crate::state::AppState;

pub const QUICK: &str = "quick";
pub const POPUP: &str = "popup";
pub const TRAY: &str = "tray";
pub const TRANSLATOR: &str = "translator";
pub const SETTINGS: &str = "settings";
pub const HISTORY: &str = "history";
pub const ULTRA: &str = "ultra";
pub const WELCOME: &str = "welcome";

/// Floating windows: transparent, so the page draws its own rounded card
/// and shadow; sized by the page to its content.
fn floating(app: &AppHandle, label: &str, w: f64, h: f64) -> Result<WebviewWindow> {
    WebviewWindowBuilder::new(app, label, WebviewUrl::App(format!("{label}.html").into()))
        .title("HeliLingo")
        .inner_size(w, h)
        .resizable(false)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .visible(false)
        .build()
}

/// Regular windows: no system title bar (the page draws the Figma one);
/// Windows 11 rounds the corners and draws the shadow.
fn regular(app: &AppHandle, label: &str) -> Result<WebviewWindow> {
    let en = app.state::<Arc<AppState>>().settings.read().unwrap().ui_language == crate::settings::UiLanguage::En;
    let (title, w, h, resizable) = match label {
        TRANSLATOR => ("HeliLingo", 540.0, 500.0, true),
        SETTINGS => (if en { "Settings — HeliLingo" } else { "Настройки — HeliLingo" }, 800.0, 660.0, false),
        HISTORY => (if en { "History — HeliLingo" } else { "История — HeliLingo" }, 540.0, 600.0, true),
        ULTRA => (if en { "Ultra — HeliLingo" } else { "«Ультра» — HeliLingo" }, 640.0, 520.0, true),
        _ => ("HeliLingo", 560.0, 510.0, false),
    };
    WebviewWindowBuilder::new(app, label, WebviewUrl::App(format!("{label}.html").into()))
        .title(title)
        .inner_size(w, h)
        .min_inner_size(w.min(480.0), h.min(400.0))
        .resizable(resizable)
        .decorations(false)
        .shadow(true)
        .center()
        .visible(false)
        .build()
}

/// Creates the windows shortcuts need instantly.
pub fn create_startup(app: &AppHandle) -> Result<()> {
    floating(app, QUICK, 728.0, 200.0)?;
    floating(app, POPUP, 480.0, 200.0)?;
    floating(app, TRAY, 372.0, 420.0)?;
    Ok(())
}

/// Shows a regular window with focus, creating it the first time. A new
/// window shows itself once its page has rendered (the `ready` command), so
/// it never flashes empty.
pub fn show(app: &AppHandle, label: &str) {
    show_with(app, label, None);
}

/// [`show`] with a payload for the page's `window:shown` handler (text for
/// the translator, a page name for settings). A window created now gets
/// it from `pending` once its page is ready.
pub fn show_with(app: &AppHandle, label: &str, payload: Option<String>) {
    remember_target(app);
    let Some(w) = app.get_webview_window(label) else {
        PENDING.lock().unwrap().insert(label.to_owned(), payload);
        if let Err(e) = regular(app, label) {
            tracing::error!("can't open {label}: {e}");
        }
        return;
    };
    let _ = w.unminimize();
    let _ = w.show();
    focus(&w);
    let _ = w.emit_to(label, "window:shown", payload);
}

/// Payloads for windows that were still loading.
static PENDING: std::sync::Mutex<std::collections::BTreeMap<String, Option<String>>> =
    std::sync::Mutex::new(std::collections::BTreeMap::new());

/// The payload a just-created window should start with.
pub fn take_pending(label: &str) -> Option<String> {
    PENDING.lock().unwrap().remove(label).flatten()
}

/// Brings a window to the front with keyboard focus (see `force_foreground`).
/// `set_focus` then hands the focus on to the webview inside, or the page
/// never gets the keys.
pub fn focus(w: &WebviewWindow) {
    if let Ok(h) = w.hwnd() {
        hl_win::window::force_foreground(hl_win::Hwnd(h.0 as isize));
    }
    let _ = w.set_focus();
}

/// Shows the Quick window on the monitor under the cursor, a quarter of
/// the way down, focused, with its text selected — or replaced by `text`.
///
/// Always on the main thread: shown from another thread (a shortcut runs
/// its action off the main thread), the transparent webview only repainted
/// the caret and the text around it. The window already exists, so nothing
/// here can hit the window-creation deadlock.
pub fn show_quick(app: &AppHandle, text: Option<String>) {
    let handle = app.clone();
    if let Err(e) = app.run_on_main_thread(move || show_quick_now(&handle, text)) {
        tracing::error!("can't show Quick: {e}");
    }
}

fn show_quick_now(app: &AppHandle, text: Option<String>) {
    let Some(w) = app.get_webview_window(QUICK) else { return };
    remember_target(app);
    if let Some(monitor) = cursor_monitor(app) {
        let area = monitor.work_area();
        let size = w.outer_size().unwrap_or_default();
        let x = area.position.x + (area.size.width as i32 - size.width as i32) / 2;
        let y = area.position.y + area.size.height as i32 / 4;
        let _ = w.set_position(tauri::PhysicalPosition::new(x, y));
    }
    let _ = w.show();
    focus(&w);
    let _ = w.emit_to(QUICK, "quick:shown", text);
}

pub fn cursor_monitor(app: &AppHandle) -> Option<tauri::Monitor> {
    let p = app.cursor_position().ok()?;
    app.monitor_from_point(p.x, p.y).ok().flatten().or_else(|| app.primary_monitor().ok().flatten())
}

/// Notes the foreground window, unless it is one of ours: Replace pastes there.
pub fn remember_target(app: &AppHandle) {
    let fg = hl_win::window::foreground();
    if fg.is_null() || hl_win::window::process_id(fg) == std::process::id() {
        return;
    }
    *app.state::<Arc<AppState>>().target.lock().unwrap() = fg;
}
