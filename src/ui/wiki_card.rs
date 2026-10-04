//! The Wikipedia card behind the "W" buttons: a compact surface (card
//! fill, border, radius 12) with the article title, a 2–4 line extract and
//! "Open in Wikipedia"; "Nothing found" with a search link otherwise.
//! Shared by the popups, the Quick window and the main window.

use egui::text::{LayoutJob, TextWrapping};
use egui::{Align2, Area, Id, Image, Margin, Order, Pos2, Rect, Sense, TextFormat, Ui, vec2};

use super::widgets::*;
use crate::i18n::tr;
use crate::icons;
use crate::theme::*;
use crate::wiki::{self, Outcome, Query};

/// Rough card height, for deciding where a popup fits.
pub const ESTIMATED_H: f32 = 150.0;
/// Gap between the card and what it belongs to.
pub const GAP: f32 = 6.0;

/// The card at `pos` (its `pivot` corner) in a foreground area. Returns the
/// card's rect and whether its × was clicked. `notify` wakes the window
/// when the article arrives.
pub fn area(
    ctx: &egui::Context,
    id: &str,
    pos: Pos2,
    pivot: Align2,
    q: &Query,
    width: f32,
    notify: impl FnOnce() + Send + 'static,
) -> (Rect, bool) {
    let mut close = false;
    let resp = Area::new(Id::new(id))
        .order(Order::Foreground)
        .fixed_pos(pos)
        .pivot(pivot)
        .constrain(false)
        .show(ctx, |ui| close = show(ui, q, width, notify));
    (resp.response.rect, close)
}

/// The card itself; returns true when it should close.
pub fn show(ui: &mut Ui, q: &Query, width: f32, notify: impl FnOnce() + Send + 'static) -> bool {
    let outcome = wiki::lookup(q, notify);
    let mut close = false;
    surface(ui, 12, Margin { left: 14, right: 12, top: 12, bottom: 12 }, |ui| {
        ui.set_width(width - 26.0);
        ui.spacing_mut().item_spacing.y = 6.0;
        let title = match outcome.as_deref() {
            Some(Outcome::Found(a)) => a.title.clone(),
            _ => tr("Wikipedia article").to_owned(),
        };
        // Header: W, title, close.
        let w = ui.available_width();
        ui.allocate_ui_with_layout(vec2(w, 20.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
            ui.set_min_size(vec2(w, 20.0));
            ui.spacing_mut().item_spacing.x = 8.0;
            let (r, _) = ui.allocate_exact_size(vec2(15.0, 15.0), Sense::hover());
            Image::new(icons::WIKI15_ON).paint_at(ui, r);
            let galley = truncated(ui, &title, font(14.0, Weight::SemiBold), TEXT, w - 15.0 - 8.0 - 8.0 - 16.0);
            let (r, _) = ui.allocate_exact_size(galley.size(), Sense::hover());
            ui.painter().galley(r.min, galley, TEXT);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if icon_button(ui, icons::X15_MUTED, 14.0, tr("Close")).clicked() {
                    close = true;
                }
            });
        });
        match outcome.as_deref() {
            None => {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    ui.add(egui::Spinner::new().size(13.0).color(TEXT_3));
                    text(ui, tr("Searching Wikipedia…"), 12.0, Weight::Regular, TEXT_3);
                });
            }
            Some(Outcome::Found(a)) => {
                if !a.extract.is_empty() {
                    let mut job = LayoutJob::single_section(
                        a.extract.clone(),
                        TextFormat {
                            font_id: font(13.0, Weight::Regular),
                            color: TEXT_2,
                            line_height: Some(18.0),
                            ..Default::default()
                        },
                    );
                    job.wrap = TextWrapping { max_rows: 4, ..TextWrapping::truncate_at_width(ui.available_width()) };
                    job.wrap.break_anywhere = false;
                    ui.add(egui::Label::new(job).selectable(false));
                }
                ui.add_space(2.0);
                if link_button(ui, Some(icons::EXTERNAL12), tr("Open in Wikipedia")).clicked() {
                    crate::win::open_url(&a.url);
                }
            }
            Some(Outcome::NotFound { search_url }) => {
                text(ui, tr("Nothing found"), 13.0, Weight::Regular, TEXT_2);
                if link_button(ui, Some(icons::EXTERNAL12), tr("Search Wikipedia")).clicked() {
                    crate::win::open_url(search_url);
                }
            }
            Some(Outcome::Failed { search_url }) => {
                text(ui, tr("Wikipedia is unavailable"), 13.0, Weight::Regular, WARN);
                if link_button(ui, Some(icons::EXTERNAL12), tr("Search Wikipedia")).clicked() {
                    crate::win::open_url(search_url);
                }
            }
        }
    });
    if close {
        wiki::forget_failure(q);
    }
    close
}
