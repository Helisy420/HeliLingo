//! Settings → General (Figma 329:2): interface (language, theme,
//! autostart, animations, acceleration), translation, popup, history and
//! the apps where shortcuts are ignored.

use std::time::{Duration, Instant};

use egui::Ui;

use super::*;
use crate::settings::PopupStyle;

/// How long the armed "Click again to clear" stays armed.
const CLEAR_CONFIRM: Duration = Duration::from_secs(4);

pub(super) fn show(ui: &mut Ui, s: &mut Settings, state: &mut SettingsState) -> Option<SettingsAction> {
    let mut action = None;
    page_title(ui, tr("General"), None);

    section(ui, tr("Interface"), |ui| {
        let lang = crate::i18n::UI_LANGUAGES
            .iter()
            .find(|(c, _)| *c == s.ui_lang)
            .map_or("Русский", |(_, n)| *n);
        row(ui, tr("Interface language"), None, 140.0, false, |ui| {
            select(ui, state, Menu::UiLang, lang, 140.0);
        });
        // Only the dark theme exists so far.
        row(ui, tr("Theme"), None, 250.0, false, |ui| {
            segmented_sm(
                ui,
                &[tr("Dark"), tr("Light"), tr("As in Windows")],
                0,
                &[true, false, false],
                tr("Coming later"),
            );
        });
        row(ui, tr("Start with Windows"), None, 32.0, false, |ui| {
            let mut on = s.start_with_windows;
            if switch(ui, &mut on).clicked() {
                action = Some(SettingsAction::Autostart(on));
            }
        });
        let sub = tr("Smooth fades and slides for popups, windows and pages");
        row(ui, tr("Animations"), Some(sub), 32.0, false, |ui| {
            changed(switch(ui, &mut s.animations), &mut action);
        });
        let sub = tr("Off: lighter rendering for weak or remote PCs (no animations). Applies after restart.");
        row(ui, tr("Hardware acceleration"), Some(sub), 32.0, true, |ui| {
            changed(switch(ui, &mut s.hardware_acceleration), &mut action);
        });
    });

    section(ui, tr("Translation"), |ui| {
        row(ui, tr("Translate to"), None, 150.0, false, |ui| {
            select(ui, state, Menu::Target, &lang_name(&s.target), 150.0);
        });
        row(ui, tr("Translate from"), None, 210.0, false, |ui| {
            select(ui, state, Menu::Source, &lang_name(&s.source), 210.0);
        });
        let title = trf("If the text is already in {lang}", &[("lang", &lang_in(&s.target))]);
        row(ui, &title, None, 210.0, true, |ui| {
            select(ui, state, Menu::AlreadyIn, already_label(s.already_in_target, &s.target), 210.0);
        });
    });

    section(ui, tr("Popup"), |ui| {
        row(ui, tr("Style"), None, 160.0, false, |ui| {
            let active = if s.popup_style == PopupStyle::Compact { 0 } else { 1 };
            if let Some(i) = segmented_sm(ui, &[tr("Compact"), tr("Full")], active, &[], "") {
                s.popup_style = if i == 0 { PopupStyle::Compact } else { PopupStyle::Full };
                action = Some(SettingsAction::Changed);
            }
        });
        {
            use crate::settings::CompactVariants as V;
            const MODES: [V; 3] = [V::Off, V::List, V::Menu];
            let sub = tr("In the compact popup: up to 3 translations from your other services, pick one");
            row(ui, tr("Other services"), Some(sub), 200.0, false, |ui| {
                let active = MODES.iter().position(|m| *m == s.compact_variants).unwrap_or(1);
                if let Some(i) = segmented_sm(ui, &[tr("Off"), tr("List"), tr("Menu")], active, &[], "") {
                    s.compact_variants = MODES[i];
                    action = Some(SettingsAction::Changed);
                }
            });
        }
        row(ui, tr("Hide after"), None, 90.0, false, |ui| {
            select(ui, state, Menu::HideAfter, &hide_after_label(s.hide_after_secs), 90.0);
        });
        row(ui, tr("Read translation aloud"), None, 32.0, false, |ui| {
            changed(switch(ui, &mut s.speak_results), &mut action);
        });
        row(ui, tr("Ctrl+C in the popup"), None, 180.0, false, |ui| {
            select(ui, state, Menu::CtrlC, ctrl_c_label(s.popup_ctrl_c), 180.0);
        });
        row(ui, tr("Show Wiki button"), Some(tr("Wikipedia article for a word, in popups and windows")), 32.0, true, |ui| {
            changed(switch(ui, &mut s.show_wiki), &mut action);
        });
    });

    section(ui, tr("History"), |ui| {
        row(ui, tr("Save translation history"), None, 32.0, false, |ui| {
            changed(switch(ui, &mut s.save_history), &mut action);
        });
        row(ui, tr("Saved translations"), None, 200.0, true, |ui| {
            let armed = state.clear_armed.is_some_and(|t| t.elapsed() < CLEAR_CONFIRM);
            let label = if armed {
                tr("Click again to clear").to_owned()
            } else {
                trf("Clear history ({n})", &[("n", &crate::history::len().to_string())])
            };
            if outline_button(ui, &label, 24.0).clicked() {
                if armed {
                    crate::history::clear();
                    state.clear_armed = None;
                } else {
                    state.clear_armed = Some(Instant::now());
                }
            }
            if armed {
                ui.ctx().request_repaint_after(Duration::from_millis(250));
            }
        });
    });

    section(ui, tr("Shortcuts are ignored in"), |ui| {
        row(ui, tr("Fullscreen apps"), Some(tr("Games and videos in full screen")), 32.0, false, |ui| {
            changed(switch(ui, &mut s.ignore_fullscreen), &mut action);
        });
        let empty = s.ignored_apps.is_empty();
        row(ui, tr("These apps"), Some(tr("While one of them is in front, the shortcuts do nothing")), 110.0, empty, |ui| {
            let open = state.open.is_some_and(|(m, _)| m == Menu::AddApp);
            let r = outline_button(ui, tr("Add app…"), 24.0);
            if r.clicked() && !open {
                state.apps = crate::win::apps::running_apps();
            }
            toggle_menu(state, Menu::AddApp, &r, open);
        });
        let mut remove = None;
        let n = s.ignored_apps.len();
        for (i, app) in s.ignored_apps.iter().enumerate() {
            row(ui, app, None, 22.0, i + 1 == n, |ui| {
                if square_button(ui, icons::X15_MUTED, 22.0, 13.0, tr("Remove")).clicked() {
                    remove = Some(i);
                }
            });
        }
        if let Some(i) = remove {
            s.ignored_apps.remove(i);
            action = Some(SettingsAction::Changed);
        }
    });
    action
}
