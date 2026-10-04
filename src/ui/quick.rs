//! `Быстрое окно перевода` — the Quick window (Ctrl+Alt+T): type or paste
//! text, the translation appears as you type. Enter copies the result and
//! closes, Esc closes. Unlike the popup, this window takes focus.

use std::sync::Arc;

use egui::{
    Align, CornerRadius, Id, Key, KeyboardShortcut, Layout, Margin, Modifiers, Rect, Sense,
    Stroke, TextEdit, Ui, UiBuilder, pos2, vec2,
};

use super::widgets::*;
use crate::i18n::tr;
use crate::icons;
use crate::settings::{ProviderKind, lang_name, search_languages};
use crate::theme::*;
use crate::translate::{Error, Translation};

/// Card width from the design; the height follows the text.
pub const CARD_W: f32 = 600.0;
/// Transparent margin around the card for its shadow.
pub const MARGIN_X: f32 = 28.0;
pub const MARGIN_TOP: f32 = 16.0;
pub const MARGIN_BOTTOM: f32 = 48.0;
/// Room for the language menus below the card.
pub const MENU_ROOM: f32 = 300.0;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Menu {
    From,
    To,
    Provider,
}

#[derive(Default)]
pub struct QuickState {
    pub input: String,
    /// Source language code or "auto".
    pub from: String,
    pub to: String,
    /// Provider picked in the chip (tried first), `None` = settings order.
    pub prefer: Option<ProviderKind>,
    /// Latest result for `input`, or the error.
    pub result: Option<Result<Arc<Translation>, Error>>,
    pub loading: bool,
    menu: Option<(Menu, Rect)>,
    query: String,
    /// Frames drawn; the input grabs focus on the first ones.
    frames: u32,
    /// Card height of the last frame (for sizing the window).
    pub card_h: f32,
    /// The Wikipedia card under the window is open, and where its button is.
    wiki_open: bool,
    wiki_button: Option<Rect>,
    /// When the window first drew (start of the open animation).
    opened_at: Option<std::time::Instant>,
}

impl QuickState {
    /// Esc from the input hook: closes an open menu or the Wikipedia card
    /// first; returns false when there was nothing to close (close the window).
    pub fn escape(&mut self) -> bool {
        if self.menu.take().is_some() {
            return true;
        }
        if self.wiki_open {
            self.wiki_open = false;
            return true;
        }
        false
    }

    pub fn new(from: &str, to: &str) -> Self {
        Self { from: from.to_owned(), to: to.to_owned(), ..Default::default() }
    }

    fn translation(&self) -> Option<&Arc<Translation>> {
        match &self.result {
            Some(Ok(t)) => Some(t),
            _ => None,
        }
    }
}

/// Width of the Wikipedia card under the window.
const WIKI_W: f32 = 360.0;

pub enum QuickAction {
    Close,
    /// Copy the translation and close (Enter).
    CopyAndClose(String),
    Copy(String),
    Speak(String, String),
    /// Languages or provider changed: translate again now.
    Retranslate,
}

/// What the window shows besides its own state.
pub struct QuickView<'a> {
    /// Providers that can run, in settings order (the provider chip menu).
    pub providers: &'a [ProviderKind],
    /// Key caps of the Quick window shortcut, bottom right.
    pub keys: Vec<String>,
    /// Play the open animation (Settings → Animations, and Windows'
    /// "Show animations").
    pub animate: bool,
    /// The native window is on screen (it is shown only after the first
    /// frame was drawn, so the animation starts when it can be seen).
    pub shown: bool,
}

pub fn show(ui: &mut Ui, st: &mut QuickState, v: &QuickView) -> Option<QuickAction> {
    let mut action = None;
    let ctx = ui.ctx().clone();
    st.frames += 1;

    // Enter = copy and close; Shift+Enter = new line in the input.
    let enter = ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Enter));
    let escape = ctx.input(|i| i.key_pressed(Key::Escape));

    // Open animation: the card unfolds sideways from a pill at its centre
    // (see `unfold_card`). The native window keeps its size; repaints only
    // while it runs. Until the window is on screen the first frame is drawn
    // folded and the clock starts once it shows.
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
    let full = Rect::from_min_size(pos2(MARGIN_X, MARGIN_TOP), vec2(CARD_W, if st.card_h > 0.0 { st.card_h } else { 220.0 }));

    let card = egui::Area::new(Id::new("qt-quick"))
        .order(egui::Order::Middle)
        .fixed_pos(pos2(MARGIN_X, MARGIN_TOP))
        .constrain(false)
        .show(&ctx, |ui| {
            unfold_card(ui, progress, full, 16, PANEL, |ui| {
                ui.set_width(CARD_W - 2.0);
                ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                header(ui, st, v, &mut action);
                input(ui, st);
                result(ui, st, &mut action);
                footer(ui, v);
            });
        })
        .response
        .rect;
    st.card_h = card.height();

    // The Wikipedia card below the window card, under its W button (the
    // window keeps room below the card for menus).
    if let (true, Some(button)) = (st.wiki_open && st.menu.is_none(), st.wiki_button) {
        let t = st.translation().cloned();
        let detected = t.as_ref().and_then(|t| t.src_lang.clone());
        let from = detected.as_deref().or((st.from != "auto").then_some(st.from.as_str()));
        let q = crate::wiki::Query::new(&st.input, from, t.as_ref().map(|t| t.text.as_str()), &st.to);
        let viewport = ctx.viewport_id();
        let repaint = ctx.clone();
        let w = WIKI_W.min(CARD_W);
        let x = (button.right() + 12.0 - w).max(card.left());
        let pos = pos2(x, card.bottom() + crate::ui::wiki_card::GAP);
        let (_, close) = crate::ui::wiki_card::area(&ctx, "qt-quick-wiki", pos, egui::Align2::LEFT_TOP, &q, w, move || {
            repaint.request_repaint_of(viewport)
        });
        if close {
            st.wiki_open = false;
        }
    }

    if let Some((menu, anchor)) = st.menu {
        let items: Vec<(String, bool)>;
        let mut codes: Vec<String> = Vec::new();
        let search = match menu {
            Menu::From => {
                let mut list = vec![("auto".to_owned(), lang_name("auto"))];
                list.extend(search_languages(&st.query).into_iter().map(|(c, n)| (c.to_owned(), n.to_owned())));
                if !st.query.trim().is_empty() {
                    list.retain(|(c, _)| c != "auto");
                }
                items = list.iter().map(|(c, n)| (n.clone(), *c == st.from)).collect();
                codes = list.into_iter().map(|(c, _)| c).collect();
                Some(&mut st.query)
            }
            Menu::To => {
                let list = search_languages(&st.query);
                items = list.iter().map(|(c, n)| ((*n).to_owned(), *c == st.to)).collect();
                codes = list.into_iter().map(|(c, _)| c.to_owned()).collect();
                Some(&mut st.query)
            }
            Menu::Provider => {
                let mut list = vec![(tr("Automatic"), st.prefer.is_none())];
                list.extend(v.providers.iter().map(|k| (k.short_name(), st.prefer == Some(*k))));
                items = list.into_iter().map(|(n, sel)| (n.to_owned(), sel)).collect();
                None
            }
        };
        let width = if menu == Menu::Provider { 170.0 } else { 220.0 };
        let (picked, rect) = floating_list(&ctx, "qt-quick-menu", anchor, width, &items, search);
        if let Some(i) = picked {
            match menu {
                Menu::From => st.from = codes[i].clone(),
                Menu::To => st.to = codes[i].clone(),
                Menu::Provider => st.prefer = if i == 0 { None } else { Some(v.providers[i - 1]) },
            }
            st.menu = None;
            action = Some(QuickAction::Retranslate);
        } else {
            let outside = ctx.input(|i| {
                i.pointer.any_pressed()
                    && i.pointer.interact_pos().is_some_and(|p| !rect.contains(p) && !anchor.contains(p))
            });
            if outside {
                st.menu = None;
            }
        }
    }

    if escape {
        if st.menu.is_some() {
            st.menu = None;
        } else if st.wiki_open {
            st.wiki_open = false;
        } else {
            action = Some(QuickAction::Close);
        }
    } else if enter && st.menu.is_none() {
        action = Some(match &st.result {
            Some(Ok(t)) => QuickAction::CopyAndClose(t.text.clone()),
            _ => QuickAction::Close,
        });
    }
    action
}

/// Language chips, swap, provider chip, close.
fn header(ui: &mut Ui, st: &mut QuickState, v: &QuickView, action: &mut Option<QuickAction>) {
    let w = ui.available_width();
    // The header's empty space moves the window (the chips and buttons
    // below are added later, so they stay clickable on top of it).
    let (rect, bar) = ui.allocate_exact_size(vec2(w, 51.0), Sense::drag());
    if bar.drag_started() {
        crate::win::begin_window_drag();
    }
    ui.painter().hline(rect.x_range(), rect.bottom() - 0.5, Stroke::new(1.0, BORDER));
    let mut row = ui.new_child(
        UiBuilder::new()
            .max_rect(Rect::from_min_max(rect.min + vec2(14.0, 0.0), rect.max - vec2(10.0, 0.0)))
            .layout(Layout::left_to_right(Align::Center)),
    );
    row.spacing_mut().item_spacing.x = 8.0;
    let open = st.menu.map(|(m, _)| m);
    let toggle = |st: &mut QuickState, m: Menu, r: egui::Response| {
        if r.clicked() {
            st.menu = if open == Some(m) { None } else { Some((m, r.rect)) };
            st.query.clear();
        }
    };
    let from = if st.from == "auto" { tr("Detect").to_owned() } else { lang_name(&st.from) };
    let r = chip(&mut row, &from, open == Some(Menu::From));
    toggle(st, Menu::From, r);
    let swap = square_button(&mut row, icons::SWAP14, 22.0, 14.0, tr("Swap languages"));
    if swap.clicked() {
        let detected = match &st.result {
            Some(Ok(t)) => t.src_lang.clone(),
            _ => None,
        };
        let new_to = if st.from == "auto" { detected.unwrap_or_else(|| "en".into()) } else { st.from.clone() };
        st.from = std::mem::replace(&mut st.to, new_to);
        if let Some(Ok(t)) = &st.result {
            st.input = t.text.clone();
        }
        *action = Some(QuickAction::Retranslate);
    }
    let r = chip(&mut row, &lang_name(&st.to), open == Some(Menu::To));
    toggle(st, Menu::To, r);
    row.with_layout(Layout::right_to_left(Align::Center), |ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        if square_button(ui, icons::X15_MUTED, 30.0, 15.0, tr("Close")).clicked() {
            *action = Some(QuickAction::Close);
        }
        let label = match (&st.result, st.prefer) {
            (Some(Ok(t)), _) => t.provider.short_name(),
            (_, Some(k)) => k.short_name(),
            _ => v.providers.first().map_or("Google", |k| k.short_name()),
        };
        let r = chip(ui, label, open == Some(Menu::Provider));
        toggle(st, Menu::Provider, r);
    });
}

/// The input line: 22 Regular, pt 18 pb 16 px 20, accent caret.
fn input(ui: &mut Ui, st: &mut QuickState) {
    egui::Frame::new()
        .inner_margin(Margin { left: 20, right: 20, top: 18, bottom: 16 })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let edit = TextEdit::multiline(&mut st.input)
                .id(Id::new("qt-quick-input"))
                .frame(egui::Frame::NONE)
                .margin(Margin::ZERO)
                .font(font(22.0, Weight::Regular))
                .text_color(TEXT)
                .hint_text(rt(tr("Type or paste text"), 22.0, Weight::Regular, TEXT_3))
                .desired_rows(1)
                .desired_width(f32::INFINITY)
                .return_key(KeyboardShortcut::new(Modifiers::SHIFT, Key::Enter));
            let resp = ui.add(edit);
            if st.frames <= 3 || (!resp.has_focus() && st.menu.is_none()) {
                resp.request_focus();
            }
        });
}

/// The translation band (`#151d3a`), with speaker and copy.
fn result(ui: &mut Ui, st: &mut QuickState, action: &mut Option<QuickAction>) {
    st.wiki_button = None;
    if st.input.trim().is_empty() && st.result.is_none() {
        st.wiki_open = false;
        return;
    }
    let w = ui.available_width();
    let top = ui.cursor().min;
    let shape = ui.painter().add(egui::Shape::Noop);
    let inner = egui::Frame::new()
        .inner_margin(Margin { left: 20, right: 12, top: 16, bottom: 16 })
        .show(ui, |ui| {
            ui.set_width(w - 32.0);
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 10.0;
                let ok = matches!(st.result, Some(Ok(_)));
                let wiki = crate::wiki::enabled();
                if !wiki {
                    st.wiki_open = false;
                }
                // Speaker and copy with a result; Wikipedia and Google
                // Translate always (also when the service is busy).
                let buttons = if ok { 2 } else { 0 } + 1 + usize::from(wiki);
                let text_w = ui.available_width() - buttons as f32 * (36.0 + 10.0);
                ui.vertical(|ui| {
                    ui.set_width(text_w);
                    ui.set_min_height(36.0);
                    match &st.result {
                        Some(Ok(t)) => {
                            let color = if st.loading { TEXT_2 } else { TEXT };
                            ui.add(
                                egui::Label::new(rt(&t.text, 22.0, Weight::Regular, color))
                                    .wrap()
                                    .selectable(true),
                            );
                        }
                        Some(Err(e)) => {
                            let msg = match e {
                                Error::Offline => tr("Can’t reach the translation service"),
                                Error::Busy => tr("The translation service is busy"),
                                Error::Other(m) => crate::i18n::tr_text(m),
                            };
                            text(ui, msg, 14.0, Weight::Regular, WARN);
                        }
                        None => {
                            text(ui, tr("Translating…"), 22.0, Weight::Regular, TEXT_3);
                        }
                    }
                });
                if wiki {
                    let r = toggle_square_button(ui, icons::WIKI17, 36.0, 17.0, tr("Wikipedia article"), st.wiki_open);
                    if r.clicked() {
                        st.wiki_open = !st.wiki_open;
                        st.menu = None;
                    }
                    st.wiki_button = Some(r.rect);
                }
                if let Some(Ok(t)) = &st.result {
                    if square_button(ui, icons::VOL17, 36.0, 17.0, tr("Listen")).clicked() {
                        *action = Some(QuickAction::Speak(t.text.clone(), t.tgt_lang.clone()));
                    }
                    if square_button(ui, icons::COPY17, 36.0, 17.0, tr("Copy")).clicked() {
                        *action = Some(QuickAction::Copy(t.text.clone()));
                    }
                }
                if square_button(ui, icons::EXTERNAL17, 36.0, 17.0, tr("Open in Google Translate")).clicked() {
                    crate::web::open_google_translate(&st.input, &st.from, &st.to);
                }
            });
        });
    let band = Rect::from_min_size(top, vec2(w, inner.response.rect.height()));
    ui.painter().set(shape, egui::epaint::RectShape::filled(band, CornerRadius::ZERO, RESULT_BG));
}

/// `[Enter] copy and close  [Esc] close  ……  [Ctrl] [Alt] [T]`
fn footer(ui: &mut Ui, v: &QuickView) {
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(vec2(w, 38.0), Sense::hover());
    ui.painter().hline(rect.x_range(), rect.top() + 0.5, Stroke::new(1.0, BORDER));
    let mut row = ui.new_child(
        UiBuilder::new()
            .max_rect(rect.shrink2(vec2(16.0, 0.0)))
            .layout(Layout::left_to_right(Align::Center)),
    );
    row.spacing_mut().item_spacing.x = 16.0;
    key_hint(&mut row, "Enter", tr("copy and close"));
    key_hint(&mut row, "Esc", tr("close"));
    row.with_layout(Layout::right_to_left(Align::Center), |ui| {
        ui.spacing_mut().item_spacing.x = 3.0;
        for k in v.keys.iter().rev() {
            key_cap(ui, k);
        }
    });
}
