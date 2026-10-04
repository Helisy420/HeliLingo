//! Settings → Hotkeys (Figma 329:683): the shortcuts as key caps, Ctrl+C
//! twice, and the double-tap interval. Clicking a row's caps starts recording
//! ([`SettingsAction::StartCapture`]); the app stores what the input hook
//! records (see `app::settings_win`).

use egui::{Shape, Ui};

use super::*;
use crate::settings::{Combo, DOUBLE_TAP_RANGE};

/// What a shortcut row shows in its caps.
enum Caps {
    DoubleTap(&'static str),
    Chord(Combo),
    Off,
    Recording,
}

/// The chord shortcuts below the main one, with their titles and notes.
fn chord_slots() -> [(HotkeySlot, &'static str, Option<&'static str>); 6] {
    [
        (HotkeySlot::Quick, tr("Quick window"), Some(tr("Type text — the translation appears at once"))),
        (HotkeySlot::Ultra, tr("Ultra mode"), Some(tr("A selection in several languages"))),
        (HotkeySlot::ScreenArea, tr("Screen area"), Some(tr("Translate text in a picture"))),
        (HotkeySlot::WholeScreen, tr("Whole screen"), Some(tr("Translate everything on the monitor"))),
        (HotkeySlot::MainWindow, tr("Main window"), None),
        (HotkeySlot::TranslatePaste, tr("Translate and paste"), Some(tr("Replaces the selection with its translation, no window"))),
    ]
}

pub(super) fn show(ui: &mut Ui, s: &mut Settings, state: &mut SettingsState) -> Option<SettingsAction> {
    let mut action = None;
    page_title(ui, tr("Hotkeys"), Some(tr("Click a shortcut to change it.")));

    // Recording: the input hook normally takes the keys before the window
    // sees them. Any key that does reach the window is passed on as a
    // backup (physical keys, so the keyboard layout doesn't matter).
    if state.capturing.is_some() {
        let keys: Vec<(u32, Combo)> = ui.input(|i| {
            i.events
                .iter()
                .filter_map(|e| match e {
                    egui::Event::Key { key, physical_key, pressed: true, repeat: false, modifiers } => {
                        let vk = crate::win::input::egui_key_vk(physical_key.unwrap_or(*key))?;
                        let held = Combo {
                            ctrl: modifiers.ctrl,
                            shift: modifiers.shift,
                            alt: modifiers.alt,
                            win: false,
                            vk,
                            double: false,
                        };
                        Some((vk, held))
                    }
                    _ => None,
                })
                .collect()
        });
        if let Some((vk, held)) = keys.into_iter().next() {
            action = Some(SettingsAction::CaptureKey(vk, held));
        }
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
    }

    section(ui, tr("Shortcuts"), |ui| {
        // The main shortcut: a menu (double taps or record a chord).
        let caps = if state.capturing == Some(HotkeySlot::Translate) {
            Caps::Recording
        } else {
            match s.shortcut {
                Shortcut::DoubleCtrl => Caps::DoubleTap("Ctrl"),
                Shortcut::DoubleAlt => Caps::DoubleTap("Alt"),
                Shortcut::DoubleShift => Caps::DoubleTap("Shift"),
                Shortcut::Combo(c) => Caps::Chord(c),
            }
        };
        row(ui, tr("Translate selection"), Some(tr("In the popup next to the word")), 160.0, false, |ui| {
            let open = state.open.is_some_and(|(m, _)| m == Menu::Shortcut);
            let r = caps_button(ui, &caps, open || state.capturing == Some(HotkeySlot::Translate));
            if r.clicked() && state.capturing == Some(HotkeySlot::Translate) {
                action = Some(SettingsAction::CancelCapture);
            } else {
                toggle_menu(state, Menu::Shortcut, &r, open);
            }
        });

        let slots = chord_slots();
        let n = slots.len();
        for (i, (slot, title, sub)) in slots.into_iter().enumerate() {
            let recording = state.capturing == Some(slot);
            let combo = s.hotkeys.get(slot);
            let caps = match combo {
                _ if recording => Caps::Recording,
                Some(c) => Caps::Chord(c),
                None => Caps::Off,
            };
            row(ui, title, sub, 190.0, i + 1 == n, |ui| {
                let r = caps_button(ui, &caps, recording);
                if combo.is_some() && !recording && off_button(ui).clicked() {
                    s.hotkeys.set(slot, None);
                    action = Some(SettingsAction::Changed);
                }
                if r.clicked() {
                    state.open = None;
                    action = Some(if recording {
                        SettingsAction::CancelCapture
                    } else {
                        SettingsAction::StartCapture(slot)
                    });
                }
            });
        }
    });

    group(ui, |ui| {
        let sub = tr("Press Ctrl + C twice to translate what you copied, like in DeepL");
        row(ui, tr("Ctrl + C twice"), Some(sub), 32.0, true, |ui| {
            changed(switch(ui, &mut s.copy_twice), &mut action);
        });
    });

    // How to record, or what the last recording changed.
    let note = if state.capturing.is_some() {
        Some(tr("Press a chord (Ctrl + Alt + A) or a mouse side button, press it twice for ×2, or tap Ctrl, Alt or Shift twice. Esc cancels, Backspace turns the shortcut off."))
    } else {
        state.notice.as_deref()
    };
    if let Some(note) = note {
        ui.add_space(6.0);
        ui.add(egui::Label::new(rt(note, 11.0, Weight::Regular, if state.capturing.is_some() { ACCENT } else { WARN })).wrap());
    }

    section(ui, tr("Double press"), |ui| {
        let sub = tr("Lower — fewer accidental triggers");
        row(ui, tr("Interval between presses"), Some(sub), 184.0, true, |ui| {
            ui.spacing_mut().item_spacing.x = 10.0;
            let value = trf("{n} ms", &[("n", &s.double_tap_ms.to_string())]);
            let (r, _) = ui.allocate_exact_size(egui::vec2(44.0, 14.0), Sense::hover());
            let g = ui.painter().layout_no_wrap(value, font(11.5, Weight::SemiBold), TEXT);
            ui.painter().galley(pos2(r.right() - g.size().x, r.center().y - g.size().y / 2.0), g, TEXT);
            let resp = slider(ui, &mut s.double_tap_ms, DOUBLE_TAP_RANGE, 10, 130.0);
            if resp.drag_stopped() || resp.clicked() {
                action = Some(SettingsAction::Changed);
            }
        });
    });
    action
}

/// Small × left of the caps that turns a shortcut off. Shown while the
/// row is hovered; its space is always kept so the caps never move.
fn off_button(ui: &mut Ui) -> egui::Response {
    let row_hovered = ui.rect_contains_pointer(ui.max_rect());
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(20.0, 20.0), Sense::click());
    if !row_hovered {
        return resp;
    }
    if resp.hovered() {
        ui.painter().rect_filled(rect, 5, HOVER);
    }
    Image::new(icons::X15_MUTED).paint_at(ui, Rect::from_center_size(rect.center(), egui::vec2(13.0, 13.0)));
    resp.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(tr("Turn off"))
}

/// The key caps of a shortcut row, clickable as one button (hover plate
/// behind them). `active` = recording or its menu is open. Rows lay their
/// controls out right to left, so the caps are added last to first.
fn caps_button(ui: &mut Ui, caps: &Caps, active: bool) -> egui::Response {
    enum Part {
        Key(String, Color32),
        Text(String),
    }
    let parts = match caps {
        Caps::DoubleTap(key) => vec![Part::Key((*key).to_owned(), TEXT_2), Part::Text("× 2".into())],
        Caps::Chord(c) => {
            let mut parts: Vec<Part> = c.keys().into_iter().map(|k| Part::Key(k, TEXT_2)).collect();
            if c.double {
                parts.push(Part::Text("× 2".into()));
            }
            parts
        }
        Caps::Off => vec![Part::Text(tr("Off").into())],
        Caps::Recording => vec![Part::Key(tr("Press keys…").into(), ACCENT)],
    };
    let plate = ui.painter().add(Shape::Noop);
    let inner = ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        for part in parts.iter().rev() {
            match part {
                Part::Key(k, c) => {
                    kbd(ui, k, *c);
                }
                Part::Text(t) => {
                    // Same height as a key cap, so the text centres with them.
                    let g = ui.painter().layout_no_wrap(t.clone(), font(11.0, Weight::Medium), TEXT_3);
                    let (r, _) = ui.allocate_exact_size(egui::vec2(g.size().x, 18.0), Sense::hover());
                    ui.painter().galley(pos2(r.left(), r.center().y - g.size().y / 2.0), g, TEXT_3);
                }
            }
        }
    });
    let rect = inner.response.rect;
    let resp = ui.interact(rect.expand2(egui::vec2(4.0, 3.0)), inner.response.id.with("caps"), Sense::click());
    let fill = if active {
        Some(SELECTED)
    } else if resp.hovered() {
        Some(HOVER)
    } else {
        None
    };
    if let Some(c) = fill {
        ui.painter().set(plate, Shape::rect_filled(rect.expand2(egui::vec2(4.0, 3.0)), 6, c));
    }
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}
