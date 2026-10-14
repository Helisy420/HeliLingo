//! Global shortcuts from settings. Plain chords ("Ctrl+Shift+L") go through
//! the global-shortcut plugin; double taps ("Ctrl×2") and mouse buttons
//! need the low-level hooks in `hl_win` (see `hook`).

use std::str::FromStr;
use std::sync::Mutex;

use tauri::{AppHandle, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutEvent, ShortcutState};

use crate::actions::{self, Action};
use crate::settings::Hotkeys;

/// Registered chords and what they do.
static BOUND: Mutex<Vec<(Shortcut, Action)>> = Mutex::new(Vec::new());

/// The actions of a hotkeys block, with their shortcut strings.
pub fn bindings(h: &Hotkeys) -> [(Action, &str); 8] {
    [
        (Action::TranslateSelection, h.translate_selection.as_str()),
        (Action::Quick, h.quick.as_str()),
        (Action::Translator, h.translator.as_str()),
        (Action::Ultra, h.ultra.as_str()),
        (Action::CopyTranslation, h.copy_translation.as_str()),
        (Action::Screenshot, h.screenshot.as_str()),
        (Action::WindowCapture, h.window_capture.as_str()),
        (Action::PasteTranslation, h.paste_translation.as_str()),
    ]
}

/// A chord the plugin can register: modifiers + key, no double tap, no mouse.
pub fn as_chord(s: &str) -> Option<Shortcut> {
    let s = s.trim();
    if s.is_empty() || s.contains('×') || s.to_ascii_lowercase().contains("mouse") {
        return None;
    }
    Shortcut::from_str(s).ok()
}

/// (Re-)registers every chord from `hotkeys`. Returns the ones another app
/// already owns.
pub fn apply(app: &AppHandle, hotkeys: &Hotkeys) -> Vec<String> {
    let gs = app.global_shortcut();
    let _ = gs.unregister_all();
    let mut bound = BOUND.lock().unwrap();
    bound.clear();
    let mut taken = Vec::new();
    for (action, text) in bindings(hotkeys) {
        let Some(sc) = as_chord(text) else { continue };
        match gs.register(sc) {
            Ok(()) => bound.push((sc, action)),
            Err(e) => {
                tracing::warn!("shortcut {text} is taken: {e}");
                taken.push(text.to_owned());
            }
        }
    }
    taken
}

/// The plugin's handler: runs the action bound to `shortcut`.
pub fn handle(app: &AppHandle, shortcut: &Shortcut, event: ShortcutEvent) {
    if event.state() != ShortcutState::Pressed {
        return;
    }
    let state = app.state::<std::sync::Arc<crate::state::AppState>>();
    if !state.active() || blocked_here(&state.settings.read().unwrap()) {
        return;
    }
    let action = BOUND.lock().unwrap().iter().find(|(s, _)| s == shortcut).map(|(_, a)| *a);
    if let Some(a) = action {
        // The plugin calls this on the main thread, and creating a webview
        // there (the translator, Ultra, the capture overlay on first use)
        // deadlocks on Windows (wry#583): run the action off it.
        let app = app.app_handle().clone();
        std::thread::spawn(move || actions::run(&app, a));
    }
}

/// The foreground app is on the ignore list, or fullscreen (when that
/// setting is on): shortcuts stay quiet, like the hook's.
fn blocked_here(s: &crate::settings::Settings) -> bool {
    let fg = hl_win::window::foreground();
    if hl_win::window::process_id(fg) == std::process::id() {
        return false;
    }
    if s.skip_fullscreen && hl_win::window::is_fullscreen(fg) {
        return true;
    }
    let exe = hl_win::window::exe_name(fg);
    !exe.is_empty() && s.ignored_apps.iter().any(|a| a.eq_ignore_ascii_case(&exe))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_chords_only() {
        assert!(as_chord("Ctrl+Shift+L").is_some());
        assert!(as_chord("Ctrl+Alt+T").is_some());
        assert!(as_chord("Ctrl×2").is_none());
        assert!(as_chord("Mouse4").is_none());
        assert!(as_chord("").is_none());
    }
}
