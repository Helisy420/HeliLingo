//! `Окно перевода` — the main translator window (Ctrl+Alt+Q, tray): a
//! Text tab that translates as you type, an Images tab (OCR + translation
//! of a picture, a pasted image or a screen area) and the History view.
//!
//! The window renders from [`MainState`]; workers in `app::main_win` fill
//! in results. What needs the app (settings, file dialogs, clipboard,
//! translation requests) comes back as [`MainAction`]s.

pub mod capture;
mod history_view;
pub mod image_tab;

use std::sync::Arc;
use std::time::Instant;

use egui::text::{LayoutJob, TextFormat};
use egui::{
    Align, CursorIcon, Image, Key, Layout, Rect, ScrollArea, Sense, Stroke, TextEdit, Ui,
    UiBuilder, pos2, vec2,
};

pub use history_view::HistoryState;
pub use image_tab::{ImageDoc, ImageSlot, RenderJob, Stage};

use super::widgets::*;
use crate::history::Entry;
use crate::i18n::{is_russian, tr, trf};
use crate::icons;
use crate::settings::{ProviderKind, lang_name, search_languages};
use crate::theme::*;
use crate::translate::{Error, MAX_CHARS, Translation};

/// Window card sizes from the Figma frames (text / empty image tab, and
/// the image tab with a result).
pub const CARD_W: f32 = 820.0;
pub const CARD_H: f32 = 410.0;
pub const CARD_H_RESULT: f32 = 500.0;
/// Transparent margin around the card for the window shadow.
pub const MARGIN_X: f32 = 32.0;
pub const MARGIN_TOP: f32 = 20.0;
pub const MARGIN_BOTTOM: f32 = 52.0;

/// Window size (points) for a card of height `card_h`.
pub fn window_size(card_h: f32) -> [f32; 2] {
    [CARD_W + 2.0 * MARGIN_X, card_h + MARGIN_TOP + MARGIN_BOTTOM]
}

/// Panel text size: 20 Regular, line height 1.4.
const TEXT_SIZE: f32 = 20.0;
const TEXT_LEADING: f32 = 1.4;
/// egui puts the extra leading below the glyphs, Figma splits it: move
/// the text down to match the frame.
const TEXT_NUDGE: f32 = 4.0;
/// Mode pills, language strips and panel footers (Figma 315:2).
const PILL_H: f32 = 32.0;
const STRIP_H: f32 = 34.0;
const PANEL_FOOT_H: f32 = 44.0;
/// Width of the Wikipedia card over the translation panel.
const WIKI_W: f32 = 340.0;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tab {
    #[default]
    Text,
    Image,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum View {
    #[default]
    Translator,
    History,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Menu {
    From,
    To,
    Provider,
}

/// What the window needs from Settings (refreshed by the app every frame).
#[derive(Clone, Default)]
pub struct Env {
    /// Providers that can run, in the user's order (provider chip menu).
    pub providers: Vec<ProviderKind>,
    /// Label of the main shortcut ("Double-tap Ctrl").
    pub shortcut: String,
    /// Key caps of the screen-area shortcut.
    pub screen_keys: Vec<String>,
    /// Key caps of the whole-screen shortcut.
    pub whole_screen_keys: Vec<String>,
    /// Settings → `show_wiki`: the Wikipedia button.
    pub wiki: bool,
}

pub struct MainState {
    /// `--preview main-image-zoom`: open the zoom viewer on the result.
    pub zoom_when_done: bool,
    pub tab: Tab,
    pub view: View,
    /// Source language code or "auto", and the target. Changing them here
    /// doesn't change Settings.
    pub from: String,
    pub to: String,
    /// Recently used languages shown in the strips, most recent first.
    pub from_recent: Vec<String>,
    pub to_recent: Vec<String>,
    /// Provider picked in the chip (tried first), `None` = settings order.
    pub prefer: Option<ProviderKind>,

    pub input: String,
    /// The input changed at this time; translate once it settles.
    pub edited_at: Option<Instant>,
    /// Bumped for every request; stale results are dropped.
    pub text_gen: u64,
    pub result: Option<Result<Arc<Translation>, Error>>,
    pub loading: bool,
    /// When the current result arrived, and whether it is in history.
    pub result_at: Option<Instant>,
    pub recorded: bool,

    pub image: ImageSlot,
    pub image_gen: u64,

    pub history: HistoryState,
    pub env: Env,

    menu: Option<(Menu, Rect)>,
    query: String,
    copied_at: Option<Instant>,
    /// The Wikipedia card over the translation panel is open.
    wiki_open: bool,
    /// Ctrl+V was down last frame (image paste is edge-triggered).
    paste_down: bool,
    /// Frames drawn since the window opened (initial focus).
    pub frames: u32,
}

impl MainState {
    pub fn new(target: &str) -> Self {
        let mut to_recent = vec![target.to_owned()];
        for c in ["en", "ru", "de"] {
            if !to_recent.iter().any(|t| t == c) {
                to_recent.push(c.to_owned());
            }
        }
        Self {
            zoom_when_done: false,
            tab: Tab::Text,
            view: View::Translator,
            from: "auto".into(),
            to: target.to_owned(),
            from_recent: vec!["en".into(), "ru".into()],
            to_recent,
            prefer: None,
            input: String::new(),
            edited_at: None,
            text_gen: 0,
            result: None,
            loading: false,
            result_at: None,
            recorded: true,
            image: ImageSlot::Empty,
            image_gen: 0,
            history: HistoryState::default(),
            env: Env::default(),
            menu: None,
            query: String::new(),
            copied_at: None,
            wiki_open: false,
            paste_down: false,
            frames: 0,
        }
    }

    /// Card height for the current content.
    pub fn card_h(&self) -> f32 {
        let doc = matches!(self.image, ImageSlot::Doc(_));
        if self.view == View::Translator && self.tab == Tab::Image && doc { CARD_H_RESULT } else { CARD_H }
    }

    /// The text result, when there is one.
    pub fn translation(&self) -> Option<&Arc<Translation>> {
        match &self.result {
            Some(Ok(t)) => Some(t),
            _ => None,
        }
    }

    /// Shows a history entry in the Text tab (no new request).
    pub fn reopen(&mut self, e: &Entry) {
        self.view = View::Translator;
        self.tab = Tab::Text;
        self.input = e.source.clone();
        if e.from != "auto" {
            self.from = e.from.clone();
            remember(&mut self.from_recent, &e.from, 2);
        }
        self.to = e.to.clone();
        remember(&mut self.to_recent, &e.to, 3);
        self.text_gen += 1;
        self.edited_at = None;
        self.loading = false;
        self.result = Some(Ok(Arc::new(Translation {
            source: e.source.clone(),
            text: e.translation.clone(),
            src_lang: (e.from != "auto").then(|| e.from.clone()),
            tgt_lang: e.to.clone(),
            is_word: false,
            dict: Vec::new(),
            provider: e.provider,
        })));
        self.result_at = Some(Instant::now());
        self.recorded = true;
    }

    /// Swaps source and target; the translation becomes the new input.
    pub fn swap(&mut self) {
        let detected = self.translation().and_then(|t| t.src_lang.clone());
        let new_to = if self.from == "auto" {
            match detected {
                Some(d) => d,
                None => return,
            }
        } else {
            self.from.clone()
        };
        let new_from = std::mem::replace(&mut self.to, new_to);
        self.from = new_from;
        remember(&mut self.from_recent, &self.from, 2);
        remember(&mut self.to_recent, &self.to, 3);
        if self.tab == Tab::Text
            && let Some(t) = self.translation()
        {
            self.input = t.text.clone();
        }
    }
}

/// Keeps `code` among the first `shown` entries of a recent list.
fn remember(list: &mut Vec<String>, code: &str, shown: usize) {
    if code == "auto" || list.iter().take(shown).any(|c| c == code) {
        return;
    }
    list.retain(|c| c != code);
    list.insert(0, code.to_owned());
    list.truncate(6);
}

/// What the window asks the app to do.
pub enum MainAction {
    Close,
    Minimize,
    OpenSettings,
    /// Translate the input now (languages or provider changed).
    TranslateNow,
    /// Record the current text result in history now.
    Record,
    Speak(String, String),
    Copy(String),
    ChooseFile,
    ScreenArea,
    /// Capture the monitor under the cursor at once.
    WholeScreen,
    PasteImage,
    LoadFile(std::path::PathBuf),
    /// Languages or provider changed while a picture is shown.
    RetranslateImage,
    CopyPicture(RenderJob),
    SavePicture(RenderJob),
    SaveText(String),
}

pub fn show(ui: &mut Ui, st: &mut MainState) -> Vec<MainAction> {
    let mut out = Vec::new();
    let ctx = ui.ctx().clone();
    st.frames += 1;

    // Files dropped anywhere on the window open in the Images tab.
    let dropped: Vec<_> = ctx.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).filter(|p| !p.as_os_str().is_empty()).collect());
    if let Some(path) = dropped.into_iter().next() {
        st.view = View::Translator;
        st.tab = Tab::Image;
        out.push(MainAction::LoadFile(path));
    }
    // Ctrl+V of an image (egui only reports text pastes).
    let paste = ctx.input(|i| i.focused) && crate::win::image::ctrl_v_down();
    if paste && !st.paste_down && st.tab == Tab::Image && st.view == View::Translator && st.menu.is_none() {
        out.push(MainAction::PasteImage);
    }
    st.paste_down = paste;

    let card = Rect::from_min_size(pos2(MARGIN_X, MARGIN_TOP), vec2(CARD_W, st.card_h()));
    let (mut gear, mut clock) = (false, false);
    let title = window_frame(
        ui,
        card,
        crate::APP_NAME,
        |ui| {
            gear = title_button(ui, icons::GEAR, tr("Settings")).clicked();
            clock = title_button(ui, icons::CLOCK, tr("History")).clicked();
        },
        |ui| {
            let content = ui.max_rect().shrink2(vec2(20.0, 16.0));
            match st.view {
                View::Translator => translator(ui, content, st, &mut out),
                View::History => {
                    if let Some(a) = history_view::show(ui, content, &mut st.history) {
                        match a {
                            history_view::HistoryAction::Back => st.view = View::Translator,
                            history_view::HistoryAction::Open(e) => st.reopen(&e),
                            history_view::HistoryAction::Copy(t) => out.push(MainAction::Copy(t)),
                        }
                    }
                }
            }
        },
    );
    if gear {
        out.push(MainAction::OpenSettings);
    }
    if clock {
        st.view = if st.view == View::History { View::Translator } else { View::History };
        st.menu = None;
    }
    match title {
        Some(TitleAction::Close) => out.push(MainAction::Close),
        Some(TitleAction::Minimize) => out.push(MainAction::Minimize),
        None => {}
    }

    if st.view == View::Translator {
        menus(&ctx, st, &mut out);
    }
    if ctx.input(|i| i.key_pressed(Key::Escape)) {
        if st.menu.is_some() {
            st.menu = None;
        } else if st.wiki_open {
            st.wiki_open = false;
        } else if st.view == View::History {
            st.view = View::Translator;
        }
    }
    if ui.input(|i| i.viewport().close_requested()) {
        out.push(MainAction::Close);
    }
    out
}

/// A child UI over `rect` with the given layout.
fn child(ui: &mut Ui, rect: Rect, layout: Layout) -> Ui {
    ui.new_child(UiBuilder::new().max_rect(rect).layout(layout))
}

fn translator(ui: &mut Ui, content: Rect, st: &mut MainState, out: &mut Vec<MainAction>) {
    let mut y = content.top();
    // Mode pills.
    let mut row = child(ui, Rect::from_min_size(pos2(content.left(), y), vec2(content.width(), PILL_H)), Layout::left_to_right(Align::Center));
    row.spacing_mut().item_spacing.x = 8.0;
    let text_on = st.tab == Tab::Text;
    if mode_pill(&mut row, if text_on { icons::MODE_TEXT_ON } else { icons::MODE_TEXT_OFF }, tr("Text"), text_on).clicked() {
        st.tab = Tab::Text;
        st.menu = None;
    }
    if mode_pill(&mut row, if text_on { icons::MODE_IMAGE_OFF } else { icons::MODE_IMAGE_ON }, tr("Images"), !text_on).clicked() {
        st.tab = Tab::Image;
        st.menu = None;
    }
    y += PILL_H + 12.0;

    // Language strips and the swap button.
    let half = (content.width() - 16.0) / 2.0;
    let strips = Rect::from_min_size(pos2(content.left(), y), vec2(content.width(), STRIP_H));
    language_strips(ui, strips, half, st, out);
    y += STRIP_H + 12.0;

    let doc = matches!(st.image, ImageSlot::Doc(_));
    let footer_h = match st.tab {
        Tab::Text => 15.0,
        Tab::Image if doc => 0.0,
        Tab::Image => 30.0,
    };
    let bottom = if footer_h > 0.0 { content.bottom() - footer_h - 12.0 } else { content.bottom() };
    let left = Rect::from_min_max(pos2(content.left(), y), pos2(content.left() + half, bottom));
    let right = Rect::from_min_max(pos2(content.right() - half, y), pos2(content.right(), bottom));
    let footer = Rect::from_min_max(pos2(content.left(), content.bottom() - footer_h), content.max);
    match st.tab {
        Tab::Text => {
            source_panel(ui, left, st, out);
            result_panel(ui, right, st, out);
            let mut f = child(ui, footer, Layout::left_to_right(Align::Center));
            let hint = trf("{s} — translate selected text in any app", &[("s", &st.env.shortcut)]);
            text(&mut f, hint, 12.0, Weight::Regular, TEXT_3);
        }
        Tab::Image => {
            image_tab::panels(ui, left, right, st, out);
            if footer_h > 0.0 {
                let mut f = child(ui, footer, Layout::left_to_right(Align::Center));
                image_tab::footer(&mut f);
            }
        }
    }
}

/// `Режим / …` — 32px pill (radius 16): icon 16, label 14 Medium; active = filled.
fn mode_pill(ui: &mut Ui, src: egui::ImageSource<'static>, label: &str, active: bool) -> egui::Response {
    let color = if active { ACCENT } else { TEXT_2 };
    let galley = ui.painter().layout_no_wrap(label.to_owned(), font(14.0, Weight::Medium), color);
    let size = vec2(12.0 + 16.0 + 8.0 + galley.size().x + 16.0, PILL_H);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let p = ui.painter();
    if active {
        p.rect_filled(rect, 16, SELECTED);
    } else {
        if resp.hovered() {
            p.rect_filled(rect, 16, HOVER);
        }
        p.rect_stroke(rect, 16, BORDER_STROKE, egui::StrokeKind::Inside);
    }
    let y = rect.center().y;
    Image::new(src).paint_at(ui, Rect::from_center_size(pos2(rect.left() + 12.0 + 8.0, y), vec2(16.0, 16.0)));
    p.galley(pos2(rect.left() + 36.0, y - galley.size().y / 2.0), galley, color);
    resp.on_hover_cursor(CursorIcon::PointingHand)
}

fn language_strips(ui: &mut Ui, strips: Rect, half: f32, st: &mut MainState, out: &mut Vec<MainAction>) {
    let open = st.menu.map(|(m, _)| m);
    let mut changed = false;

    // Source: "Detect language" + two recent languages + chevron.
    let mut items = vec![("auto".to_owned(), tr("Detect language").to_owned())];
    items.extend(st.from_recent.iter().take(2).map(|c| (c.clone(), lang_name(c))));
    let left = Rect::from_min_size(strips.min, vec2(half, STRIP_H));
    let (pick, chev) = strip(ui, left, &items, &st.from, open == Some(Menu::From), "from");
    if let Some(code) = pick {
        changed |= code != st.from;
        st.from = code;
    }
    if chev.clicked() {
        st.menu = if open == Some(Menu::From) { None } else { Some((Menu::From, chev.rect)) };
        st.query.clear();
    }

    // Target: three recent languages + chevron, just right of the swap
    // button (where the Figma frame renders them).
    let items: Vec<_> = st.to_recent.iter().take(3).map(|c| (c.clone(), lang_name(c))).collect();
    let right = Rect::from_min_max(pos2(strips.left() + 371.0 + 36.0 + 3.0, strips.top()), strips.max);
    let (pick, chev) = strip(ui, right, &items, &st.to, open == Some(Menu::To), "to");
    if let Some(code) = pick {
        changed |= code != st.to;
        st.to = code;
    }
    if chev.clicked() {
        st.menu = if open == Some(Menu::To) { None } else { Some((Menu::To, chev.rect)) };
        st.query.clear();
    }

    // Swap: 36px circle at x 371 of the content row, 1px above it.
    let swap = Rect::from_min_size(pos2(strips.left() + 371.0, strips.top() - 1.0), vec2(36.0, 36.0));
    let resp = ui.interact(swap, ui.id().with("swap"), Sense::click());
    let p = ui.painter();
    p.rect_filled(swap, 18, if resp.hovered() { HOVER } else { PANEL });
    p.rect_stroke(swap, 18, BORDER_STROKE, egui::StrokeKind::Inside);
    Image::new(icons::SWAP).paint_at(ui, Rect::from_center_size(swap.center(), vec2(16.0, 16.0)));
    if resp.on_hover_cursor(CursorIcon::PointingHand).on_hover_text(tr("Swap source and target")).clicked() {
        st.swap();
        changed = true;
    }
    if changed {
        on_languages_changed(st, out);
    }
}

fn on_languages_changed(st: &mut MainState, out: &mut Vec<MainAction>) {
    match st.tab {
        Tab::Text => out.push(MainAction::TranslateNow),
        Tab::Image => {
            if matches!(st.image, ImageSlot::Doc(_)) {
                out.push(MainAction::RetranslateImage);
            }
        }
    }
}

/// One language strip: tabs `px 9`, 34px, 13 Medium, gap 2, selected = accent with a
/// 2px underline; then the 30px chevron. Returns the clicked code and the
/// chevron's response.
fn strip(
    ui: &mut Ui,
    rect: Rect,
    items: &[(String, String)],
    selected: &str,
    open: bool,
    salt: &str,
) -> (Option<String>, egui::Response) {
    let mut row = child(ui, rect, Layout::left_to_right(Align::Center));
    row.set_clip_rect(rect.intersect(ui.clip_rect()));
    row.spacing_mut().item_spacing.x = 2.0;
    let mut picked = None;
    for (code, label) in items {
        let on = code == selected;
        let galley = row.painter().layout_no_wrap(label.clone(), font(13.0, Weight::Medium), TEXT_2);
        let (r, resp) = row.allocate_exact_size(vec2(galley.size().x + 18.0, STRIP_H), Sense::click());
        let color = if on {
            ACCENT
        } else if resp.hovered() {
            TEXT
        } else {
            TEXT_2
        };
        let p = row.painter();
        p.galley_with_override_text_color(pos2(r.left() + 9.0, r.center().y - galley.size().y / 2.0), galley, color);
        if on {
            p.rect_filled(Rect::from_min_max(pos2(r.left(), r.bottom() - 2.0), r.max), 0, ACCENT);
        }
        if resp.on_hover_cursor(CursorIcon::PointingHand).clicked() {
            picked = Some(code.clone());
        }
    }
    let (r, chev) = row.allocate_exact_size(vec2(30.0, STRIP_H), Sense::click());
    if chev.hovered() || open {
        row.painter().rect_filled(Rect::from_center_size(r.center(), vec2(26.0, 26.0)), 7, HOVER);
    }
    Image::new(icons::CHEV16).paint_at(&row, Rect::from_center_size(r.center(), vec2(16.0, 16.0)));
    let chev = chev.on_hover_cursor(CursorIcon::PointingHand).on_hover_text(tr("More languages"));
    let _ = salt;
    (picked, chev)
}

/// Text in the panels: 20 Regular, line height 1.4.
fn panel_job(text: &str, color: egui::Color32, wrap: f32) -> LayoutJob {
    let mut job = LayoutJob::single_section(
        text.to_owned(),
        TextFormat {
            font_id: font(TEXT_SIZE, Weight::Regular),
            color,
            line_height: Some(TEXT_SIZE * TEXT_LEADING),
            ..Default::default()
        },
    );
    job.wrap.max_width = wrap;
    job
}

fn panel_frame(ui: &Ui, rect: Rect, fill: egui::Color32) {
    ui.painter().rect(rect, 16, fill, BORDER_STROKE, egui::StrokeKind::Inside);
}

/// Left panel: the input, clear button, speaker, detected language, count.
fn source_panel(ui: &mut Ui, panel: Rect, st: &mut MainState, out: &mut Vec<MainAction>) {
    panel_frame(ui, panel, PANEL);
    let area = Rect::from_min_max(panel.min + vec2(20.0, 18.0 + TEXT_NUDGE), pos2(panel.right() - 12.0 - 32.0 - 8.0, panel.bottom() - PANEL_FOOT_H));
    let mut tui = child(ui, area, Layout::top_down(Align::Min));
    let before = st.input.clone();
    ScrollArea::vertical()
        .id_salt("qt-main-input")
        .max_height(area.height())
        .auto_shrink([false, false])
        .show(&mut tui, |ui| {
            let mut layouter = |ui: &Ui, buf: &dyn egui::TextBuffer, wrap: f32| {
                ui.painter().layout_job(panel_job(buf.as_str(), TEXT, wrap))
            };
            let edit = TextEdit::multiline(&mut st.input)
                .id(egui::Id::new("qt-main-input"))
                .frame(egui::Frame::NONE)
                .margin(egui::Margin::ZERO)
                .hint_text(egui::WidgetText::LayoutJob(Arc::new(panel_job(tr("Enter text"), TEXT_3, area.width()))))
                .char_limit(MAX_CHARS)
                .desired_width(area.width())
                .min_size(vec2(area.width(), area.height()))
                .layouter(&mut layouter);
            let resp = ui.add(edit);
            if st.frames <= 3 && st.view == View::Translator {
                resp.request_focus();
            }
        });
    if st.input != before {
        st.edited_at = Some(Instant::now());
    }

    // Clear ×.
    if !st.input.is_empty() {
        let r = Rect::from_min_size(pos2(panel.right() - 12.0 - 32.0, panel.top() + 16.0), vec2(32.0, 32.0));
        let mut b = child(ui, r, Layout::left_to_right(Align::Center));
        if square_button(&mut b, icons::X16, 32.0, 16.0, tr("Clear")).clicked() {
            st.input.clear();
            st.edited_at = Some(Instant::now());
        }
    }

    // Footer: speaker, "detected: English", count.
    let foot = Rect::from_min_max(pos2(panel.left() + 12.0, panel.bottom() - PANEL_FOOT_H), pos2(panel.right() - 16.0, panel.bottom()));
    let mut f = child(ui, foot, Layout::left_to_right(Align::Center));
    f.spacing_mut().item_spacing.x = 2.0;
    let detected = st.translation().and_then(|t| t.src_lang.clone());
    if square_button(&mut f, icons::VOL17, 36.0, 17.0, tr("Listen")).clicked() && !st.input.trim().is_empty() {
        let lang = if st.from == "auto" { detected.clone().unwrap_or_else(|| "en".into()) } else { st.from.clone() };
        out.push(MainAction::Speak(st.input.clone(), lang));
    }
    if st.from == "auto"
        && let Some(code) = detected
    {
        let mut name = lang_name(&code);
        if is_russian() {
            name = name.to_lowercase();
        }
        f.add_space(6.0);
        text(&mut f, trf("detected: {lang}", &[("lang", &name)]), 12.0, Weight::Regular, TEXT_3);
    }
    f.with_layout(Layout::right_to_left(Align::Center), |ui| {
        let count = format!("{} / {}", group_digits(st.input.chars().count()), group_digits(MAX_CHARS));
        text(ui, count, 12.0, Weight::Regular, TEXT_3);
    });
}

/// "5 000" in Russian, "5,000" in English.
pub fn group_digits(n: usize) -> String {
    group_with(n, if is_russian() { '\u{a0}' } else { ',' })
}

fn group_with(n: usize, sep: char) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(sep);
        }
        out.push(c);
    }
    out
}

/// Message and hint for a failed request.
pub fn error_text(e: &Error) -> (&str, &str) {
    match e {
        Error::Offline => (tr("Can’t reach the translation service"), tr("Check your connection and try again")),
        Error::Busy => (tr("The translation service is busy"), tr("Wait a moment and try again")),
        Error::Other(m) => (tr("Translation failed"), m.as_str()),
    }
}

/// Right panel: the translation, star, provider chip, speaker, copy.
fn result_panel(ui: &mut Ui, panel: Rect, st: &mut MainState, out: &mut Vec<MainAction>) {
    panel_frame(ui, panel, TRANSLATION_PANEL);
    let area = Rect::from_min_max(panel.min + vec2(20.0, 18.0 + TEXT_NUDGE), pos2(panel.right() - 12.0 - 32.0 - 8.0, panel.bottom() - PANEL_FOOT_H));
    let mut tui = child(ui, area, Layout::top_down(Align::Min));
    let empty = st.input.trim().is_empty();
    ScrollArea::vertical()
        .id_salt("qt-main-result")
        .max_height(area.height())
        .auto_shrink([false, false])
        .show(&mut tui, |ui| match &st.result {
            Some(Ok(t)) if !empty => {
                let color = if st.loading { TEXT_2 } else { TEXT };
                ui.add(egui::Label::new(panel_job(&t.text, color, area.width())).selectable(true));
            }
            Some(Err(e)) if !empty && !st.loading => {
                let (msg, hint) = error_text(e);
                text(ui, msg, 15.0, Weight::Medium, WARN);
                ui.add_space(4.0);
                ui.add(egui::Label::new(rt(hint, 13.0, Weight::Regular, TEXT_3)).wrap());
                ui.add_space(8.0);
                if text_button(ui, tr("Retry"), 13.0, Weight::Medium, ACCENT, ACCENT_HOVER).clicked() {
                    out.push(MainAction::TranslateNow);
                }
            }
            _ if !empty => {
                ui.add(egui::Label::new(panel_job(tr("Translating…"), TEXT_3, area.width())));
            }
            _ => {
                ui.add(egui::Label::new(panel_job(tr("Translation"), TEXT_3, area.width())));
            }
        });

    let current = st.translation().filter(|_| !empty).cloned();
    if let Some(t) = &current {
        let starred = crate::history::is_starred(&t.source, &t.tgt_lang);
        let r = Rect::from_min_size(pos2(panel.right() - 12.0 - 32.0, panel.top() + 16.0), vec2(32.0, 32.0));
        let mut b = child(ui, r, Layout::left_to_right(Align::Center));
        let (icon, tip) = if starred { (icons::STAR_ON, tr("Remove from favourites")) } else { (icons::STAR, tr("Add to favourites")) };
        if square_button(&mut b, icon, 32.0, 16.0, tip).clicked() {
            let from = t.src_lang.clone().unwrap_or_else(|| st.from.clone());
            crate::history::star_translation(&t.source, &t.text, &from, &t.tgt_lang, t.provider, crate::history::Mode::Main, !starred);
            st.recorded = true;
        }
    }

    let foot = Rect::from_min_max(pos2(panel.left() + 16.0, panel.bottom() - PANEL_FOOT_H), pos2(panel.right() - 12.0, panel.bottom()));
    let mut f = child(ui, foot, Layout::left_to_right(Align::Center));
    f.spacing_mut().item_spacing.x = 8.0;
    provider_chip(&mut f, st, current.as_ref().map(|t| t.provider));
    if st.loading {
        f.add(egui::Spinner::new().size(14.0).color(TEXT_3));
    }
    // Right to left: Google Translate (the Figma "share" slot), copy,
    // speaker, Wikipedia.
    let mut wiki_button = None;
    f.with_layout(Layout::right_to_left(Align::Center), |ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        if empty {
            st.wiki_open = false;
            return;
        }
        // Also when the service is busy: the web page usually still works.
        if square_button(ui, icons::EXTERNAL17, 36.0, 17.0, tr("Open in Google Translate")).clicked() {
            crate::web::open_google_translate(&st.input, &st.from, &st.to);
        }
        if let Some(t) = &current {
            let copied = st.copied_at.is_some_and(|c| c.elapsed().as_secs_f32() < 1.5);
            let tip = if copied { tr("Copied") } else { tr("Copy") };
            if square_button(ui, icons::COPY17, 36.0, 17.0, tip).clicked() {
                st.copied_at = Some(Instant::now());
                out.push(MainAction::Copy(t.text.clone()));
                out.push(MainAction::Record);
            }
            if square_button(ui, icons::VOL17, 36.0, 17.0, tr("Listen")).clicked() {
                out.push(MainAction::Speak(t.text.clone(), t.tgt_lang.clone()));
                out.push(MainAction::Record);
            }
        }
        if st.env.wiki {
            let r = toggle_square_button(ui, icons::WIKI17, 36.0, 17.0, tr("Wikipedia article"), st.wiki_open);
            if r.clicked() {
                st.wiki_open = !st.wiki_open;
            }
            wiki_button = Some(r.rect);
        } else {
            st.wiki_open = false;
        }
    });

    // The Wikipedia card, over the panel above the W button.
    if let (true, Some(button)) = (st.wiki_open, wiki_button) {
        let detected = current.as_ref().and_then(|t| t.src_lang.clone());
        let from = detected.as_deref().or((st.from != "auto").then_some(st.from.as_str()));
        let q = crate::wiki::Query::new(&st.input, from, current.as_ref().map(|t| t.text.as_str()), &st.to);
        let ctx = ui.ctx().clone();
        let viewport = ctx.viewport_id();
        let pos = pos2(button.right().min(panel.right() - 8.0), button.top() - crate::ui::wiki_card::GAP);
        let (_, close) = crate::ui::wiki_card::area(&ctx, "qt-main-wiki", pos, egui::Align2::RIGHT_BOTTOM, &q, WIKI_W, {
            let ctx = ctx.clone();
            move || ctx.request_repaint_of(viewport)
        });
        if close {
            st.wiki_open = false;
        }
    }
}

/// `Чип / Google` — shows the provider of the current result; opens the
/// provider menu.
pub(crate) fn provider_chip(ui: &mut Ui, st: &mut MainState, shown: Option<ProviderKind>) {
    let label = shown
        .or(st.prefer)
        .or_else(|| st.env.providers.first().copied())
        .map_or("Google", |k| k.short_name());
    let open = st.menu.is_some_and(|(m, _)| m == Menu::Provider);
    let r = chip(ui, label, open).on_hover_text(tr("Translation service"));
    if r.clicked() {
        st.menu = if open { None } else { Some((Menu::Provider, r.rect)) };
    }
}

/// The open dropdown: language lists with search, or the provider list.
fn menus(ctx: &egui::Context, st: &mut MainState, out: &mut Vec<MainAction>) {
    let Some((menu, anchor)) = st.menu else { return };
    let mut codes: Vec<String> = Vec::new();
    let items: Vec<(String, bool)>;
    let search = match menu {
        Menu::From => {
            let mut list = Vec::new();
            if crate::settings::lang_matches("auto", &["Detect language", tr("Detect language")], &st.query) {
                list.push(("auto".to_owned(), tr("Detect language").to_owned()));
            }
            list.extend(search_languages(&st.query).into_iter().map(|(c, n)| (c.to_owned(), n.to_owned())));
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
            let mut list = vec![(tr("Any provider").to_owned(), st.prefer.is_none())];
            list.extend(st.env.providers.iter().map(|k| (k.short_name().to_owned(), st.prefer == Some(*k))));
            items = list;
            None
        }
    };
    let width = if menu == Menu::Provider { 170.0 } else { 220.0 };
    let (picked, rect) = floating_list(ctx, "qt-main-menu", anchor, width, &items, search);
    if let Some(i) = picked {
        match menu {
            Menu::From => {
                st.from = codes[i].clone();
                remember(&mut st.from_recent, &st.from, 2);
            }
            Menu::To => {
                st.to = codes[i].clone();
                remember(&mut st.to_recent, &st.to, 3);
            }
            Menu::Provider => st.prefer = if i == 0 { None } else { st.env.providers.get(i - 1).copied() },
        }
        st.menu = None;
        on_languages_changed(st, out);
    } else {
        let outside = ctx.input(|i| {
            i.pointer.any_pressed() && i.pointer.interact_pos().is_some_and(|p| !rect.contains(p) && !anchor.contains(p))
        });
        if outside {
            st.menu = None;
        }
    }
}

/// Thin line (used between history rows).
pub(crate) fn hline(ui: &Ui, x: egui::Rangef, y: f32) {
    ui.painter().hline(x, y, Stroke::new(1.0, BORDER));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digits_are_grouped() {
        assert_eq!(group_with(5000, ','), "5,000");
        assert_eq!(group_with(32, ','), "32");
        assert_eq!(group_with(123456, ' '), "123 456");
    }

    #[test]
    fn recent_languages_keep_the_pick_visible() {
        let mut v: Vec<String> = ["en", "ru", "de"].map(String::from).to_vec();
        remember(&mut v, "ru", 2);
        assert_eq!(v, ["en", "ru", "de"]);
        remember(&mut v, "de", 2);
        assert_eq!(v, ["de", "en", "ru"]);
        remember(&mut v, "auto", 2);
        assert_eq!(v.len(), 3);
    }
}
