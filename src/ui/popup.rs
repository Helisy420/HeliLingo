//! The translation popup: Loading, Word, Sentence, Compact and Offline
//! states from the "Quick popup" page.
//!
//! Picking: in the Word popup the main translation and every alternative
//! can be clicked to pick it — Copy and Replace act on the pick. In the
//! Sentence popup any word can be clicked (Shift+click extends the range,
//! dragging across words selects a run of them), which adds a "Copy word"
//! action next to "Copy all" and makes Replace use the picked words.

use std::sync::Arc;
use std::time::{Duration, Instant};

use egui::{Align2, Area, Id, Margin, Order, Pos2, Rect, ScrollArea, Sense, Ui, pos2, vec2};

use super::widgets::*;
use super::wiki_card;
use crate::wiki::Query;
use crate::icons;
use crate::i18n::{tr, tr_text, trf};
use crate::settings::{CompactVariants, ProviderKind, lang_name};
use crate::theme::*;
use crate::translate::{Error, Translation};

pub enum Action {
    Copy(String, CopyKind),
    Replace(String),
    Speak(String, String),
    Retry,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CopyKind {
    /// The whole translation (or the picked word in the Word popup).
    All,
    /// Words picked inside a sentence.
    Part,
}

/// "Copied" confirmation shown for a moment after copying.
#[derive(Default)]
pub struct Feedback {
    copied: Option<(Instant, CopyKind)>,
}

impl Feedback {
    pub const DURATION: Duration = Duration::from_millis(1400);

    pub fn copied(&mut self, kind: CopyKind) {
        self.copied = Some((Instant::now(), kind));
    }

    pub fn showing(&self, kind: CopyKind) -> bool {
        matches!(self.copied, Some((t, k)) if k == kind && t.elapsed() < Self::DURATION)
    }

    pub fn active(&self) -> bool {
        self.copied.is_some_and(|(t, _)| t.elapsed() < Self::DURATION)
    }
}

pub struct WordView {
    pub t: Arc<Translation>,
    /// Main translation followed by every distinct dictionary term.
    options: Vec<String>,
    /// Index into `options`; 0 = main translation.
    picked: usize,
    expanded: bool,
    /// The Wikipedia card is open.
    wiki: bool,
}

impl WordView {
    pub fn new(t: Arc<Translation>) -> Self {
        let mut options = vec![t.text.clone()];
        options.extend(t.alternatives(usize::MAX));
        Self { t, options, picked: 0, expanded: false, wiki: false }
    }

    /// Opens the Wikipedia card (`--preview wiki`).
    pub fn open_wiki(&mut self) {
        self.wiki = true;
    }

    fn index_of(&self, term: &str) -> Option<usize> {
        let lower = term.to_lowercase();
        self.options.iter().position(|o| o.to_lowercase() == lower)
    }
}

pub struct Token {
    pub text: String,
    pub newline_before: bool,
}

pub struct SentenceView {
    pub t: Arc<Translation>,
    tokens: Vec<Token>,
    /// Picked token range (inclusive).
    picked: Option<(usize, usize)>,
    /// Token a drag-selection started on, while the button is held.
    drag_from: Option<usize>,
    /// The Wikipedia card is open.
    wiki: bool,
}

impl SentenceView {
    pub fn new(t: Arc<Translation>) -> Self {
        let mut tokens = Vec::new();
        for (li, line) in t.text.split('\n').enumerate() {
            for (wi, word) in line.split_whitespace().enumerate() {
                tokens.push(Token { text: word.to_owned(), newline_before: li > 0 && wi == 0 });
            }
        }
        Self { t, tokens, picked: None, drag_from: None, wiki: false }
    }

    fn picked_text(&self) -> Option<String> {
        let (a, b) = self.picked?;
        let joined = self.tokens[a..=b]
            .iter()
            .map(|t| t.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        Some(joined.trim_matches(|c: char| !c.is_alphanumeric()).to_owned())
    }
}

pub enum Content {
    Loading { source: String, compact: bool },
    Word(WordView),
    Sentence(SentenceView),
    Compact(CompactView),
    /// `to` is the target language (for "Open in Google Translate").
    Error { source: String, error: Error, to: String },
}

impl Content {
    /// Fixed card widths from the design; `None` = hug content.
    pub fn card_width(&self) -> Option<f32> {
        match self {
            Content::Loading { compact: true, .. } | Content::Compact(_) => None,
            Content::Loading { .. } | Content::Word(_) => Some(300.0),
            // Russian button labels are longer: a little more room.
            Content::Sentence(v) => Some(
                if v.t.text.chars().count() > 220 { 460.0 } else { 380.0 }
                    + if crate::i18n::is_russian() { 20.0 } else { 0.0 },
            ),
            Content::Error { .. } => Some(320.0),
        }
    }

    /// Rough height, used only to decide whether the popup fits below the anchor.
    pub fn estimated_height(&self) -> f32 {
        let wiki = if self.wiki_open() { wiki_card::ESTIMATED_H + wiki_card::GAP } else { 0.0 };
        wiki + match self {
            Content::Loading { compact: true, .. } => 34.0,
            // One line per ~34 characters at the wrap width, up to the scroll limit.
            Content::Compact(v) => v.estimated_height(),
            Content::Loading { .. } => 130.0,
            Content::Word(_) => 190.0,
            Content::Sentence(v) => {
                let lines = (v.t.text.chars().count() as f32 / 34.0).ceil().min(10.0);
                90.0 + lines * 24.0
            }
            Content::Error { .. } => 130.0,
        }
    }

    /// What Ctrl+C copies: the picked translation in the Word popup, the
    /// picked words (else the whole translation) in the Sentence popup.
    pub fn copy_text(&self) -> Option<String> {
        match self {
            Content::Word(v) => Some(v.options[v.picked].clone()),
            Content::Sentence(v) => Some(v.picked_text().unwrap_or_else(|| v.t.text.clone())),
            Content::Compact(v) => Some(v.text()),
            Content::Loading { .. } | Content::Error { .. } => None,
        }
    }

    /// Whether the Wikipedia card is shown under (or above) the card.
    pub fn wiki_open(&self) -> bool {
        match self {
            Content::Word(v) => v.wiki,
            Content::Sentence(v) => v.wiki,
            _ => false,
        }
    }

    /// What the Wikipedia card looks up, while it is open.
    fn wiki_query(&self) -> Option<Query> {
        let t = match self {
            Content::Word(v) if v.wiki => &v.t,
            Content::Sentence(v) if v.wiki => &v.t,
            _ => return None,
        };
        Some(Query::new(&t.source, t.src_lang.as_deref(), Some(&t.text), &t.tgt_lang))
    }

    fn close_wiki(&mut self) {
        match self {
            Content::Word(v) => v.wiki = false,
            Content::Sentence(v) => v.wiki = false,
            _ => {}
        }
    }

    pub fn is_settled(&self) -> bool {
        !matches!(self, Content::Loading { .. })
    }
}

fn preview(source: &str) -> String {
    let one_line = source.split_whitespace().collect::<Vec<_>>().join(" ");
    if one_line.chars().count() > 38 {
        format!("{}…", one_line.chars().take(37).collect::<String>())
    } else {
        one_line
    }
}

/// Draws the popup card at `origin` and returns the user's action and the
/// card's rect (window points).
pub fn show(
    ctx: &egui::Context,
    content: &mut Content,
    origin: Pos2,
    pivot: Align2,
    feedback: &Feedback,
) -> (Option<Action>, Rect) {
    let mut action = None;
    let resp = Area::new(Id::new("qt-popup"))
        .order(Order::Foreground)
        .fixed_pos(origin)
        .pivot(pivot)
        .constrain(false)
        .show(ctx, |ui| {
            let fixed_inner = content.card_width().map(|w| w - 34.0);
            match content {
                Content::Loading { source, compact: false } => loading(ui, source, fixed_inner),
                Content::Loading { compact: true, .. } => loading_compact(ui),
                Content::Word(v) => word(ui, v, fixed_inner, feedback, &mut action),
                Content::Sentence(v) => sentence(ui, v, fixed_inner, feedback, &mut action),
                Content::Compact(v) => compact(ui, v, feedback, &mut action),
                Content::Error { source, error, to } => {
                    error_view(ui, source, error, to, fixed_inner, &mut action)
                }
            }
        });
    let card = resp.response.rect;
    let Some(q) = content.wiki_query() else { return (action, card) };
    // The Wikipedia card sits under the popup card, or above it when the
    // popup opened above the selection; same width.
    let (pos, wiki_pivot) = if pivot == Align2::LEFT_BOTTOM {
        (pos2(card.left(), card.top() - wiki_card::GAP), Align2::LEFT_BOTTOM)
    } else {
        (pos2(card.left(), card.bottom() + wiki_card::GAP), Align2::LEFT_TOP)
    };
    let repaint = ctx.clone();
    let (rect, close) =
        wiki_card::area(ctx, "qt-popup-wiki", pos, wiki_pivot, &q, card.width(), move || repaint.request_repaint());
    if close {
        content.close_wiki();
    }
    (action, card.union(rect))
}

/// Header buttons besides the speaker: "Open in Google Translate" and,
/// when enabled, the Wikipedia toggle.
struct HeaderLinks<'a> {
    source: &'a str,
    from: &'a str,
    to: &'a str,
    wiki: &'a mut bool,
}

impl HeaderLinks<'_> {
    /// Drawn right to left: Wikipedia, then Google Translate.
    fn show(self, ui: &mut Ui) {
        if crate::wiki::enabled() {
            let (rect, r) = ui.allocate_exact_size(vec2(15.0, 15.0), Sense::click());
            if *self.wiki {
                ui.painter().rect_filled(rect.expand(4.0), 5, SELECTED);
            } else if r.hovered() {
                ui.painter().rect_filled(rect.expand(4.0), 5, HOVER);
            }
            egui::Image::new(icons::WIKI15).paint_at(ui, rect);
            let r = r.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(tr("Wikipedia article"));
            if r.clicked() {
                *self.wiki = !*self.wiki;
            }
        } else {
            *self.wiki = false;
        }
        if icon_button(ui, icons::EXTERNAL15, 15.0, tr("Open in Google Translate")).clicked() {
            crate::web::open_google_translate(self.source, self.from, self.to);
        }
    }
}

fn pulse(ui: &Ui) -> f32 {
    let t = ui.input(|i| i.time) as f32;
    0.7 + 0.3 * (t * 4.0).sin()
}

fn skeleton(ui: &mut Ui, w: f32, h: f32, alpha: f32) {
    let (rect, _) = ui.allocate_exact_size(vec2(w, h), Sense::hover());
    ui.painter().rect_filled(rect, 4, SKELETON.gamma_multiply(alpha));
}

/// `Popup / Loading`
fn loading(ui: &mut Ui, source: &str, inner: Option<f32>) {
    let a = pulse(ui);
    surface(ui, 12, POPUP_PADDING, |ui| {
        if let Some(w) = inner {
            ui.set_width(w);
        }
        ui.spacing_mut().item_spacing.y = 10.0;
        text(ui, preview(source), 13.0, Weight::Regular, TEXT_2);
        skeleton(ui, 170.0, 24.0, a);
        skeleton(ui, 96.0, 12.0, a);
        ui.add_space(-6.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            icon(ui, icons::DOT_LOADING, vec2(5.0, 5.0));
            text(ui, tr("Translating…"), 12.0, Weight::Regular, TEXT_3);
        });
    });
    ui.ctx().request_repaint();
}

fn loading_compact(ui: &mut Ui) {
    let a = pulse(ui);
    surface(ui, 8, Margin::symmetric(12, 7), |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 10.0;
            ui.set_height(18.0);
            skeleton(ui, 90.0, 14.0, a);
            icon(ui, icons::VOL_COMPACT, vec2(14.0, 14.0));
        });
    });
    ui.ctx().request_repaint();
}

/// Header row: muted label on the left; speaker, Google Translate and
/// Wikipedia on the right.
fn header(ui: &mut Ui, label: String, size: f32, color: egui::Color32, speak: impl FnOnce(), links: HeaderLinks) {
    split_row(
        ui,
        16.0,
        |ui| {
            text(ui, label, size, Weight::Regular, color);
        },
        |ui| {
            ui.spacing_mut().item_spacing.x = 12.0;
            if icon_button(ui, icons::VOL, 15.0, tr("Listen")).clicked() {
                speak();
            }
            links.show(ui);
        },
    );
}

/// Picks shown as a comma-separated, wrapping list of chips.
fn chip_list(ui: &mut Ui, view: &mut WordView, terms: &[String]) {
    let space = space_width(ui, 13.0, Weight::Regular);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
        for (i, term) in terms.iter().enumerate() {
            let idx = view.index_of(term);
            let picked = idx.is_some_and(|i| i == view.picked && i != 0);
            let r = word_chip(ui, term, 13.0, Weight::Regular, TEXT_2, 16.0, picked);
            if r.on_hover_text(tr("Pick this translation")).clicked()
                && let Some(i) = idx
            {
                view.picked = if view.picked == i { 0 } else { i };
            }
            if i + 1 < terms.len() {
                text(ui, ",", 13.0, Weight::Regular, TEXT_2);
                ui.add_space(space);
            }
        }
    });
}

/// `Popup / Word`
fn word(
    ui: &mut Ui,
    v: &mut WordView,
    inner: Option<f32>,
    feedback: &Feedback,
    action: &mut Option<Action>,
) {
    let inner = inner.unwrap_or(266.0);
    surface(ui, 12, POPUP_PADDING, |ui| {
        ui.set_width(inner);
        let pick = v.options[v.picked].clone();
        let lang = v.t.tgt_lang.clone();

        let t = v.t.clone();
        let from = t.src_lang.as_deref().unwrap_or("auto");
        let links = HeaderLinks { source: &t.source, from, to: &t.tgt_lang, wiki: &mut v.wiki };
        let speak = || *action = Some(Action::Speak(pick.clone(), lang.clone()));
        header(ui, t.source.clone(), 13.0, TEXT_2, speak, links);

        // Main translation (24 Medium). Long phrases step down so they fit.
        let main = v.options[0].clone();
        let size = if ui.painter().layout_no_wrap(main.clone(), font(24.0, Weight::Medium), TEXT).size().x > inner {
            18.0
        } else {
            24.0
        };
        let r = word_chip(ui, &main, size, Weight::Medium, TEXT, size * 1.21, false);
        if r.on_hover_text(tr("Pick this translation")).clicked() {
            v.picked = 0;
        }

        if let Some(pos) = v.t.pos() {
            text(ui, tr_text(pos), 12.0, Weight::Regular, TEXT_3);
        }

        if v.expanded {
            ui.add_space(4.0);
            let groups = v.t.dict.clone();
            ScrollArea::vertical().max_height(if v.wiki { 120.0 } else { 220.0 }).show(ui, |ui| {
                for g in groups.iter().take(6) {
                    if !g.pos.is_empty() {
                        text(ui, tr_text(&g.pos), 12.0, Weight::Regular, TEXT_3);
                    }
                    let terms: Vec<String> = g.terms.iter().take(10).cloned().collect();
                    chip_list(ui, v, &terms);
                    ui.add_space(4.0);
                }
            });
        } else {
            let alts: Vec<String> = v.options.iter().skip(1).take(3).cloned().collect();
            if !alts.is_empty() {
                ui.add_space(4.0);
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    text(ui, tr("also"), 13.0, Weight::Regular, TEXT_3);
                    ui.vertical(|ui| chip_list(ui, v, &alts));
                });
            }
        }

        divider(ui);

        let pick = v.options[v.picked].clone();
        let has_more = v.t.dict.len() > 1 || v.options.len() > 4;
        split_row(ui, 17.0, esc_hint, |ui| {
            ui.spacing_mut().item_spacing.x = 12.0;
            if has_more || v.expanded {
                let tip = if v.expanded { tr("Fewer meanings") } else { tr("All meanings") };
                if icon_button(ui, icons::EXPAND, 15.0, tip).clicked() {
                    v.expanded = !v.expanded;
                }
            }
            if feedback.showing(CopyKind::All) {
                let (rect, _) = ui.allocate_exact_size(vec2(15.0, 15.0), Sense::hover());
                egui::Image::new(icons::CHECK)
                    .paint_at(ui, Rect::from_center_size(rect.center(), vec2(14.0, 14.0)));
            } else if icon_button(ui, icons::COPY, 15.0, &trf("Copy “{w}”", &[("w", &pick)])).clicked() {
                *action = Some(Action::Copy(pick.clone(), CopyKind::All));
            }
            ui.add_space(4.0);
            let r = text_button(ui, tr("Replace"), 13.0, Weight::SemiBold, ACCENT, ACCENT_HOVER)
                .on_hover_text(trf("Replace the selection with “{w}”", &[("w", &pick)]));
            if r.clicked() {
                *action = Some(Action::Replace(pick.clone()));
            }
        });
    });
}

/// `Popup / Sentence`
fn sentence(
    ui: &mut Ui,
    v: &mut SentenceView,
    inner: Option<f32>,
    feedback: &Feedback,
    action: &mut Option<Action>,
) {
    let inner = inner.unwrap_or(346.0);
    surface(ui, 12, POPUP_PADDING, |ui| {
        ui.set_width(inner);
        let from = v.t.src_lang.as_deref().map(lang_name).unwrap_or_else(|| crate::i18n::tr("Detect").to_owned());
        let pair = format!("{from} → {}", lang_name(&v.t.tgt_lang));
        let (full, lang) = (v.t.text.clone(), v.t.tgt_lang.clone());
        let t = v.t.clone();
        let src = t.src_lang.as_deref().unwrap_or("auto");
        let links = HeaderLinks { source: &t.source, from: src, to: &t.tgt_lang, wiki: &mut v.wiki };
        let speak = || *action = Some(Action::Speak(full.clone(), lang.clone()));
        header(ui, pair, 12.0, TEXT_3, speak, links);

        // Translation: 16 Regular, line-height 1.5; every word is pickable,
        // by click, Shift+click or dragging across words.
        let space = space_width(ui, 16.0, Weight::Regular);
        let plate_h = ui.fonts_mut(|f| f.row_height(&font(16.0, Weight::Regular)));
        let shift = ui.input(|i| i.modifiers.shift);
        ScrollArea::vertical()
            .max_height(if v.wiki { 120.0 } else { 240.0 })
            .show(ui, |ui| {
                let mut rects = Vec::with_capacity(v.tokens.len());
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                    let n = v.tokens.len();
                    for i in 0..n {
                        if v.tokens[i].newline_before {
                            ui.end_row();
                        }
                        let picked = v.picked.is_some_and(|(a, b)| (a..=b).contains(&i));
                        let r = word_chip_sense(
                            ui,
                            &v.tokens[i].text,
                            16.0,
                            Weight::Regular,
                            TEXT,
                            24.0,
                            picked,
                            Sense::click_and_drag(),
                        );
                        if r.drag_started() {
                            v.drag_from = Some(i);
                            v.picked = Some((i, i));
                        } else if r.clicked() {
                            v.picked = match v.picked {
                                Some((a, b)) if shift => Some((a.min(i), b.max(i))),
                                Some((a, b)) if a == i && b == i => None,
                                _ => Some((i, i)),
                            };
                        }
                        // One continuous highlight across the spaces of a picked run.
                        if picked && i > 0 && v.picked.is_some_and(|(a, _)| a < i) {
                            let prev: Rect = rects[i - 1];
                            if (prev.top() - r.rect.top()).abs() < 1.0 {
                                let h = plate_h.min(r.rect.height());
                                let gap = Rect::from_x_y_ranges(
                                    prev.right()..=r.rect.left(),
                                    (r.rect.center().y - h / 2.0)..=(r.rect.center().y + h / 2.0),
                                );
                                ui.painter().rect_filled(gap, 0, HIGHLIGHT);
                            }
                        }
                        rects.push(r.rect);
                        if i + 1 < n && !v.tokens[i + 1].newline_before {
                            ui.add_space(space);
                        }
                    }
                });
                if let Some(from) = v.drag_from {
                    let (down, pos) = ui.input(|i| (i.pointer.primary_down(), i.pointer.interact_pos()));
                    if !down {
                        v.drag_from = None;
                    } else if let Some(to) = pos.and_then(|p| token_at(&rects, p, space)) {
                        v.picked = Some((from.min(to), from.max(to)));
                    }
                }
            });

        divider(ui);

        let part = v.picked_text();
        // With words picked the footer has three actions; the Esc hint
        // makes room for them (Esc still closes).
        let left = |ui: &mut Ui| {
            if v.picked.is_none() {
                esc_hint(ui);
            }
        };
        split_row(ui, 17.0, left, |ui| {
            ui.spacing_mut().item_spacing.x = if v.picked.is_some() { 12.0 } else { 16.0 };
            let (replacement, tip) = match &part {
                Some(p) => (p.clone(), trf("Replace the selection with “{w}”", &[("w", p)])),
                None => (v.t.text.clone(), tr("Replace the selection with the translation").to_owned()),
            };
            if text_button(ui, tr("Replace"), 13.0, Weight::SemiBold, ACCENT, ACCENT_HOVER)
                .on_hover_text(tip)
                .clicked()
            {
                *action = Some(Action::Replace(replacement));
            }
            let all_label = match (feedback.showing(CopyKind::All), part.is_some()) {
                (true, _) => tr("Copied"),
                (false, true) => tr("Copy all"),
                (false, false) => tr("Copy"),
            };
            if text_button(ui, all_label, 13.0, Weight::Medium, TEXT_2, TEXT).clicked() {
                *action = Some(Action::Copy(v.t.text.clone(), CopyKind::All));
            }
            if let Some(part) = part {
                let words = part.split_whitespace().count();
                let label = if feedback.showing(CopyKind::Part) {
                    tr("Copied").to_owned()
                } else if words == 1 {
                    tr("Copy word").to_owned()
                } else {
                    trf("Copy {n} words", &[("n", &words.to_string())])
                };
                if text_button(ui, label, 13.0, Weight::Medium, TEXT_2, TEXT)
                    .on_hover_text(trf("Copy “{w}”", &[("w", &part)]))
                    .clicked()
                {
                    *action = Some(Action::Copy(part, CopyKind::Part));
                }
            }
        });
    });
}

/// The word under (or, between words and past line ends, nearest to) the
/// pointer while drag-selecting. `None` when the pointer is above or below
/// every line.
fn token_at(rects: &[Rect], p: Pos2, space: f32) -> Option<usize> {
    rects
        .iter()
        .enumerate()
        .filter(|(_, r)| r.y_range().contains(p.y))
        .min_by(|(_, a), (_, b)| {
            let d = |r: &Rect| r.expand2(vec2(space / 2.0, 0.0)).distance_to_pos(p);
            d(a).total_cmp(&d(b))
        })
        .map(|(i, _)| i)
}

/// Compact popup text wraps at this width (points); longer text scrolls
/// past [`COMPACT_MAX_H`].
const COMPACT_WRAP: f32 = 380.0;
const COMPACT_MAX_H: f32 = 260.0;
/// Width of the provider column in the other translations' rows.
const VARIANT_LABEL_W: f32 = 70.0;

/// One translation in the compact popup: the chain's (index 0) or another
/// provider's.
pub struct CompactOption {
    pub provider: ProviderKind,
    /// `None` while it is loading.
    pub text: Option<String>,
    /// Failed, or the same as another option: not shown.
    pub hidden: bool,
}

/// `Popup / Compact tooltip` plus, optionally, other providers' translations
/// to pick from (Settings → General → Other services).
pub struct CompactView {
    pub t: Arc<Translation>,
    options: Vec<CompactOption>,
    /// The option shown big; Copy, Replace and Ctrl+C use it.
    picked: usize,
    style: CompactVariants,
    /// Menu style: the list is open.
    open: bool,
}

impl CompactView {
    /// `others`: the providers whose translations will arrive later
    /// ([`CompactView::set_variant`]).
    pub fn new(t: Arc<Translation>, style: CompactVariants, others: &[ProviderKind]) -> Self {
        let mut options = vec![CompactOption { provider: t.provider, text: Some(t.text.clone()), hidden: false }];
        if style != CompactVariants::Off {
            options.extend(others.iter().map(|&provider| CompactOption { provider, text: None, hidden: false }));
        }
        Self { t, options, picked: 0, style, open: false }
    }

    /// The translation in use.
    pub fn text(&self) -> String {
        self.options[self.picked].text.clone().unwrap_or_else(|| self.t.text.clone())
    }

    /// Another provider answered (`None` = it failed). A translation equal
    /// to one already shown is hidden: the list only offers differences.
    pub fn set_variant(&mut self, provider: ProviderKind, text: Option<String>) {
        let norm = |t: &str| t.trim().trim_end_matches(['.', '!', '?']).to_lowercase();
        let duplicate = text.as_deref().is_some_and(|new| {
            self.options.iter().any(|o| o.provider != provider && o.text.as_deref().is_some_and(|t| norm(t) == norm(new)))
        });
        if let Some(o) = self.options.iter_mut().find(|o| o.provider == provider) {
            o.hidden = text.is_none() || duplicate;
            o.text = text;
        }
    }

    /// Rows to list: every option but the picked one that isn't hidden.
    fn rows(&self) -> Vec<usize> {
        (0..self.options.len()).filter(|&i| i != self.picked && !self.options[i].hidden).collect()
    }

    fn ready(&self) -> usize {
        self.rows().iter().filter(|&&i| self.options[i].text.is_some()).count()
    }

    fn list_shown(&self) -> bool {
        match self.style {
            CompactVariants::Off => false,
            CompactVariants::List => !self.rows().is_empty(),
            CompactVariants::Menu => self.open && !self.rows().is_empty(),
        }
    }

    /// For placing the popup: about how tall the card is.
    fn estimated_height(&self) -> f32 {
        let lines = (self.text().chars().count() as f32 / 34.0).ceil().clamp(1.0, 13.0);
        let list = if self.style == CompactVariants::List { self.options.len().saturating_sub(1) as f32 * 24.0 + 8.0 } else { 0.0 };
        14.0 + lines * 20.0 + list
    }
}

/// `Popup / Compact tooltip`: one line for a word, a wrapped block for a
/// sentence, and the other services' translations when enabled.
fn compact(ui: &mut Ui, v: &mut CompactView, feedback: &Feedback, action: &mut Option<Action>) {
    let text = v.text();
    surface(ui, 8, Margin::symmetric(12, 7), |ui| {
        let galley = ui.painter().layout(text.clone(), font(15.0, Weight::Medium), TEXT, COMPACT_WRAP);
        let size = galley.size();
        let show_menu = v.style == CompactVariants::Menu && v.ready() > 0;
        // Width of the top row: text, the copied check, and the menu button.
        let top_w = size.x + if show_menu { 10.0 + 34.0 } else { 0.0 } + if feedback.showing(CopyKind::All) { 24.0 } else { 0.0 };
        let list = v.list_shown();
        let width = if list { top_w.max(300.0).min(COMPACT_WRAP + 60.0) } else { top_w };
        ui.set_width(width);

        ui.with_layout(egui::Layout::left_to_right(egui::Align::Min), |ui| {
            ui.spacing_mut().item_spacing.x = 10.0;
            let paint = |ui: &mut Ui| {
                let (rect, r) = ui.allocate_exact_size(size, Sense::click());
                let color = if r.hovered() { egui::Color32::WHITE } else { TEXT };
                ui.painter().galley_with_override_text_color(rect.min, galley, color);
                r.on_hover_cursor(egui::CursorIcon::PointingHand)
            };
            let r = if size.y > COMPACT_MAX_H {
                ScrollArea::vertical().max_height(COMPACT_MAX_H).auto_shrink([true, true]).show(ui, paint).inner
            } else {
                paint(ui)
            }
            .on_hover_text(tr("Click to copy · Shift+click to replace"));
            if r.clicked() {
                *action = Some(if ui.input(|i| i.modifiers.shift) {
                    Action::Replace(text.clone())
                } else {
                    Action::Copy(text.clone(), CopyKind::All)
                });
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                ui.spacing_mut().item_spacing.x = 10.0;
                if show_menu && menu_button(ui, v.ready(), v.open).clicked() {
                    v.open = !v.open;
                }
                if feedback.showing(CopyKind::All) {
                    icon(ui, icons::CHECK, vec2(14.0, 14.0));
                }
            });
        });

        if list {
            ui.add_space(4.0);
            let (line, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
            ui.painter().rect_filled(line, 0, BORDER);
            ui.add_space(3.0);
            for i in v.rows() {
                if let Some(pick) = variant_row(ui, &v.options[i]) {
                    if pick.shift {
                        if let Some(t) = &v.options[i].text {
                            *action = Some(Action::Replace(t.clone()));
                        }
                    } else {
                        v.picked = i;
                        v.open = false;
                    }
                }
            }
        }
    });
    if v.options.iter().any(|o| o.text.is_none() && !o.hidden) {
        ui.ctx().request_repaint_after(Duration::from_millis(60));
    }
}

/// A click on an option row (with Shift = replace with it at once).
struct Pick {
    shift: bool,
}

/// `Yandex   перевод…`: provider on the left, the translation wrapped on
/// the right; a pulsing bar while it loads.
fn variant_row(ui: &mut Ui, o: &CompactOption) -> Option<Pick> {
    let w = ui.available_width();
    let label = ui.painter().layout_no_wrap(o.provider.short_name().to_owned(), font(10.5, Weight::SemiBold), TEXT_3);
    let text_w = w - VARIANT_LABEL_W - 8.0;
    let galley = o
        .text
        .as_ref()
        .map(|t| ui.painter().layout(t.clone(), font(13.0, Weight::Regular), TEXT_2, text_w));
    let h = galley.as_ref().map_or(16.0, |g| g.size().y).max(16.0) + 6.0;
    let sense = if o.text.is_some() { Sense::click() } else { Sense::hover() };
    let (rect, r) = ui.allocate_exact_size(vec2(w, h), sense);
    if r.hovered() && o.text.is_some() {
        ui.painter().rect_filled(rect.expand2(vec2(4.0, 0.0)), 5, HOVER);
    }
    let lp = pos2(rect.left(), rect.top() + 3.0 + (16.0 - label.size().y) / 2.0 + 1.0);
    ui.painter().galley(lp, label, TEXT_3);
    let tx = rect.left() + VARIANT_LABEL_W;
    match galley {
        Some(g) => {
            let color = if r.hovered() { TEXT } else { TEXT_2 };
            ui.painter().galley_with_override_text_color(pos2(tx, rect.top() + 3.0), g, color);
        }
        None => {
            let bar = Rect::from_min_size(pos2(tx, rect.top() + 6.0), vec2(text_w.min(150.0), 10.0));
            ui.painter().rect_filled(bar, 4, SKELETON.gamma_multiply(pulse(ui)));
        }
    }
    if o.text.is_none() {
        return None;
    }
    let r = r.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(tr("Use this translation"));
    r.clicked().then(|| Pick { shift: ui.input(|i| i.modifiers.shift) })
}

/// Menu style: "3 ▾" opens and closes the list.
fn menu_button(ui: &mut Ui, count: usize, open: bool) -> egui::Response {
    let g = ui.painter().layout_no_wrap(count.to_string(), font(11.0, Weight::SemiBold), TEXT_2);
    let size = vec2(g.size().x + 8.0 + 12.0 + 12.0, 18.0);
    let (rect, r) = ui.allocate_exact_size(size, Sense::click());
    let fill = if open { SELECTED } else if r.hovered() { HOVER } else { egui::Color32::TRANSPARENT };
    ui.painter().rect_filled(rect, 5, fill);
    ui.painter().rect_stroke(rect, 5, egui::Stroke::new(1.0, BORDER), egui::StrokeKind::Inside);
    ui.painter().galley(pos2(rect.left() + 6.0, rect.center().y - g.size().y / 2.0), g, TEXT_2);
    let chev = Rect::from_center_size(pos2(rect.right() - 6.0 - 6.0, rect.center().y), vec2(12.0, 12.0));
    let img = egui::Image::new(icons::CHEV12);
    let img = if open { img.rotate(std::f32::consts::PI, vec2(0.5, 0.5)) } else { img };
    img.paint_at(ui, chev);
    r.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(tr("Other services' translations"))
}

/// `Popup / Offline` (and the other failure states, same layout).
fn error_view(
    ui: &mut Ui,
    source: &str,
    error: &Error,
    to: &str,
    inner: Option<f32>,
    action: &mut Option<Action>,
) {
    let (title, hint) = match error {
        Error::Offline => (
            tr("Can’t reach the translation service"),
            tr("Check your connection and try again"),
        ),
        Error::Busy => (tr("The translation service is busy"), tr("Wait a moment and try again")),
        Error::Other(msg) => (tr("Translation failed"), tr_text(msg)),
    };
    surface(ui, 12, POPUP_PADDING, |ui| {
        if let Some(w) = inner {
            ui.set_width(w);
        }
        text(ui, preview(source), 13.0, Weight::Regular, TEXT_2);
        ui.add_space(2.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            if matches!(error, Error::Offline) {
                icon(ui, icons::WIFI_OFF, vec2(16.0, 16.0));
            }
            text(ui, title, 14.0, Weight::Medium, TEXT);
        });
        text(ui, hint, 12.0, Weight::Regular, TEXT_3);
        divider(ui);
        split_row(ui, 17.0, esc_hint, |ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            let label = text_button(ui, tr("Retry"), 13.0, Weight::SemiBold, ACCENT, ACCENT_HOVER);
            let r = icon_button(ui, icons::REFRESH, 14.0, "");
            if label.clicked() || r.clicked() {
                *action = Some(Action::Retry);
            }
            // Busy or unreachable: Google Translate's web page usually
            // still works.
            ui.add_space(10.0);
            if icon_button(ui, icons::EXTERNAL15, 15.0, tr("Open in Google Translate")).clicked() {
                crate::web::open_google_translate(source, "auto", to);
            }
        });
    });
}
