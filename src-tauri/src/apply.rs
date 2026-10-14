//! Changing settings: validate, save, apply side effects (shortcuts,
//! autostart, engines), and tell every window.

use std::sync::Arc;

use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_autostart::ManagerExt;

use crate::settings::Settings;
use crate::state::AppState;

pub fn update(app: &AppHandle, patch: serde_json::Value) -> Result<Settings, String> {
    let state = app.state::<Arc<AppState>>();
    let (old, new) = {
        let mut s = state.settings.write().unwrap();
        let new = s.merged(patch)?;
        let old = std::mem::replace(&mut *s, new.clone());
        (old, new)
    };
    new.save().map_err(|e| format!("can't save settings: {e}"))?;
    side_effects(app, Some(&old), &new);
    let _ = app.emit("settings", &new);
    Ok(new)
}

/// What changing from `old` to `new` means outside the settings file.
/// `old = None`: app start.
pub fn side_effects(app: &AppHandle, old: Option<&Settings>, new: &Settings) {
    let state = app.state::<Arc<AppState>>();
    if old.is_none_or(|o| o.hotkeys != new.hotkeys) {
        let taken = crate::hotkeys::apply(app, &new.hotkeys);
        if !taken.is_empty() {
            let _ = app.emit("hotkeys:taken", taken);
        }
    }
    if old.is_none_or(|o| {
        o.hotkeys != new.hotkeys
            || o.double_tap_ms != new.double_tap_ms
            || o.popup_ctrl_c != new.popup_ctrl_c
            || o.ignored_apps != new.ignored_apps
            || o.skip_fullscreen != new.skip_fullscreen
            || o.bubble.close_on_outside != new.bubble.close_on_outside
    }) {
        crate::selection::configure_hook(app, new);
    }
    if old.is_none_or(|o| {
        o.engines != new.engines
            || o.google_mode != new.google_mode
            || o.yandex_folder != new.yandex_folder
            || o.bing_region != new.bing_region
            || o.openrouter_model != new.openrouter_model
            || o.offline != new.offline
            // OpenRouter's token counter knows whether to count.
            || o.keep_stats != new.keep_stats
    }) {
        state.rebuild_engines();
        let _ = app.emit("engines", ());
    }
    // Offline switched off: end every model process. Device, threads,
    // precision or folder changed: they restart with the new setup on use.
    if let Some(o) = old {
        let (a, b) = (&o.offline, &new.offline);
        if a.enabled && !b.enabled
            || a.device != b.device
            || a.threads != b.threads
            || a.precision != b.precision
            || a.model_dir != b.model_dir
        {
            hl_offline::shutdown();
        }
        for id in a.tiers.iter().filter(|t| !b.tiers.contains(t)) {
            if let Some(m) = hl_offline::Model::from_id(id) {
                hl_offline::unload(m);
            }
        }
    }
    if old.is_none_or(|o| o.editor_api != new.editor_api) {
        crate::api::apply(app, new.editor_api);
    }
    if old.is_none_or(|o| o.autostart != new.autostart) {
        set_autostart(app, new.autostart);
    }
}

/// Start with Windows. Only release builds touch the Run key: a debug exe
/// in target\debug must never end up starting with Windows.
fn set_autostart(app: &AppHandle, on: bool) {
    if cfg!(debug_assertions) {
        return;
    }
    let al = app.autolaunch();
    let now = al.is_enabled().unwrap_or(false);
    let result = match (on, now) {
        (true, false) => al.enable(),
        (false, true) => al.disable(),
        _ => Ok(()),
    };
    if let Err(e) = result {
        tracing::warn!("autostart: {e}");
    }
}
