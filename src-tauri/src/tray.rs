//! Tray icon. Either click opens the tray menu (Figma «Трей-меню»), which
//! is its own window placed next to the icon.

use tauri::tray::{MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, Result};

use crate::windows::{self, TRAY};

pub fn create(app: &AppHandle) -> Result<()> {
    TrayIconBuilder::with_id("main")
        .icon(app.default_window_icon().cloned().expect("bundle icon"))
        .tooltip("HeliLingo")
        .show_menu_on_left_click(false)
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button_state: MouseButtonState::Up, position, .. } = event {
                show_menu(tray.app_handle(), position);
            }
        })
        .build(app)?;
    Ok(())
}

/// Opens the tray menu window with a corner at the click, kept inside the
/// work area (the taskbar can be on any edge).
fn show_menu(app: &AppHandle, at: PhysicalPosition<f64>) {
    let Some(w) = app.get_webview_window(TRAY) else { return };
    windows::remember_target(app);
    let size = w.outer_size().unwrap_or_default();
    let (mut x, mut y) = (at.x as i32 - size.width as i32 + 24, at.y as i32 - size.height as i32 + 24);
    if let Some(m) = app.monitor_from_point(at.x, at.y).ok().flatten() {
        let a = m.work_area();
        let (left, top) = (a.position.x, a.position.y);
        let (right, bottom) = (left + a.size.width as i32, top + a.size.height as i32);
        x = x.clamp(left, (right - size.width as i32).max(left));
        y = y.clamp(top, (bottom - size.height as i32).max(top));
    }
    let _ = w.set_position(PhysicalPosition::new(x, y));
    let _ = w.show();
    windows::focus(&w);
    let _ = w.emit_to(TRAY, "window:shown", ());
}
