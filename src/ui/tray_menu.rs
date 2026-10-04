//! `Tray menu` — the custom-styled menu shown when the tray icon is clicked.

use egui::{
    Align, Area, CursorIcon, Id, Image, Layout, Margin, Order, Rect, ScrollArea, Sense, Ui,
    UiBuilder, pos2, vec2,
};

use super::widgets::*;
use crate::i18n::tr;
use crate::icons;
use crate::settings::search_languages;
use crate::theme::*;

pub const CARD_W: f32 = 300.0;
pub const CARD_H: f32 = 315.0;
/// Transparent margin around the card for its shadow.
pub const MARGIN_X: f32 = 20.0;
pub const MARGIN_TOP: f32 = 12.0;
pub const MARGIN_BOTTOM: f32 = 34.0;
pub const WINDOW: [f32; 2] = [CARD_W + 2.0 * MARGIN_X, CARD_H + MARGIN_TOP + MARGIN_BOTTOM];

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum Page {
    #[default]
    Main,
    Languages,
}

pub enum TrayAction {
    TranslateSelection,
    SetTarget(String),
    TogglePause,
    OpenTranslator,
    OpenHistory,
    OpenSettings,
    Quit,
}

pub struct TrayView<'a> {
    pub keys: Vec<String>,
    pub pair: String,
    pub target: &'a str,
    pub paused: bool,
}

/// A 34px menu row with a label on the left and `right` on the right.
fn item(ui: &mut Ui, label: &str, right: impl FnOnce(&mut Ui)) -> egui::Response {
    let rect = Rect::from_min_size(ui.cursor().min, vec2(ui.available_width(), 34.0));
    let resp = ui.allocate_rect(rect, Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(rect, 5, HOVER);
    }
    let mut child = ui.new_child(
        UiBuilder::new()
            .max_rect(rect.shrink2(vec2(10.0, 0.0)))
            .layout(Layout::left_to_right(Align::Center)),
    );
    text(&mut child, label, 13.0, Weight::Regular, TEXT);
    child.with_layout(Layout::right_to_left(Align::Center), right);
    resp.on_hover_cursor(CursorIcon::PointingHand)
}

/// `query` is the text in the language search box.
pub fn show(ui: &mut Ui, page: &mut Page, query: &mut String, v: &TrayView) -> Option<TrayAction> {
    let mut action = None;
    Area::new(Id::new("qt-tray-menu"))
        .order(Order::Foreground)
        .fixed_pos(pos2(MARGIN_X, MARGIN_TOP))
        .constrain(false)
        .show(ui.ctx(), |ui| {
            surface(ui, 8, Margin::same(4), |ui| {
                ui.set_width(CARD_W - 10.0);
                ui.set_height(CARD_H - 10.0);
                ui.spacing_mut().item_spacing = vec2(10.0, 1.0);
                match page {
                    Page::Main => main_page(ui, page, v, &mut action),
                    Page::Languages => languages_page(ui, page, query, v, &mut action),
                }
            });
        });
    action
}

fn main_page(ui: &mut Ui, page: &mut Page, v: &TrayView, action: &mut Option<TrayAction>) {
    // Header: app name and state.
    let rect = Rect::from_min_size(ui.cursor().min, vec2(ui.available_width(), 36.0));
    ui.allocate_rect(rect, Sense::hover());
    let mut header = ui.new_child(
        UiBuilder::new()
            .max_rect(rect.shrink2(vec2(10.0, 0.0)))
            .layout(Layout::left_to_right(Align::Center)),
    );
    header.spacing_mut().item_spacing.x = 8.0;
    icon(&mut header, icons::LANG, vec2(15.0, 15.0));
    text(&mut header, crate::APP_NAME, 13.0, Weight::SemiBold, TEXT);
    header.with_layout(Layout::right_to_left(Align::Center), |ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        text(ui, if v.paused { tr("Paused") } else { tr("On") }, 12.0, Weight::Regular, TEXT_3);
        if v.paused {
            let (r, _) = ui.allocate_exact_size(vec2(6.0, 6.0), Sense::hover());
            ui.painter().circle_filled(r.center(), 3.0, TEXT_3);
        } else {
            icon(ui, icons::DOT_ON, vec2(6.0, 6.0));
        }
    });

    menu_separator(ui);

    let keys = v.keys.clone();
    if item(ui, tr("Translate selection"), |ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        for k in keys.iter().rev() {
            key_cap(ui, k);
        }
    })
    .clicked()
    {
        *action = Some(TrayAction::TranslateSelection);
    }

    if item(ui, &v.pair, |ui| {
        icon(ui, icons::CHEVR, vec2(14.0, 14.0));
    })
    .clicked()
    {
        *page = Page::Languages;
    }

    if item(ui, tr("Open translator"), |_| {}).clicked() {
        *action = Some(TrayAction::OpenTranslator);
    }
    if item(ui, tr("History"), |ui| {
        icon(ui, icons::CLOCK, vec2(14.0, 14.0));
    })
    .clicked()
    {
        *action = Some(TrayAction::OpenHistory);
    }

    menu_separator(ui);

    let pause = if v.paused { tr("Resume") } else { tr("Pause for 1 hour") };
    if item(ui, pause, |_| {}).clicked() {
        *action = Some(TrayAction::TogglePause);
    }
    if item(ui, tr("Settings…"), |_| {}).clicked() {
        *action = Some(TrayAction::OpenSettings);
    }

    menu_separator(ui);

    if item(ui, tr("Quit"), |_| {}).clicked() {
        *action = Some(TrayAction::Quit);
    }
}

fn languages_page(
    ui: &mut Ui,
    page: &mut Page,
    query: &mut String,
    v: &TrayView,
    action: &mut Option<TrayAction>,
) {
    let rect = Rect::from_min_size(ui.cursor().min, vec2(ui.available_width(), 34.0));
    let back = ui.allocate_rect(rect, Sense::click());
    if back.hovered() {
        ui.painter().rect_filled(rect, 5, HOVER);
    }
    let chev = Rect::from_center_size(pos2(rect.left() + 17.0, rect.center().y), vec2(14.0, 14.0));
    Image::new(icons::CHEVR)
        .rotate(std::f32::consts::PI, vec2(0.5, 0.5))
        .paint_at(ui, chev);
    let galley = ui.painter().layout_no_wrap(
        tr("Translate to").to_owned(),
        font(13.0, Weight::SemiBold),
        TEXT,
    );
    ui.painter()
        .galley(pos2(rect.left() + 32.0, rect.center().y - galley.size().y / 2.0), galley, TEXT);
    if back.on_hover_cursor(CursorIcon::PointingHand).clicked() {
        *page = Page::Main;
        query.clear();
    }

    menu_separator(ui);

    let pick = |code: &str, page: &mut Page, query: &mut String, action: &mut Option<TrayAction>| {
        *action = Some(TrayAction::SetTarget(code.to_owned()));
        *page = Page::Main;
        query.clear();
    };

    ui.add_space(3.0);
    let (_, enter) = search_field(ui, query, tr("Search languages"), ui.available_width());
    ui.add_space(4.0);
    let matches = search_languages(query);
    if enter && let Some((code, _)) = matches.first() {
        pick(code, page, query, action);
        return;
    }

    ScrollArea::vertical()
        .max_height(ui.available_height())
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 1.0;
            if matches.is_empty() {
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.add_space(10.0);
                    text(ui, tr("No languages found"), 13.0, Weight::Regular, TEXT_3);
                });
            }
            for (code, name) in &matches {
                if menu_item(ui, name, *code == v.target, 34.0).clicked() {
                    pick(code, page, query, action);
                }
            }
        });
}
