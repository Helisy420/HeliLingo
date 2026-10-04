//! The Settings window on the app side: opening it, rendering it as a
//! deferred viewport, and running what it asks for (save + apply, autostart,
//! provider key checks, shortcut recording, minimize).

use std::time::Duration;
use std::sync::{Arc, Mutex};

use egui::{ViewportBuilder, ViewportCommand, ViewportId};

use super::{App, Floating, SETTINGS_TITLE, UiEvent, centered, send_ui, window_icon};
use crate::settings::{Combo, HotkeySlot, Settings, Shortcut};
use crate::i18n::trf;
use crate::translate::Error;
use crate::ui::settings_view::{self, CheckStatus, Page, SettingsAction, SettingsState};
use crate::win::{self, hook};

fn viewport_id() -> ViewportId {
    ViewportId::from_hash_of("qt-settings")
}

/// What the settings viewport renders: its own copy of the settings (the
/// source of truth while editing) and the view state.
pub(super) struct SettingsModel {
    pub settings: Settings,
    pub state: SettingsState,
}

pub(super) struct SettingsWindow {
    pub win: Floating,
    pub model: Arc<Mutex<SettingsModel>>,
}

impl App {
    pub(super) fn open_settings(&mut self) {
        self.open_settings_on(None);
    }

    /// Opens (or brings back) the settings window, optionally on `page`.
    pub(super) fn open_settings_on(&mut self, page: Option<Page>) {
        match &self.settings_window {
            Some(w) => {
                if let Some(p) = page {
                    w.model.lock().unwrap().state.page = p;
                }
                if let Some(h) = w.win.hwnd {
                    self.ctx.send_viewport_cmd_to(viewport_id(), ViewportCommand::Minimized(false));
                    win::show_and_focus(h);
                }
            }
            None => {
                let mut state = SettingsState::default();
                state.page = page.unwrap_or_default();
                self.settings_window = Some(SettingsWindow {
                    win: Floating::new(),
                    model: Arc::new(Mutex::new(SettingsModel { settings: self.settings.clone(), state })),
                });
            }
        }
    }

    /// `--preview settings[-page]`: opens on that page; the Providers
    /// preview expands DeepL like the Figma frame.
    pub(super) fn preview_settings(&mut self, what: &str) -> bool {
        // `settings-record`: Hotkeys page, already recording the Quick
        // window shortcut (for driving the recording path in tests).
        let record = what == "settings-record";
        let what = if record { "settings-hotkeys" } else { what };
        let Some(page) = Page::from_preview(what) else { return false };
        self.open_settings_on(Some(page));
        if let Some(w) = &self.settings_window {
            let mut m = w.model.lock().unwrap();
            match page {
                Page::Providers => m.state.expanded = Some(crate::settings::ProviderKind::DeepL),
                // Sample numbers in memory; stats.json is never touched.
                Page::Stats => m.state.stats.sample = true,
                _ => {}
            }
        }
        if record {
            self.on_settings_action(SettingsAction::StartCapture(HotkeySlot::Quick), self.settings.clone());
        }
        true
    }

    pub(super) fn settings_frame(&mut self, ctx: &egui::Context) {
        let Some(w) = &mut self.settings_window else { return };
        let builder = ViewportBuilder::default()
            .with_title(SETTINGS_TITLE)
            .with_icon(window_icon())
            .with_inner_size(settings_view::window_for(self.settings.settings_size.unwrap_or([settings_view::CARD_W, settings_view::CARD_H])))
            .with_min_inner_size(settings_view::WINDOW)
            .with_decorations(false)
            .with_transparent(true)
            .with_resizable(true)
            .with_taskbar(true)
            .with_visible(false);
        let (model, tx) = (w.model.clone(), self.ui_tx.clone());
        ctx.show_viewport_deferred(viewport_id(), builder, move |ui, _| {
            let mut guard = model.lock().unwrap();
            let m = &mut *guard;
            let mut action = settings_view::show(ui, &mut m.settings, &mut m.state);
            if ui.input(|i| i.viewport().close_requested()) {
                action = Some(SettingsAction::Close);
            }
            if let Some(a) = action {
                send_ui(ui.ctx(), &tx, UiEvent::Settings(a, Box::new(m.settings.clone())));
            }
        });
        let size = settings_view::window_for(self.settings.settings_size.unwrap_or([settings_view::CARD_W, settings_view::CARD_H]));
        w.win.reveal(SETTINGS_TITLE, || centered(size));
    }

    /// A [`SettingsAction`] from the window, with its settings at that moment.
    pub(super) fn on_settings_action(&mut self, action: SettingsAction, edited: Settings) {
        let Some(w) = &self.settings_window else { return };
        let model = w.model.clone();
        match action {
            SettingsAction::Close => {
                hook::stop_capture();
                self.settings_window = None;
            }
            SettingsAction::Minimize => {
                self.ctx.send_viewport_cmd_to(viewport_id(), ViewportCommand::Minimized(true));
            }
            // The window's copy is the source of truth while editing: don't
            // write back into it, or a keystroke typed meanwhile would be lost.
            SettingsAction::Changed => {
                self.settings = edited;
                self.apply_settings();
            }
            // The window only repaints on its own input, and the hook
            // swallows the keys it records: repaint it explicitly, or it
            // keeps showing the old caps / "Press keys…".
            SettingsAction::StartCapture(slot) => {
                let mut m = model.lock().unwrap();
                m.state.capturing = Some(slot);
                m.state.notice = None;
                hook::start_capture(Duration::from_millis(m.settings.double_tap_ms as u64));
                drop(m);
                self.ctx.request_repaint_of(viewport_id());
            }
            SettingsAction::CancelCapture => {
                model.lock().unwrap().state.capturing = None;
                hook::stop_capture();
                self.ctx.request_repaint_of(viewport_id());
            }
            // A key the window saw itself while recording (the hook normally
            // takes them first; this is the backup path).
            SettingsAction::CaptureKey(vk, mods) => hook::capture_key(vk, mods),
            SettingsAction::Autostart(on) => {
                if win::set_autostart(on) {
                    self.settings.start_with_windows = on;
                    self.settings.save();
                    model.lock().unwrap().settings.start_with_windows = on;
                }
            }
            SettingsAction::Check(kind) => {
                let (engine, keys, ctx) = (self.engine.clone(), edited.keys.clone(), self.ctx.clone());
                std::thread::spawn(move || {
                    let status = match engine.check(kind, &keys) {
                        Ok(()) => CheckStatus::Ok,
                        Err(e) => CheckStatus::Failed(check_error(e)),
                    };
                    model.lock().unwrap().state.checks.insert(kind, status);
                    ctx.request_repaint_of(viewport_id());
                });
            }
        }
    }

    /// A shortcut recording finished.
    pub(super) fn on_captured(&mut self, capture: hook::Capture) {
        hook::stop_capture();
        let Some(w) = &self.settings_window else { return };
        let model = w.model.clone();
        let changed = apply_capture(&mut self.settings, &mut model.lock().unwrap(), capture);
        if changed {
            self.apply_settings();
        }
        self.ctx.request_repaint_of(viewport_id());
    }
}

/// Every shortcut that is a chord-only slot (all but the main one).
const CHORD_SLOTS: [HotkeySlot; 6] = [
    HotkeySlot::Quick,
    HotkeySlot::Ultra,
    HotkeySlot::ScreenArea,
    HotkeySlot::WholeScreen,
    HotkeySlot::MainWindow,
    HotkeySlot::TranslatePaste,
];

/// Stores a recorded chord for the slot the window is recording: in the
/// app's settings and in the window's copy (which is the source of truth
/// while editing, so it must not keep the old chord and send it back with
/// the next edit). Ends the recording either way. Returns whether the
/// settings changed (then the caller saves and re-applies them).
fn apply_capture(app: &mut Settings, model: &mut SettingsModel, capture: hook::Capture) -> bool {
    let Some(slot) = model.state.capturing.take() else { return false };
    // Resolve clashes against the window's copy (it has every edit made
    // so far), then hand only the shortcuts to the app.
    let mut next = model.settings.clone();
    match capture {
        hook::Capture::Cancel => return false,
        // The main shortcut can't be off: Backspace resets it to Ctrl ×2.
        hook::Capture::Clear if slot == HotkeySlot::Translate => next.shortcut = Shortcut::DoubleCtrl,
        hook::Capture::Clear => next.hotkeys.set(slot, None),
        hook::Capture::Set(combo) => {
            if let Some(taken) = assign(&mut next, slot, combo) {
                model.state.notice = Some(trf(
                    "{keys} was used by “{other}” — that shortcut is now off",
                    &[("keys", combo.label().as_str()), ("other", settings_view::slot_title(taken))],
                ));
            }
        }
    }
    model.settings.shortcut = next.shortcut;
    model.settings.hotkeys = next.hotkeys;
    app.shortcut = next.shortcut;
    app.hotkeys = next.hotkeys;
    true
}

/// Gives `combo` to `slot`. Another shortcut that used the same binding is
/// turned off (the main one falls back to double-tap Ctrl); returns it so
/// the window can say so.
fn assign(s: &mut Settings, slot: HotkeySlot, combo: Combo) -> Option<HotkeySlot> {
    let mut taken = None;
    for other in CHORD_SLOTS {
        if other != slot && s.hotkeys.get(other) == Some(combo) {
            s.hotkeys.set(other, None);
            taken = Some(other);
        }
    }
    if slot != HotkeySlot::Translate && s.shortcut.as_combo() == combo {
        s.shortcut = Shortcut::DoubleCtrl;
        taken = Some(HotkeySlot::Translate);
    }
    match slot {
        HotkeySlot::Translate => s.shortcut = Shortcut::from_combo(combo),
        other => s.hotkeys.set(other, Some(combo)),
    }
    taken
}

fn check_error(e: Error) -> String {
    use crate::i18n::tr;
    match e {
        Error::Offline => tr("Can’t reach the translation service").to_owned(),
        Error::Busy => tr("The translation service is busy").to_owned(),
        Error::Other(m) => crate::i18n::tr_text(&m).to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model(capturing: Option<HotkeySlot>) -> SettingsModel {
        let mut state = SettingsState::default();
        state.capturing = capturing;
        SettingsModel { settings: Settings::default(), state }
    }

    #[test]
    fn captured_chord_reaches_both_copies() {
        let chord = Combo { ctrl: true, shift: true, vk: 0x4B, ..Default::default() }; // Ctrl+Shift+K
        for slot in [HotkeySlot::Translate].into_iter().chain(CHORD_SLOTS) {
            let mut app = Settings::default();
            let mut m = model(Some(slot));
            // An edit the window made before recording must survive.
            m.settings.double_tap_ms = 550;
            assert!(apply_capture(&mut app, &mut m, hook::Capture::Set(chord)));
            assert_eq!(m.state.capturing, None);
            let stored = |s: &Settings| match slot {
                HotkeySlot::Translate => s.shortcut == Shortcut::Combo(chord),
                other => s.hotkeys.get(other) == Some(chord),
            };
            assert!(stored(&app), "{slot:?} not stored in the app settings");
            assert!(stored(&m.settings), "{slot:?} not stored in the window copy");
            assert_eq!(m.settings.double_tap_ms, 550);
        }
    }

    #[test]
    fn whole_screen_chord_is_not_duplicated() {
        let mut app = Settings::default();
        let whole = app.hotkeys.whole_screen.unwrap();
        let mut m = model(Some(HotkeySlot::Quick));
        assert!(apply_capture(&mut app, &mut m, hook::Capture::Set(whole)));
        assert_eq!(app.hotkeys.quick, Some(whole));
        assert_eq!(app.hotkeys.whole_screen, None);
        assert_eq!(m.settings.hotkeys.whole_screen, None);
    }

    #[test]
    fn cancel_or_no_recording_changes_nothing() {
        let mut app = Settings::default();
        let mut m = model(Some(HotkeySlot::Ultra));
        assert!(!apply_capture(&mut app, &mut m, hook::Capture::Cancel));
        assert_eq!(m.state.capturing, None);
        assert_eq!(app.hotkeys, Settings::default().hotkeys);
        let mut idle = model(None);
        assert!(!apply_capture(&mut app, &mut idle, hook::Capture::Set(Combo::ctrl_alt(0x4B))));
    }

    #[test]
    fn recorded_chord_moves_between_slots() {
        let mut s = Settings::default();
        let quick = s.hotkeys.quick.unwrap();
        assign(&mut s, HotkeySlot::Ultra, quick);
        assert_eq!(s.hotkeys.ultra, Some(quick));
        assert_eq!(s.hotkeys.quick, None);
        assign(&mut s, HotkeySlot::Translate, quick);
        assert_eq!(s.shortcut, Shortcut::Combo(quick));
        assert_eq!(s.hotkeys.ultra, None);
    }
}
