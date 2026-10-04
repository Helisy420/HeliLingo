//! `Settings` window (Figma "Настройки — …" frames 329:2 … 329:838): a
//! compact 680×440 card with the HeliLingo title bar, a sidebar of seven
//! sections and a scrolling page.
//!
//! The view edits a copy of [`Settings`] in place and reports what happened
//! as a [`SettingsAction`]; the app saves, applies and runs the side effects
//! (autostart, key checks, shortcut recording).

mod general;
mod hotkeys;
mod offline;
mod pages;
mod providers;
mod statistics;

use std::collections::HashMap;
use std::time::Instant;

use egui::{
    Align, Area, Color32, Frame, Id, Image, Layout, Margin, Order, Rect, ScrollArea, Sense, Stroke,
    Ui, UiBuilder, pos2, vec2,
};

use super::widgets::*;
use crate::i18n::{tr, trf};
use crate::icons;
use crate::settings::{
    AlreadyInTarget, HIDE_AFTER_OPTIONS, HotkeySlot, PopupCtrlC, ProviderEntry, ProviderKind,
    Settings, Shortcut, hide_after_label, lang_matches, lang_name, search_languages,
};
use crate::theme::*;

/// The window card: default and smallest size. The Figma frame is 680×440;
/// the owner found that too cramped, so rows and type are a size larger
/// and the window can be resized (the size is remembered).
pub const CARD_W: f32 = 760.0;
pub const CARD_H: f32 = 540.0;
/// Transparent margin around the card for its shadow (`0 14 36`).
pub const MARGIN_X: f32 = 40.0;
pub const MARGIN_TOP: f32 = 24.0;
pub const MARGIN_BOTTOM: f32 = 56.0;
pub const WINDOW: [f32; 2] = [CARD_W + 2.0 * MARGIN_X, CARD_H + MARGIN_TOP + MARGIN_BOTTOM];

/// The window size for a card size (the shadow margin around it).
pub fn window_for(card: [f32; 2]) -> [f32; 2] {
    [card[0].max(CARD_W) + 2.0 * MARGIN_X, card[1].max(CARD_H) + MARGIN_TOP + MARGIN_BOTTOM]
}

/// `Разделы`: 180 wide, `px 8 py 10`, 28px items 1px apart.
const SIDEBAR_W: f32 = 204.0;
const SIDEBAR_ITEM_H: f32 = 34.0;
/// `Содержимое`: `px 22 py 16`, 14px between blocks.
const CONTENT_PX: f32 = 22.0;
const CONTENT_PY: f32 = 16.0;
/// `Строка / …`: 42px tall (`py 8` with a description), `px 14`.
const ROW_H: f32 = 42.0;
const ROW_PX: f32 = 14.0;

/// Sections of the sidebar.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Page {
    #[default]
    General,
    Providers,
    Offline,
    Modes,
    Hotkeys,
    Stats,
    About,
}

impl Page {
    const ALL: [Page; 7] = [
        Page::General,
        Page::Providers,
        Page::Offline,
        Page::Modes,
        Page::Hotkeys,
        Page::Stats,
        Page::About,
    ];

    fn label(self) -> &'static str {
        match self {
            Page::General => tr("General"),
            Page::Providers => tr("Providers"),
            Page::Offline => tr("Offline & acceleration"),
            Page::Modes => tr("Context & modes"),
            Page::Hotkeys => tr("Hotkeys"),
            Page::Stats => tr("Statistics"),
            Page::About => tr("About"),
        }
    }

    /// Sidebar icon: normal and selected (accent).
    fn icons(self) -> (egui::ImageSource<'static>, egui::ImageSource<'static>) {
        match self {
            Page::General => (icons::SLIDERS, icons::SLIDERS_ON),
            Page::Providers => (icons::GLOBE_DIM, icons::GLOBE),
            Page::Offline => (icons::CPU, icons::CPU_ON),
            Page::Modes => (icons::TEXT_LINES, icons::TEXT_LINES_ON),
            Page::Hotkeys => (icons::KEYBOARD, icons::KEYBOARD_ON),
            Page::Stats => (icons::CHART, icons::CHART_ON),
            Page::About => (icons::INFO, icons::INFO_ON),
        }
    }

    /// The page a `--preview settings-…` flag opens.
    pub fn from_preview(what: &str) -> Option<Page> {
        Some(match what {
            "settings" | "settings-general" => Page::General,
            "settings-providers" => Page::Providers,
            "settings-offline" => Page::Offline,
            "settings-modes" => Page::Modes,
            "settings-hotkeys" => Page::Hotkeys,
            "settings-stats" => Page::Stats,
            "settings-about" => Page::About,
            _ => return None,
        })
    }
}

/// Result of Settings → Providers → Check.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CheckStatus {
    Running,
    Ok,
    Failed(String),
}

/// The dropdowns of the window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Menu {
    UiLang,
    Target,
    Source,
    AlreadyIn,
    HideAfter,
    CtrlC,
    Shortcut,
    AddApp,
}

#[derive(Default)]
pub struct SettingsState {
    pub page: Page,
    /// Waiting for the user to press a shortcut for this slot.
    pub capturing: Option<HotkeySlot>,
    /// Shown on the Hotkeys page after a recording, e.g. that the new
    /// binding took another shortcut's keys.
    pub notice: Option<String>,
    /// Key checks, filled in by the app.
    pub checks: HashMap<ProviderKind, CheckStatus>,
    /// The provider row that shows its key fields.
    pub expanded: Option<ProviderKind>,
    /// Statistics page: period, cached summary, sample data for previews.
    pub stats: statistics::StatsView,
    /// When the sidebar last switched pages (drives the fade-in).
    page_switched: Option<Instant>,
    /// Open dropdown and the rect of its trigger.
    open: Option<(Menu, Rect)>,
    /// The open list was already scrolled to the current choice.
    scrolled: bool,
    /// Search text of a language list, or the typed name in "Add app".
    query: String,
    /// Running apps offered by "Add app" (refreshed when it opens).
    apps: Vec<String>,
    /// Provider being dragged by its grip, and the live order.
    drag: Option<ProviderKind>,
    drag_order: Option<Vec<ProviderEntry>>,
    /// "Clear history" was clicked once; a second click within a few seconds clears.
    clear_armed: Option<Instant>,
}

pub enum SettingsAction {
    Close,
    Minimize,
    Changed,
    StartCapture(HotkeySlot),
    CancelCapture,
    /// A key the window saw while recording: (virtual-key code, held
    /// modifiers). Backup for when the input hook doesn't get it.
    CaptureKey(u32, crate::settings::Combo),
    Autostart(bool),
    Check(ProviderKind),
}

/// The title of a shortcut, as on the Hotkeys page.
pub fn slot_title(slot: HotkeySlot) -> &'static str {
    match slot {
        HotkeySlot::Translate => tr("Translate selection"),
        HotkeySlot::Quick => tr("Quick window"),
        HotkeySlot::Ultra => tr("Ultra mode"),
        HotkeySlot::ScreenArea => tr("Screen area"),
        HotkeySlot::WholeScreen => tr("Whole screen"),
        HotkeySlot::MainWindow => tr("Main window"),
        HotkeySlot::TranslatePaste => tr("Translate and paste"),
        HotkeySlot::CopyTwice => tr("Ctrl + C twice"),
    }
}

pub fn show(ui: &mut Ui, s: &mut Settings, state: &mut SettingsState) -> Option<SettingsAction> {
    let mut action = None;
    let ctx = ui.ctx().clone();
    let had_focus = ctx.memory(|m| m.focused().is_some());
    // The card fills the window minus the shadow margin (resizable).
    let screen = ctx.content_rect();
    let card_size = vec2(
        (screen.width() - 2.0 * MARGIN_X).max(CARD_W),
        (screen.height() - MARGIN_TOP - MARGIN_BOTTOM).max(CARD_H),
    );
    let card = Rect::from_min_size(pos2(MARGIN_X, MARGIN_TOP), card_size);
    // Remember the size once a resize is over.
    let size = [card_size.x.round(), card_size.y.round()];
    if s.settings_size.unwrap_or([CARD_W, CARD_H]) != size && !ctx.input(|i| i.pointer.any_down()) {
        s.settings_size = Some(size);
        action = Some(SettingsAction::Changed);
    }

    Area::new(Id::new("qt-settings"))
        .order(Order::Middle)
        .fixed_pos(pos2(0.0, 0.0))
        .constrain(false)
        .show(&ctx, |ui| {
            ui.allocate_rect(card, Sense::hover());
            let title = compact_window_frame(ui, card, tr("HeliLingo — settings"), |body| {
                let rect = body.max_rect();
                if let Some(a) = sidebar(body, Rect::from_min_size(rect.min, vec2(SIDEBAR_W, rect.height())), state) {
                    action = Some(a);
                }
                let content = Rect::from_min_max(pos2(rect.left() + SIDEBAR_W, rect.top()), rect.max);
                let mut page = body.new_child(UiBuilder::new().max_rect(content).layout(Layout::top_down(Align::Min)));
                page.set_clip_rect(content);
                // A short fade-in after a page switch (Animations on).
                if let Some(at) = state.page_switched {
                    let t = at.elapsed().as_secs_f32() / ANIMATION_TIME;
                    if !s.animations || t >= 1.0 {
                        state.page_switched = None;
                    } else {
                        page.multiply_opacity(0.25 + 0.75 * t);
                        page.ctx().request_repaint();
                    }
                }
                ScrollArea::vertical()
                    .id_salt(state.page)
                    .auto_shrink([false, false])
                    .show(&mut page, |ui| {
                        let (px, py) = (CONTENT_PX as i8, CONTENT_PY as i8);
                        Frame::new()
                            .inner_margin(Margin { left: px, right: px, top: py, bottom: py })
                            .show(ui, |ui| {
                                ui.set_width(content.width() - 2.0 * CONTENT_PX);
                                ui.spacing_mut().item_spacing = vec2(8.0, 14.0);
                                let a = match state.page {
                                    Page::General => general::show(ui, s, state),
                                    Page::Providers => providers::show(ui, s, state),
                                    Page::Offline => offline::show(ui, s, state),
                                    Page::Modes => pages::modes(ui, s),
                                    Page::Hotkeys => hotkeys::show(ui, s, state),
                                    Page::Stats => statistics::show(ui, s, state),
                                    Page::About => pages::about(ui),
                                };
                                if a.is_some() {
                                    action = a;
                                }
                            });
                    });
            });
            resize_handles(ui, card);
            match title {
                Some(TitleAction::Close) => action = Some(SettingsAction::Close),
                Some(TitleAction::Minimize) => action = Some(SettingsAction::Minimize),
                None => {}
            }
        });

    if let Some((menu, anchor)) = state.open
        && let Some(a) = dropdown(&ctx, menu, anchor, card, s, state)
    {
        action = Some(a);
    }

    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) && state.capturing.is_none() {
        if state.open.is_some() {
            state.open = None;
        } else if !had_focus {
            action = Some(SettingsAction::Close);
        }
    }
    action
}

/// `Разделы`: panel fill, right border, `px 8 py 10`, 28px items 1px apart
/// (`px 10`, 14px icon, gap 8, 12px label, radius 7). Leaving a page while
/// a shortcut is being recorded cancels the recording.
fn sidebar(ui: &mut Ui, rect: Rect, state: &mut SettingsState) -> Option<SettingsAction> {
    let mut action = None;
    let p = ui.painter();
    p.rect_filled(rect, egui::CornerRadius { nw: 0, ne: 0, sw: 12, se: 0 }, PANEL);
    p.vline(rect.right() - 0.5, rect.y_range(), Stroke::new(1.0, BORDER));
    let mut y = rect.top() + 10.0;
    for page in Page::ALL {
        let item = Rect::from_min_size(pos2(rect.left() + 8.0, y), vec2(rect.width() - 16.0, SIDEBAR_ITEM_H));
        y += SIDEBAR_ITEM_H + 1.0;
        let resp = ui.interact(item, Id::new(("qt-settings-page", page)), Sense::click());
        let selected = state.page == page;
        if selected {
            ui.painter().rect_filled(item, 7, SELECTED);
        } else if resp.hovered() {
            ui.painter().rect_filled(item, 7, HOVER);
        }
        let (normal, on) = page.icons();
        let icon_rect = Rect::from_min_size(pos2(item.left() + 10.0, item.center().y - 7.0), vec2(14.0, 14.0));
        Image::new(if selected { on } else { normal }).paint_at(ui, icon_rect);
        let (w, c) = if selected { (Weight::SemiBold, ACCENT) } else { (Weight::Medium, TEXT_2) };
        // Montserrat runs wide: step down half a point rather than cut a label.
        let room = item.right() - 6.0 - (icon_rect.right() + 8.0);
        let wide = ui.painter().layout_no_wrap(page.label().to_owned(), font(12.0, w), c).size().x > room;
        let size = if wide { 11.0 } else { 12.0 };
        let galley = truncated(ui, page.label(), font(size, w), c, room);
        ui.painter()
            .galley(pos2(icon_rect.right() + 8.0, item.center().y - galley.size().y / 2.0), galley, c);
        if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() && !selected {
            state.page = page;
            state.page_switched = Some(Instant::now());
            state.open = None;
            if state.capturing.is_some() {
                action = Some(SettingsAction::CancelCapture);
            }
        }
    }
    action
}

// ------------------------------------------------------------ page parts

/// Page heading: 18 SemiBold title, optional 12px muted description (gap 3).
fn page_title(ui: &mut Ui, title: &str, sub: Option<&str>) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 3.0;
        text(ui, title, 18.0, Weight::SemiBold, TEXT);
        if let Some(sub) = sub {
            ui.add(egui::Label::new(rt(sub, 12.0, Weight::Regular, TEXT_3)).wrap().selectable(false));
        }
    });
}

/// Invisible grips on the right and bottom edges of the card and its
/// corner: dragging them resizes the (borderless) window.
fn resize_handles(ui: &mut Ui, card: Rect) {
    use egui::{CursorIcon, ResizeDirection, ViewportCommand};
    let grips = [
        (Rect::from_min_max(pos2(card.right() - 4.0, card.top() + 40.0), pos2(card.right() + 4.0, card.bottom() - 12.0)), ResizeDirection::East, CursorIcon::ResizeHorizontal),
        (Rect::from_min_max(pos2(card.left() + 12.0, card.bottom() - 4.0), pos2(card.right() - 12.0, card.bottom() + 4.0)), ResizeDirection::South, CursorIcon::ResizeVertical),
        (Rect::from_center_size(card.right_bottom(), vec2(20.0, 20.0)), ResizeDirection::SouthEast, CursorIcon::ResizeNwSe),
    ];
    for (i, (rect, dir, cursor)) in grips.into_iter().enumerate() {
        let r = ui.interact(rect, ui.id().with(("resize", i)), Sense::drag()).on_hover_cursor(cursor);
        if r.drag_started() {
            ui.ctx().send_viewport_cmd(ViewportCommand::BeginResize(dir));
        }
    }
}

/// The 10px SemiBold caps label of a section (tracking 0.8).
fn section_label(ui: &mut Ui, label: &str) {
    let mut job = egui::text::LayoutJob::default();
    job.append(
        &label.to_uppercase(),
        0.0,
        egui::TextFormat {
            font_id: font(11.0, Weight::SemiBold),
            color: TEXT_3,
            extra_letter_spacing: 0.8,
            ..Default::default()
        },
    );
    let galley = ui.painter().layout_job(job);
    let (r, _) = ui.allocate_exact_size(galley.size(), Sense::hover());
    ui.painter().galley(r.min, galley, TEXT_3);
}

/// A titled group: section label, 5px gap, then the `Группа / …` card
/// (panel fill, border, radius 10) holding the rows.
fn section(ui: &mut Ui, label: &str, add: impl FnOnce(&mut Ui)) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 8.0;
        section_label(ui, label);
        group(ui, add);
    });
}

/// `Группа / …` on its own.
fn group(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    let w = ui.available_width();
    Frame::new()
        .fill(PANEL)
        .stroke(BORDER_STROKE)
        .corner_radius(10)
        .show(ui, |ui| {
            ui.set_width(w - 2.0);
            ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
            add(ui);
        });
}

/// `Строка / …`: title (12 Medium) and optional description (10.5 Regular,
/// muted, wraps, gap 1) on the left, `control` right-aligned and centred.
/// 30px tall, or `py 5` with a description; 1px bottom border unless `last`.
/// `reserve` is the width kept free for the control when wrapping.
fn row(ui: &mut Ui, title: &str, sub: Option<&str>, reserve: f32, last: bool, control: impl FnOnce(&mut Ui)) -> Rect {
    let w = ui.available_width();
    let text_w = (w - 2.0 * ROW_PX - 12.0 - reserve).max(60.0);
    let title_g = ui.painter().layout(title.to_owned(), font(13.0, Weight::Medium), TEXT, text_w);
    let sub_g = sub.map(|s| ui.painter().layout(s.to_owned(), font(11.5, Weight::Regular), TEXT_3, text_w));
    let text_h = title_g.size().y + sub_g.as_ref().map_or(0.0, |g| 2.0 + g.size().y);
    let h = (text_h + 16.0).max(ROW_H);
    let rect = Rect::from_min_size(ui.cursor().min, vec2(w, h));
    ui.allocate_rect(rect, Sense::hover());

    let mut y = rect.center().y - text_h / 2.0;
    let x = rect.left() + ROW_PX;
    let th = title_g.size().y;
    ui.painter().galley(pos2(x, y), title_g, TEXT);
    y += th + 2.0;
    if let Some(g) = sub_g {
        ui.painter().galley(pos2(x, y), g, TEXT_3);
    }
    let mut c = ui.new_child(
        UiBuilder::new()
            .max_rect(rect.shrink2(vec2(ROW_PX, 0.0)))
            .layout(Layout::right_to_left(Align::Center)),
    );
    c.spacing_mut().item_spacing = vec2(8.0, 0.0);
    control(&mut c);
    if !last {
        ui.painter().hline(rect.x_range(), rect.bottom() - 0.5, Stroke::new(1.0, BORDER));
    }
    rect
}

/// A switch (or other toggle) was clicked: report the edit.
fn changed(r: egui::Response, action: &mut Option<SettingsAction>) {
    if r.clicked() {
        *action = Some(SettingsAction::Changed);
    }
}

/// A muted footnote with the 12px info icon (`px 2`, gap 6).
fn footnote(ui: &mut Ui, note: &str) {
    footnote_with(ui, icons::INFO14, note);
}

/// [`footnote`] with another 12px icon (the lock on Providers).
fn footnote_with(ui: &mut Ui, src: egui::ImageSource<'static>, note: &str) {
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        ui.add_space(2.0);
        ui.vertical(|ui| {
            ui.add_space(1.0);
            icon(ui, src, vec2(12.0, 12.0));
        });
        ui.add(egui::Label::new(rt(note, 10.5, Weight::Regular, TEXT_3)).wrap().selectable(false));
    });
}

/// A select trigger bound to `menu`: toggles it on click and keeps the
/// dropdown anchored to the trigger while it scrolls.
/// `width` is the Figma width; it grows to fit a longer label, up to 240.
fn select(ui: &mut Ui, state: &mut SettingsState, menu: Menu, label: &str, width: f32) {
    let text_w = ui.painter().layout_no_wrap(label.to_owned(), font(11.5, Weight::Medium), TEXT).size().x;
    let width = width.max(text_w + 10.0 + 6.0 + 12.0 + 8.0 + 2.0).min(240.0);
    let open = state.open.is_some_and(|(m, _)| m == menu);
    let r = select_button(ui, label, width, open);
    toggle_menu(state, menu, &r, open);
}

fn toggle_menu(state: &mut SettingsState, menu: Menu, r: &egui::Response, open: bool) {
    if open {
        state.open = Some((menu, r.rect));
    }
    if r.clicked() {
        state.open = if open { None } else { Some((menu, r.rect)) };
        state.scrolled = false;
        state.query.clear();
    }
}

/// Language name as it reads after "in"/"на": "Russian" / "русском".
fn lang_in(code: &str) -> String {
    let name = lang_name(code);
    if !crate::i18n::is_russian() {
        return name;
    }
    let n = name.to_lowercase();
    if let Some(stem) = n.strip_suffix("ий") {
        format!("{stem}ом")
    } else if n.ends_with(|c: char| !"аеёиоуыэюяй".contains(c)) {
        format!("{n}е")
    } else {
        n
    }
}

/// Label of the "already in the target language" choice.
fn already_label(a: AlreadyInTarget, target: &str) -> &'static str {
    match a {
        AlreadyInTarget::TranslateBack if target.starts_with("en") => tr("Translate to Russian"),
        AlreadyInTarget::TranslateBack => tr("Translate to English"),
        AlreadyInTarget::Keep => tr("Leave as is"),
    }
}

/// Label of a "Ctrl+C in the popup" choice.
fn ctrl_c_label(c: PopupCtrlC) -> &'static str {
    match c {
        PopupCtrlC::CopyAndClose => tr("Copy and close"),
        PopupCtrlC::CopyOnly => tr("Copy only"),
        PopupCtrlC::Ignore => tr("Ignore"),
    }
}

// --------------------------------------------------------------- dropdown

/// What a dropdown row sets when clicked.
#[derive(Clone)]
enum Pick {
    UiLang(&'static str),
    Lang(&'static str),
    Already(AlreadyInTarget),
    Hide(u32),
    CtrlC(PopupCtrlC),
    Shortcut(Shortcut),
    Record,
    /// The current custom chord: nothing to change.
    Keep,
    App(String),
}

/// Height of a dropdown row in this window.
const MENU_ROW_H: f32 = 30.0;

/// The open dropdown (`Shortcut menu` style), right-aligned under its
/// trigger (above it when there's no room). Language lists get a search
/// box (Enter picks the first match); "Add app" gets a name field (Enter
/// adds what was typed).
fn dropdown(
    ctx: &egui::Context,
    menu: Menu,
    anchor: Rect,
    card: Rect,
    s: &mut Settings,
    state: &mut SettingsState,
) -> Option<SettingsAction> {
    let mut action = None;
    let mut picked: Option<Pick> = None;
    let q = state.query.clone();

    let items: Vec<(String, bool, Pick)> = match menu {
        Menu::UiLang => crate::i18n::UI_LANGUAGES
            .iter()
            .map(|&(c, n)| (n.to_owned(), c == s.ui_lang, Pick::UiLang(c)))
            .collect(),
        Menu::Target => search_languages(&q)
            .into_iter()
            .map(|(c, n)| (n.to_owned(), c == s.target, Pick::Lang(c)))
            .collect(),
        Menu::Source => lang_matches("auto", &["Auto-detect", tr("Auto-detect")], &q)
            .then(|| (lang_name("auto"), s.source == "auto", Pick::Lang("auto")))
            .into_iter()
            .chain(
                search_languages(&q)
                    .into_iter()
                    .map(|(c, n)| (n.to_owned(), c == s.source, Pick::Lang(c))),
            )
            .collect(),
        Menu::AlreadyIn => [AlreadyInTarget::TranslateBack, AlreadyInTarget::Keep]
            .into_iter()
            .map(|a| (already_label(a, &s.target).to_owned(), a == s.already_in_target, Pick::Already(a)))
            .collect(),
        Menu::HideAfter => HIDE_AFTER_OPTIONS
            .iter()
            .map(|&secs| (hide_after_label(secs), secs == s.hide_after_secs, Pick::Hide(secs)))
            .collect(),
        Menu::CtrlC => [PopupCtrlC::CopyAndClose, PopupCtrlC::CopyOnly, PopupCtrlC::Ignore]
            .into_iter()
            .map(|c| (ctrl_c_label(c).to_owned(), c == s.popup_ctrl_c, Pick::CtrlC(c)))
            .collect(),
        Menu::Shortcut => {
            let mut v = vec![
                (tr("Double-tap Ctrl").to_owned(), s.shortcut == Shortcut::DoubleCtrl, Pick::Shortcut(Shortcut::DoubleCtrl)),
                (tr("Double-tap Alt").to_owned(), s.shortcut == Shortcut::DoubleAlt, Pick::Shortcut(Shortcut::DoubleAlt)),
            ];
            if let Shortcut::Combo(_) = s.shortcut {
                v.push((s.shortcut.label(), true, Pick::Keep));
            }
            v.push((tr("Record shortcut…").to_owned(), false, Pick::Record));
            v
        }
        Menu::AddApp => state
            .apps
            .iter()
            .filter(|a| !s.is_ignored_app(a))
            .filter(|a| q.trim().is_empty() || a.to_lowercase().contains(&q.trim().to_lowercase()))
            .map(|a| (a.clone(), false, Pick::App(a.clone())))
            .collect(),
    };

    let searchable = matches!(menu, Menu::Target | Menu::Source);
    let typed = menu == Menu::AddApp;
    let has_field = searchable || typed;
    let width = match menu {
        Menu::AddApp => 240.0,
        _ => anchor.width().max(180.0),
    };
    // Searchable lists keep their full height while filtering, so the box
    // doesn't jump around under the cursor.
    let rows = if has_field { 7 } else { items.len().max(1) };
    let list_h = (rows as f32 * (MENU_ROW_H + 1.0) - 1.0).min(216.0);
    let field_h = if has_field { 38.0 } else { 0.0 };
    let menu_h = list_h + field_h + 10.0;
    let below = anchor.bottom() + 4.0;
    let top = if below + menu_h <= card.bottom() - 8.0 {
        below
    } else {
        (anchor.top() - 4.0 - menu_h).max(card.top() + COMPACT_TITLE_H + 4.0)
    };
    let left = (anchor.right() - width - 8.0).max(card.left() + 8.0);

    let resp = Area::new(Id::new("qt-settings-dropdown"))
        .order(Order::Foreground)
        .fixed_pos(pos2(left, top))
        .constrain(false)
        .show(ctx, |ui| {
            surface(ui, 8, Margin::same(4), |ui| {
                ui.set_width(width);
                if searchable {
                    let (_, enter) = search_field(ui, &mut state.query, tr("Search"), width);
                    if enter {
                        picked = items.first().map(|(_, _, p)| p.clone());
                    }
                    ui.add_space(6.0);
                } else if typed {
                    let r = text_field(ui, &mut state.query, tr("Type a name, e.g. game.exe"), width, false, 0.0);
                    if !r.has_focus() && !r.lost_focus() {
                        r.request_focus();
                    }
                    if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        let name = state.query.trim();
                        picked = if name.is_empty() {
                            items.first().map(|(_, _, p)| p.clone())
                        } else {
                            Some(Pick::App(name.to_owned()))
                        };
                    }
                    ui.add_space(6.0);
                }
                ScrollArea::vertical()
                    .max_height(list_h)
                    .min_scrolled_height(if has_field { list_h } else { 0.0 })
                    .auto_shrink([!has_field, !has_field])
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing = vec2(0.0, 1.0);
                        if items.is_empty() {
                            ui.add_space(8.0);
                            ui.horizontal(|ui| {
                                ui.add_space(10.0);
                                let empty = if typed { tr("No other apps are open") } else { tr("No languages found") };
                                text(ui, empty, 12.0, Weight::Regular, TEXT_3);
                            });
                        }
                        for (label, selected, pick) in &items {
                            let r = menu_item(ui, label, *selected, MENU_ROW_H);
                            if *selected && !state.scrolled {
                                r.scroll_to_me(Some(Align::Center));
                            }
                            if r.clicked() {
                                picked = Some(pick.clone());
                            }
                        }
                    });
            });
        })
        .response;

    let close = picked.is_some();
    if let Some(pick) = picked {
        action = Some(SettingsAction::Changed);
        match pick {
            Pick::UiLang(c) => s.ui_lang = c.to_owned(),
            Pick::Lang(c) if menu == Menu::Target => s.target = c.to_owned(),
            Pick::Lang(c) => s.source = c.to_owned(),
            Pick::Already(a) => s.already_in_target = a,
            Pick::Hide(secs) => s.hide_after_secs = secs,
            Pick::CtrlC(c) => s.popup_ctrl_c = c,
            Pick::Shortcut(sc) => s.shortcut = sc,
            Pick::Record => action = Some(SettingsAction::StartCapture(HotkeySlot::Translate)),
            Pick::Keep => action = None,
            Pick::App(name) => {
                let name = if name.to_lowercase().ends_with(".exe") { name } else { format!("{name}.exe") };
                if s.is_ignored_app(&name) {
                    action = None;
                } else {
                    s.ignored_apps.push(name);
                }
            }
        }
    }

    // Click anywhere else closes the menu.
    let clicked_outside = ctx.input(|i| {
        i.pointer.any_pressed()
            && i.pointer
                .interact_pos()
                .is_some_and(|p| !resp.rect.contains(p) && !anchor.contains(p))
    });
    state.scrolled = true;
    if close || clicked_outside {
        state.open = None;
        state.query.clear();
    }
    action
}

/// Muted grey, the inactive badge colours (`Резерв`).
const BADGE_MUTED: (Color32, Color32) = (TEXT_2, HOVER);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn russian_prepositional_language_names() {
        crate::i18n::set_lang("ru");
        assert_eq!(lang_in("ru"), "русском");
        assert_eq!(lang_in("en"), "английском");
        assert_eq!(lang_in("he"), "иврите");
        assert_eq!(lang_in("hi"), "хинди");
        crate::i18n::set_lang("en");
        assert_eq!(lang_in("ru"), "Russian");
        crate::i18n::set_lang("ru");
    }

    #[test]
    fn preview_pages() {
        assert_eq!(Page::from_preview("settings"), Some(Page::General));
        assert_eq!(Page::from_preview("settings-hotkeys"), Some(Page::Hotkeys));
        assert_eq!(Page::from_preview("settings-stats"), Some(Page::Stats));
        assert_eq!(Page::from_preview("word"), None);
    }
}
