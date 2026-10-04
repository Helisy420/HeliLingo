//! History view inside the main window (no Figma frame; built from the
//! window's own pieces): search, All / Starred, the list of translations
//! with star / copy / delete, and "Clear history".

use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use egui::{Align, CursorIcon, Label, Layout, Rect, ScrollArea, Sense, Ui, pos2, vec2};

use super::{child, hline};
use crate::history::{self, Entry, Mode};
use crate::i18n::tr;
use crate::icons;
use crate::settings::lang_badge;
use crate::theme::*;
use crate::ui::widgets::*;

#[derive(Default)]
pub struct HistoryState {
    pub query: String,
    pub starred_only: bool,
    /// "Clear history" was clicked once; a second click within 4 s clears.
    confirm_clear: Option<Instant>,
    copied: Option<(u64, Instant)>,
}

pub enum HistoryAction {
    Back,
    Open(Entry),
    Copy(String),
}

const ROW_H: f32 = 72.0;

pub fn show(ui: &mut Ui, content: Rect, hs: &mut HistoryState) -> Option<HistoryAction> {
    let mut action = None;

    // Header: back pill, title, All / Starred.
    let head = Rect::from_min_size(content.min, vec2(content.width(), 32.0));
    let mut row = child(ui, head, Layout::left_to_right(Align::Center));
    row.spacing_mut().item_spacing.x = 14.0;
    if back_pill(&mut row).clicked() {
        action = Some(HistoryAction::Back);
    }
    text(&mut row, tr("History"), 16.0, Weight::SemiBold, TEXT);
    row.with_layout(Layout::right_to_left(Align::Center), |ui| {
        let active = usize::from(hs.starred_only);
        if let Some(i) = segmented(ui, &[tr("All"), tr("Starred")], active) {
            hs.starred_only = i == 1;
        }
    });

    // Search.
    let search = Rect::from_min_size(pos2(content.left(), head.bottom() + 12.0), vec2(content.width(), 32.0));
    let mut s = child(ui, search, Layout::left_to_right(Align::Center));
    search_field(&mut s, &mut hs.query, tr("Search history"), content.width());

    // List.
    let footer_h = 30.0;
    let list = Rect::from_min_max(pos2(content.left(), search.bottom() + 12.0), pos2(content.right(), content.bottom() - footer_h - 12.0));
    ui.painter().rect(list, 16, PANEL, BORDER_STROKE, egui::StrokeKind::Inside);
    let entries = history::list(&hs.query, hs.starred_only);
    let inner = list.shrink2(vec2(1.0, 6.0));
    let mut lui = child(ui, inner, Layout::top_down(Align::Min));
    if entries.is_empty() {
        let (title, hint) = if !hs.query.trim().is_empty() {
            (tr("Nothing found"), tr("Try another word"))
        } else if hs.starred_only {
            (tr("No starred translations"), tr("Star a translation to keep it here"))
        } else {
            (tr("History is empty"), tr("Your translations will appear here"))
        };
        let mut col = child(&mut lui, inner, Layout::top_down(Align::Center));
        col.add_space(inner.height() / 2.0 - 22.0);
        text(&mut col, title, 15.0, Weight::Medium, TEXT_3);
        col.add_space(6.0);
        text(&mut col, hint, 13.0, Weight::Regular, TEXT_3);
    } else {
        ScrollArea::vertical()
            .id_salt("qt-history-list")
            .auto_shrink([false, false])
            .show_rows(&mut lui, ROW_H, entries.len(), |ui, range| {
                ui.spacing_mut().item_spacing.y = 0.0;
                for i in range {
                    if let Some(a) = entry_row(ui, &entries[i], i + 1 < entries.len(), hs) {
                        action = Some(a);
                    }
                }
            });
    }

    // Footer: Clear history (confirm), count.
    let foot = Rect::from_min_max(pos2(content.left(), content.bottom() - footer_h), content.max);
    let mut f = child(ui, foot, Layout::left_to_right(Align::Center));
    let confirming = hs.confirm_clear.is_some_and(|t| t.elapsed() < Duration::from_secs(4));
    if history::len() > 0 {
        let (label, color) = if confirming {
            (tr("Click again to clear the history"), WARN)
        } else {
            (tr("Clear history"), TEXT_2)
        };
        if text_button(&mut f, label, 13.0, Weight::Medium, color, WARN).clicked() {
            if confirming {
                history::clear();
                hs.confirm_clear = None;
            } else {
                hs.confirm_clear = Some(Instant::now());
            }
        }
        if confirming {
            ui.ctx().request_repaint_after(Duration::from_millis(500));
        }
    }
    f.with_layout(Layout::right_to_left(Align::Center), |ui| {
        let n = history::len();
        let s = crate::i18n::trf("{n} entries", &[("n", &n.to_string())]);
        text(ui, s, 12.0, Weight::Regular, TEXT_3);
    });
    action
}

/// Back to the translator: a 32px pill like the mode pills.
fn back_pill(ui: &mut Ui) -> egui::Response {
    let label = tr("Translator");
    let galley = ui.painter().layout_no_wrap(label.to_owned(), font(14.0, Weight::Medium), TEXT_2);
    let size = vec2(12.0 + 16.0 + 8.0 + galley.size().x + 16.0, 32.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let p = ui.painter();
    if resp.hovered() {
        p.rect_filled(rect, 16, HOVER);
    }
    p.rect_stroke(rect, 16, BORDER_STROKE, egui::StrokeKind::Inside);
    // "‹" chevron drawn as two strokes.
    let c = pos2(rect.left() + 20.0, rect.center().y);
    let stroke = egui::Stroke::new(1.7, TEXT_2);
    p.line_segment([c + vec2(2.5, -5.0), c + vec2(-2.5, 0.0)], stroke);
    p.line_segment([c + vec2(-2.5, 0.0), c + vec2(2.5, 5.0)], stroke);
    p.galley(pos2(rect.left() + 36.0, rect.center().y - galley.size().y / 2.0), galley, TEXT_2);
    resp.on_hover_cursor(CursorIcon::PointingHand)
}

fn mode_label(m: Mode) -> &'static str {
    match m {
        Mode::Popup => tr("Popup"),
        Mode::Quick => tr("Quick window"),
        Mode::Main => tr("Text"),
        Mode::Ultra => tr("Ultra"),
        Mode::Image => tr("Image"),
    }
}

/// "just now", "5 min ago", "3 h ago", "yesterday", "4 d ago", or a date.
pub fn relative_time(then: u64, now: u64) -> String {
    let secs = now.saturating_sub(then);
    let n = |v: u64| v.to_string();
    match secs {
        0..60 => tr("just now").to_owned(),
        60..3600 => crate::i18n::trf("{n} min ago", &[("n", &n(secs / 60))]),
        3600..86400 => crate::i18n::trf("{n} h ago", &[("n", &n(secs / 3600))]),
        86400..172800 => tr("yesterday").to_owned(),
        172800..604800 => crate::i18n::trf("{n} d ago", &[("n", &n(secs / 86400))]),
        _ => {
            let (y, m, d) = civil_date(then);
            format!("{d:02}.{m:02}.{y}")
        }
    }
}

/// UTC calendar date of a Unix time (days-from-civil, inverted).
fn civil_date(t: u64) -> (i64, u32, u32) {
    let z = (t / 86400) as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d)
}

fn entry_row(ui: &mut Ui, e: &Entry, divider: bool, hs: &mut HistoryState) -> Option<HistoryAction> {
    let mut action = None;
    let w = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(vec2(w, ROW_H), Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(rect.shrink2(vec2(6.0, 2.0)), 10, HOVER);
    }
    if divider {
        hline(ui, rect.shrink2(vec2(16.0, 0.0)).x_range(), rect.bottom() - 0.5);
    }
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());

    // Actions on the right.
    let buttons = Rect::from_min_max(pos2(rect.right() - 16.0 - 3.0 * 32.0 - 8.0, rect.top()), pos2(rect.right() - 12.0, rect.bottom()));
    let mut b = child(ui, buttons, Layout::right_to_left(Align::Center));
    b.spacing_mut().item_spacing.x = 2.0;
    if square_button(&mut b, icons::X16, 32.0, 16.0, tr("Delete")).clicked() {
        history::remove(e.id);
    }
    let copied = hs.copied.is_some_and(|(id, t)| id == e.id && t.elapsed() < Duration::from_millis(1500));
    if square_button(&mut b, icons::COPY17, 32.0, 16.0, if copied { tr("Copied") } else { tr("Copy") }).clicked() {
        hs.copied = Some((e.id, Instant::now()));
        action = Some(HistoryAction::Copy(e.translation.clone()));
    }
    let (star, tip) = if e.starred { (icons::STAR_ON, tr("Remove from favourites")) } else { (icons::STAR, tr("Add to favourites")) };
    if square_button(&mut b, star, 32.0, 16.0, tip).clicked() {
        history::set_starred(e.id, !e.starred);
    }

    // Texts.
    let body = Rect::from_min_max(pos2(rect.left() + 18.0, rect.top() + 8.0), pos2(buttons.left() - 12.0, rect.bottom() - 6.0));
    let mut t = child(ui, body, Layout::top_down(Align::Min));
    t.spacing_mut().item_spacing.y = 4.0;
    t.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        let from = if e.from == "auto" { "?".to_owned() } else { lang_badge(&e.from) };
        let badge = format!("{from} → {}", lang_badge(&e.to));
        let g = ui.painter().layout_no_wrap(badge, font(11.0, Weight::SemiBold), TEXT_2);
        let (r, _) = ui.allocate_exact_size(g.size() + vec2(12.0, 4.0), Sense::hover());
        ui.painter().rect_filled(r, 4, SELECTED);
        ui.painter().galley(r.min + vec2(6.0, 2.0), g, TEXT_2);
        let meta = format!("{} · {} · {}", relative_time(e.time, now), e.provider.short_name(), mode_label(e.mode));
        text(ui, meta, 11.0, Weight::Regular, TEXT_3);
    });
    let one_line = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ");
    t.add(Label::new(rt(one_line(&e.source), 13.0, Weight::Regular, TEXT_3)).truncate().selectable(false));
    t.add(Label::new(rt(one_line(&e.translation), 15.0, Weight::Regular, TEXT)).truncate().selectable(false));

    if action.is_none() && resp.on_hover_cursor(CursorIcon::PointingHand).clicked() {
        action = Some(HistoryAction::Open(e.clone()));
    }
    action
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates() {
        assert_eq!(civil_date(0), (1970, 1, 1));
        assert_eq!(civil_date(1_700_000_000), (2023, 11, 14));
        assert_eq!(civil_date(951_782_400), (2000, 2, 29));
    }
}
