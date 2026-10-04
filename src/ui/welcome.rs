//! `How it works` — three-step intro shown on first launch.

use egui::{
    Align, Align2, Area, Frame, Id, Layout, Margin, Order, Rect, Sense, Stroke, Ui, UiBuilder,
    pos2, vec2,
};

use super::widgets::*;
use crate::i18n::{tr, trf};
use crate::icons;
use crate::settings::Shortcut;
use crate::theme::*;

pub const MAX_CARD_W: f32 = 1320.0;
pub const CARD_H: f32 = 250.0;
pub const MARGIN_X: f32 = 24.0;
pub const MARGIN_TOP: f32 = 16.0;
pub const MARGIN_BOTTOM: f32 = 40.0;

pub fn window_size(card_w: f32) -> [f32; 2] {
    [card_w + 2.0 * MARGIN_X, CARD_H + MARGIN_TOP + MARGIN_BOTTOM]
}

/// Returns true when the user dismissed the window.
pub fn show(ui: &mut Ui, card_w: f32, shortcut: &Shortcut) -> bool {
    let mut close = false;
    let ctx = ui.ctx().clone();
    Area::new(Id::new("qt-welcome"))
        .order(Order::Middle)
        .fixed_pos(pos2(MARGIN_X, MARGIN_TOP))
        .constrain(false)
        .show(&ctx, |ui| {
            let r = Frame::new()
                .fill(CANVAS)
                .stroke(BORDER_STROKE)
                .corner_radius(12)
                .inner_margin(Margin::symmetric(28, 26))
                .shadow(SHADOW_LG)
                .show(ui, |ui| {
                    let inner_w = card_w - 58.0;
                    ui.set_width(inner_w);
                    ui.set_height(CARD_H - 54.0);
                    let col_w = (inner_w - 2.0 * 16.0 - 4.0 * 20.0) / 3.0;
                    ui.horizontal_top(|ui| {
                        ui.spacing_mut().item_spacing.x = 20.0;
                        let (title2, hint2) = if shortcut.is_double_tap() {
                            (shortcut.label(), tr("Two quick presses, under half a second"))
                        } else {
                            (trf("Press {s}", &[("s", &shortcut.label())]), tr("Works in any app"))
                        };
                        step(ui, col_w, 1, tr("Select a word"), tr("Or a sentence — anywhere, in any app"), 20.0, demo_selection);
                        chevron(ui);
                        step(ui, col_w, 2, &title2, hint2, 34.0, |ui| demo_keys(ui, shortcut));
                        chevron(ui);
                        step(ui, col_w, 3, tr("Read it, carry on"), tr("Appears next to the word. Esc or click away closes it"), 34.0, demo_popup);
                    });
                });

            // Grab the card anywhere to move the window.
            if ui.interact(r.response.rect, Id::new("qt-welcome-drag"), Sense::drag()).drag_started() {
                crate::win::begin_window_drag();
            }

            // Close button in the corner.
            let x = Rect::from_min_size(r.response.rect.right_top() + vec2(-28.0, 12.0), vec2(16.0, 16.0));
            let mut corner = ui.new_child(UiBuilder::new().max_rect(x).layout(Layout::left_to_right(Align::Min)));
            if icon_button(&mut corner, icons::X, 16.0, tr("Got it")).clicked() {
                close = true;
            }
        });
    if ui.input(|i| i.key_pressed(egui::Key::Escape) || i.key_pressed(egui::Key::Enter)) {
        close = true;
    }
    close
}

fn step(
    ui: &mut Ui,
    w: f32,
    n: u32,
    title: &str,
    hint: &str,
    demo_height: f32,
    demo: impl FnOnce(&mut Ui),
) {
    ui.vertical(|ui| {
        ui.set_width(w);
        ui.spacing_mut().item_spacing.y = 6.0;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 10.0;
            let (r, _) = ui.allocate_exact_size(vec2(20.0, 20.0), Sense::hover());
            ui.painter().circle_stroke(r.center(), 9.5, Stroke::new(1.0, BORDER));
            ui.painter().text(
                r.center(),
                Align2::CENTER_CENTER,
                n.to_string(),
                font(11.0, Weight::SemiBold),
                TEXT_2,
            );
            text(ui, title, 14.0, Weight::SemiBold, TEXT);
        });
        text(ui, hint, 12.0, Weight::Regular, TEXT_3);
        let (well, _) = ui.allocate_exact_size(vec2(w, 110.0), Sense::hover());
        ui.painter().rect_filled(well, 10, WELL);
        // Centre the demo: vertically by its height, horizontally via `centered_row`.
        let mut inner = ui.new_child(
            UiBuilder::new()
                .max_rect(well)
                .layout(Layout::top_down(Align::Min)),
        );
        inner.add_space(((110.0 - demo_height) / 2.0).max(0.0));
        inner.horizontal(demo);
    });
}

fn chevron(ui: &mut Ui) {
    ui.vertical(|ui| {
        ui.set_width(16.0);
        ui.add_space(90.0);
        icon(ui, icons::CHEVR_STEPS, vec2(16.0, 16.0));
    });
}

fn centered_row(ui: &mut Ui, content_w: f32, add: impl FnOnce(&mut Ui)) {
    let avail = ui.available_width();
    ui.add_space(((avail - content_w) / 2.0).max(0.0));
    add(ui);
}

fn demo_selection(ui: &mut Ui) {
    let words = ["far more", "effectively", "last"];
    let widths: f32 = words
        .iter()
        .map(|w| ui.painter().layout_no_wrap((*w).to_owned(), font(16.0, Weight::Regular), TEXT).size().x)
        .sum::<f32>()
        + 2.0 * 5.0
        + 4.0;
    centered_row(ui, widths, |ui| {
        ui.spacing_mut().item_spacing.x = 5.0;
        text(ui, words[0], 16.0, Weight::Regular, TEXT);
        Frame::new()
            .fill(HIGHLIGHT)
            .corner_radius(3)
            .inner_margin(Margin::symmetric(2, 0))
            .show(ui, |ui| text(ui, words[1], 16.0, Weight::Regular, TEXT));
        text(ui, words[2], 16.0, Weight::Regular, TEXT);
    });
}

fn demo_keys(ui: &mut Ui, shortcut: &Shortcut) {
    let keys = shortcut.keys();
    let cap_w = |ui: &Ui, k: &str| {
        ui.painter().layout_no_wrap(k.to_owned(), font(13.0, Weight::Medium), TEXT_2).size().x + 34.0
    };
    let total: f32 = keys.iter().map(|k| cap_w(ui, k)).sum::<f32>() + 8.0 * (keys.len() as f32 - 1.0);
    centered_row(ui, total, |ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        for k in &keys {
            // Raised key: solid 2px "shadow" in the border colour.
            let idx = ui.painter().add(egui::Shape::Noop);
            let r = Frame::new()
                .fill(CARD)
                .stroke(BORDER_STROKE)
                .corner_radius(7)
                .inner_margin(Margin::symmetric(16, 7))
                .show(ui, |ui| text(ui, k, 13.0, Weight::Medium, TEXT_2));
            ui.painter().set(
                idx,
                egui::epaint::RectShape::filled(r.response.rect.translate(vec2(0.0, 2.0)), 7, BORDER),
            );
        }
    });
}

fn demo_popup(ui: &mut Ui) {
    let w = ui.painter().layout_no_wrap("эффективно".to_owned(), font(15.0, Weight::Medium), TEXT).size().x
        + 24.0
        + 10.0
        + 14.0
        + 2.0;
    centered_row(ui, w, |ui| {
        surface(ui, 8, Margin::symmetric(12, 7), |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 10.0;
                text(ui, "эффективно", 15.0, Weight::Medium, TEXT);
                icon(ui, icons::VOL_COMPACT, vec2(14.0, 14.0));
            });
        });
    });
}
