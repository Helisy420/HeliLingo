//! `Режим «Ультра»` — the Ultra window: one row per fragment of the
//! selection with its language badge, the original and the translation
//! ("unchanged" for fragments already in the target language), a summary of
//! the languages found, and Copy all / Replace selection.

use egui::{Align, Id, Key, Layout, Margin, Rect, ScrollArea, Sense, Stroke, Ui, UiBuilder, pos2, vec2};

use super::widgets::*;
use crate::i18n::{is_russian, plural, tr};
use crate::icons;
use crate::settings::{ProviderKind, lang_badge, lang_name};
use crate::theme::*;
use crate::translate::Error;
use crate::ultra::{Fragment, FragmentState, LangGroup};

pub const CARD_W: f32 = 680.0;
pub const MARGIN_X: f32 = 28.0;
pub const MARGIN_TOP: f32 = 16.0;
pub const MARGIN_BOTTOM: f32 = 48.0;
/// Room for the provider menu below the card.
pub const MENU_ROOM: f32 = 200.0;
/// The fragment list scrolls beyond this height.
const LIST_MAX_H: f32 = 380.0;

#[derive(Default)]
pub struct UltraState {
    menu: Option<Rect>,
    /// First frame after the window was shown (start of the unfold).
    opened_at: Option<std::time::Instant>,
    /// Card height of the last frame (for sizing the window).
    pub card_h: f32,
}

impl UltraState {
    /// Esc from the input hook: closes the provider menu first; returns
    /// false when there was nothing to close.
    pub fn escape(&mut self) -> bool {
        self.menu.take().is_some()
    }
}

pub enum UltraAction {
    Close,
    CopyAll,
    Replace,
    /// Translate again with this provider first (`None` = settings order).
    Provider(Option<ProviderKind>),
}

pub struct UltraView<'a> {
    pub fragments: &'a [Fragment],
    pub groups: &'a [LangGroup],
    /// Provider chip label (the provider that answered, or the preferred one).
    pub provider: &'static str,
    pub providers: &'a [ProviderKind],
    pub prefer: Option<ProviderKind>,
    pub copied: bool,
    /// Play the open animation, and whether the window is on screen yet.
    pub animate: bool,
    pub shown: bool,
}

fn group_color(i: usize, g: &LangGroup) -> egui::Color32 {
    if g.unchanged {
        ULTRA_UNCHANGED
    } else {
        ULTRA_LANG[i.min(ULTRA_LANG.len() - 1)]
    }
}

/// "3 languages · 5 fragments" / "3 языка · 5 фрагментов".
fn summary_label(languages: usize, fragments: usize) -> String {
    let (l, f) = if is_russian() {
        (plural(languages, "язык", "языка", "языков"), plural(fragments, "фрагмент", "фрагмента", "фрагментов"))
    } else {
        (
            if languages == 1 { "language" } else { "languages" },
            if fragments == 1 { "fragment" } else { "fragments" },
        )
    };
    format!("{languages} {l} · {fragments} {f}")
}

pub fn show(ui: &mut Ui, st: &mut UltraState, v: &UltraView) -> Option<UltraAction> {
    let mut action = None;
    let ctx = ui.ctx().clone();
    let progress = if !v.animate {
        None
    } else if !v.shown {
        Some(0.0)
    } else {
        let opened = *st.opened_at.get_or_insert_with(std::time::Instant::now);
        unfold_progress(opened.elapsed().as_secs_f32())
    };
    if progress.is_some() {
        ctx.request_repaint();
    }
    let full = Rect::from_min_size(pos2(MARGIN_X, MARGIN_TOP), vec2(CARD_W, if st.card_h > 0.0 { st.card_h } else { 486.0 }));
    let card = egui::Area::new(Id::new("qt-ultra"))
        .order(egui::Order::Middle)
        .fixed_pos(pos2(MARGIN_X, MARGIN_TOP))
        .constrain(false)
        .show(&ctx, |ui| {
            unfold_card(ui, progress, full, 14, PANEL, |ui| {
                    ui.set_width(CARD_W - 2.0);
                    ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                    header(ui, v, &mut action);
                    languages(ui, v);
                    fragments(ui, v);
                    footer(ui, st, v, &mut action);
                });
        })
        .response
        .rect;
    st.card_h = card.height();

    if let Some(anchor) = st.menu {
        let mut items = vec![(tr("Automatic").to_owned(), v.prefer.is_none())];
        items.extend(v.providers.iter().map(|k| (k.short_name().to_owned(), v.prefer == Some(*k))));
        let (picked, rect) = floating_list(&ctx, "qt-ultra-menu", anchor, 170.0, &items, None);
        if let Some(i) = picked {
            st.menu = None;
            action = Some(UltraAction::Provider(if i == 0 { None } else { Some(v.providers[i - 1]) }));
        } else if ctx.input(|i| {
            i.pointer.any_pressed()
                && i.pointer.interact_pos().is_some_and(|p| !rect.contains(p) && !anchor.contains(p))
        }) {
            st.menu = None;
        }
    }
    if ctx.input(|i| i.key_pressed(Key::Escape)) {
        if st.menu.is_some() {
            st.menu = None;
        } else {
            action = Some(UltraAction::Close);
        }
    }
    action
}

/// Zap tile, title, summary badge, close (`pt 14 pb 10 pl 18 pr 10`).
fn header(ui: &mut Ui, v: &UltraView, action: &mut Option<UltraAction>) {
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(vec2(w, 54.0), Sense::hover());
    let drag = ui.interact(rect, Id::new("qt-ultra-drag"), Sense::click_and_drag());
    if drag.drag_started() {
        crate::win::begin_window_drag();
    }
    let mut row = ui.new_child(
        UiBuilder::new()
            .max_rect(Rect::from_min_max(rect.min + vec2(18.0, 14.0), rect.max - vec2(10.0, 10.0)))
            .layout(Layout::left_to_right(Align::Center)),
    );
    row.spacing_mut().item_spacing.x = 10.0;
    let (tile, _) = row.allocate_exact_size(vec2(28.0, 28.0), Sense::hover());
    row.painter().rect_filled(tile, 8, SELECTED);
    egui::Image::new(icons::ZAP).paint_at(&row, Rect::from_center_size(tile.center(), vec2(15.0, 15.0)));
    text(&mut row, tr("Ultra mode"), 15.0, Weight::SemiBold, TEXT);
    // Languages that needed translating; the unchanged target language
    // isn't counted (as in the design: 3 languages, Russian left as is).
    let langs = v.groups.iter().filter(|g| !g.unchanged).count();
    if !v.fragments.is_empty() {
        egui::Frame::new()
            .fill(SELECTED)
            .corner_radius(10)
            .inner_margin(Margin::symmetric(9, 3))
            .show(&mut row, |ui| {
                text(ui, summary_label(langs.max(1), v.fragments.len()), 12.0, Weight::Medium, ACCENT);
            });
    }
    row.with_layout(Layout::right_to_left(Align::Center), |ui| {
        if square_button(ui, icons::X15_MUTED, 30.0, 15.0, tr("Close")).clicked() {
            *action = Some(UltraAction::Close);
        }
    });
}

/// Segmented bar (6px, one segment per language, width by count) and the legend.
fn languages(ui: &mut Ui, v: &UltraView) {
    egui::Frame::new()
        .inner_margin(Margin { left: 18, right: 18, top: 4, bottom: 14 })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 10.0;
            let (bar, _) = ui.allocate_exact_size(vec2(ui.available_width(), 6.0), Sense::hover());
            let total: usize = v.groups.iter().map(|g| g.count).sum();
            let pending = v.fragments.len().saturating_sub(total);
            let gaps = (v.groups.len() + usize::from(pending > 0)).saturating_sub(1) as f32 * 2.0;
            let unit = (bar.width() - gaps) / v.fragments.len().max(1) as f32;
            let mut x = bar.left();
            for (i, g) in v.groups.iter().enumerate() {
                let r = Rect::from_min_size(pos2(x, bar.top()), vec2(unit * g.count as f32, 6.0));
                ui.painter().rect_filled(r, 3, group_color(i, g));
                x = r.right() + 2.0;
            }
            if pending > 0 {
                let r = Rect::from_min_max(pos2(x, bar.top()), bar.max);
                ui.painter().rect_filled(r, 3, SKELETON);
            }
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 16.0;
                for (i, g) in v.groups.iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        let (dot, _) = ui.allocate_exact_size(vec2(7.0, 7.0), Sense::hover());
                        ui.painter().circle_filled(dot.center(), 3.5, group_color(i, g));
                        let mut label = format!("{} · {}", lang_name(&g.lang), g.count);
                        if g.unchanged {
                            label = format!("{label} — {}", tr("unchanged"));
                        }
                        text(ui, label, 12.0, Weight::Regular, TEXT_2);
                    });
                }
                if v.groups.is_empty() {
                    text(ui, tr("Detecting languages…"), 12.0, Weight::Regular, TEXT_3);
                }
            });
        });
}

/// One row per fragment: badge, original (13, muted), translation (16).
fn fragments(ui: &mut Ui, v: &UltraView) {
    let w = ui.available_width();
    let top = ui.cursor().min.y;
    ui.painter().hline(ui.max_rect().x_range(), top + 0.5, Stroke::new(1.0, BORDER));
    ScrollArea::vertical().max_height(LIST_MAX_H).auto_shrink([false, true]).show(ui, |ui| {
        ui.set_width(w);
        let n = v.fragments.len();
        for (i, f) in v.fragments.iter().enumerate() {
            let row = egui::Frame::new()
                .inner_margin(Margin::symmetric(18, 12))
                .show(ui, |ui| {
                    ui.set_width(w - 36.0);
                    ui.horizontal_top(|ui| {
                        ui.spacing_mut().item_spacing.x = 14.0;
                        let (badge, _) = ui.allocate_exact_size(vec2(32.0, 22.0), Sense::hover());
                        ui.painter().rect_filled(badge, 5, HOVER);
                        let code = f.lang.as_deref().map(lang_badge).unwrap_or_else(|| "··".into());
                        ui.painter().text(
                            badge.center(),
                            egui::Align2::CENTER_CENTER,
                            code,
                            font(11.0, Weight::SemiBold),
                            TEXT_2,
                        );
                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing.y = 4.0;
                            ui.set_width(ui.available_width());
                            ui.add(egui::Label::new(rt(&f.text, 13.0, Weight::Regular, TEXT_3)).wrap());
                            match &f.state {
                                FragmentState::Translated(t) => {
                                    ui.add(egui::Label::new(rt(t, 16.0, Weight::Regular, TEXT)).wrap().selectable(true));
                                }
                                FragmentState::Unchanged => {
                                    text(ui, tr("unchanged"), 12.0, Weight::Regular, TEXT_3);
                                }
                                FragmentState::Pending => {
                                    let t = ui.input(|i| i.time) as f32;
                                    let a = 0.7 + 0.3 * (t * 4.0).sin();
                                    let (r, _) = ui.allocate_exact_size(vec2(220.0, 16.0), Sense::hover());
                                    ui.painter().rect_filled(r, 4, SKELETON.gamma_multiply(a));
                                    ui.ctx().request_repaint();
                                }
                                FragmentState::Failed(e) => {
                                    let msg = match e {
                                        Error::Offline => tr("Can’t reach the translation service"),
                                        Error::Busy => tr("The translation service is busy"),
                                        Error::Other(m) => crate::i18n::tr_text(m),
                                    };
                                    text(ui, msg, 12.0, Weight::Regular, WARN);
                                }
                            }
                        });
                    });
                });
            if i + 1 < n {
                let y = row.response.rect.bottom() - 0.5;
                ui.painter().hline(row.response.rect.x_range(), y, Stroke::new(1.0, BORDER));
            }
        }
    });
}

/// Provider chip, `[Esc] close`, Copy all, Replace selection.
fn footer(ui: &mut Ui, st: &mut UltraState, v: &UltraView, action: &mut Option<UltraAction>) {
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(vec2(w, 61.0), Sense::hover());
    ui.painter().hline(rect.x_range(), rect.top() + 0.5, Stroke::new(1.0, BORDER));
    let mut row = ui.new_child(
        UiBuilder::new()
            .max_rect(Rect::from_min_max(rect.min + vec2(18.0, 0.0), rect.max - vec2(16.0, 0.0)))
            .layout(Layout::left_to_right(Align::Center)),
    );
    row.spacing_mut().item_spacing.x = 10.0;
    let r = chip(&mut row, v.provider, st.menu.is_some());
    if r.clicked() {
        st.menu = if st.menu.is_some() { None } else { Some(r.rect) };
    }
    key_hint(&mut row, "Esc", tr("close"));
    let done = v.fragments.iter().all(|f| !matches!(f.state, FragmentState::Pending));
    row.with_layout(Layout::right_to_left(Align::Center), |ui| {
        ui.spacing_mut().item_spacing.x = 10.0;
        ui.add_enabled_ui(done, |ui| {
            if button(ui, tr("Replace selection"), true).clicked() {
                *action = Some(UltraAction::Replace);
            }
            let copy = if v.copied { tr("Copied") } else { tr("Copy all") };
            if button(ui, copy, false).clicked() {
                *action = Some(UltraAction::CopyAll);
            }
        });
    });
}
