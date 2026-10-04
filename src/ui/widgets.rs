//! Small building blocks shared by the popup, tray menu and settings.

use egui::{
    Align, Color32, CornerRadius, CursorIcon, Frame, Image, ImageSource, InnerResponse, Label,
    Layout, Margin, Rect, Response, Sense, Shape, Stroke, TextEdit, Ui, Vec2, vec2,
};

use crate::icons;
use crate::theme::{self, *};

/// A floating surface: card fill, 1px border, both drop shadows.
pub fn surface<R>(
    ui: &mut Ui,
    radius: u8,
    margin: Margin,
    add: impl FnOnce(&mut Ui) -> R,
) -> InnerResponse<R> {
    let contact = ui.painter().add(Shape::Noop);
    let frame = Frame::new()
        .fill(CARD)
        .stroke(BORDER_STROKE)
        .corner_radius(radius)
        .inner_margin(margin)
        .shadow(SHADOW_LG);
    let r = frame.show(ui, add);
    ui.painter()
        .set(contact, SHADOW_SM.as_shape(r.response.rect, CornerRadius::same(radius)));
    r
}

pub fn text(ui: &mut Ui, s: impl Into<String>, size: f32, w: Weight, color: Color32) -> Response {
    ui.add(Label::new(rt(s, size, w, color)).selectable(false).extend())
}

/// Text that is a button: pointer cursor and a hover colour.
pub fn text_button(
    ui: &mut Ui,
    s: impl Into<String>,
    size: f32,
    w: Weight,
    color: Color32,
    hover: Color32,
) -> Response {
    let galley = ui.painter().layout_no_wrap(s.into(), font(size, w), color);
    let (rect, resp) = ui.allocate_exact_size(galley.size(), Sense::click());
    let c = if resp.hovered() { hover } else { color };
    ui.painter().galley_with_override_text_color(rect.min, galley, c);
    resp.on_hover_cursor(CursorIcon::PointingHand)
}

/// An icon at its exported size.
pub fn icon(ui: &mut Ui, src: ImageSource<'static>, size: Vec2) -> Response {
    ui.add(Image::new(src).fit_to_exact_size(size))
}

/// A clickable icon with a soft hover plate.
pub fn icon_button(ui: &mut Ui, src: ImageSource<'static>, size: f32, tip: &str) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(size, size), Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(rect.expand(4.0), 5, HOVER);
    }
    Image::new(src).paint_at(ui, rect);
    let resp = resp.on_hover_cursor(CursorIcon::PointingHand);
    if tip.is_empty() { resp } else { resp.on_hover_text(tip) }
}

/// `Key / Esc` — a small outlined key cap (px 6, py 1, radius 4).
pub fn key_cap(ui: &mut Ui, label: &str) {
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_owned(), font(11.0, Weight::Medium), TEXT_3);
    let size = galley.size() + vec2(14.0, 4.0);
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let p = ui.painter();
    p.rect_stroke(rect, 4, BORDER_STROKE, egui::StrokeKind::Inside);
    p.galley(rect.min + vec2(7.0, 2.0), galley, TEXT_3);
}

/// Divider frame from the popups: 6px above, 1px line, 4px below.
pub fn divider(ui: &mut Ui) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 11.0), Sense::hover());
    let y = rect.top() + 6.5;
    ui.painter()
        .hline(rect.x_range(), y, Stroke::new(1.0, BORDER));
}

/// Menu separator: 3px above/below, inset 6px.
pub fn menu_separator(ui: &mut Ui) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 7.0), Sense::hover());
    ui.painter()
        .hline(rect.shrink2(vec2(6.0, 0.0)).x_range(), rect.top() + 3.5, Stroke::new(1.0, BORDER));
}

/// The `[Esc] close` hint at the left of every popup footer.
pub fn esc_hint(ui: &mut Ui) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        key_cap(ui, "Esc");
        text(ui, crate::i18n::tr("close"), 11.0, Weight::Regular, TEXT_3);
    });
}

/// Row with content on the left and right edges (the Figma "Spacer" rows).
pub fn split_row(
    ui: &mut Ui,
    height: f32,
    left: impl FnOnce(&mut Ui),
    right: impl FnOnce(&mut Ui),
) -> Response {
    let w = ui.available_width();
    ui.allocate_ui_with_layout(vec2(w, height), Layout::left_to_right(Align::Center), |ui| {
        ui.set_min_size(vec2(w, height));
        left(ui);
        ui.with_layout(Layout::right_to_left(Align::Center), right);
    })
    .response
}

/// One row of a dropdown or tray menu.
pub fn menu_item(ui: &mut Ui, label: &str, selected: bool, height: f32) -> Response {
    let w = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(vec2(w, height), Sense::click());
    let painter = ui.painter();
    if selected {
        painter.rect_filled(rect, 5, SELECTED);
    } else if resp.hovered() {
        painter.rect_filled(rect, 5, HOVER);
    }
    let (font, color) = if selected {
        (theme::font(13.0, Weight::SemiBold), ACCENT)
    } else {
        (theme::font(13.0, Weight::Regular), TEXT)
    };
    let galley = painter.layout_no_wrap(label.to_owned(), font, color);
    let pos = egui::pos2(rect.left() + 10.0, rect.center().y - galley.size().y / 2.0);
    painter.galley(pos, galley, color);
    if selected {
        let icon_rect = Rect::from_center_size(
            egui::pos2(rect.right() - 10.0 - 7.0, rect.center().y),
            vec2(14.0, 14.0),
        );
        Image::new(icons::CHECK).paint_at(ui, icon_rect);
    }
    resp.on_hover_cursor(CursorIcon::PointingHand)
}

/// Two-option segmented control (Compact | Full). Returns the clicked index.
/// Field fill, outline radius 8 with 2px padding; segments `px 12 py 5`, radius 6.
pub fn segmented(ui: &mut Ui, options: &[&str], active: usize) -> Option<usize> {
    segmented_enabled(ui, options, active, true)
}

/// [`segmented`]; when not `enabled` it only shows the value (no hover, no clicks).
pub fn segmented_enabled(ui: &mut Ui, options: &[&str], active: usize, enabled: bool) -> Option<usize> {
    let segments: Vec<_> = options
        .iter()
        .enumerate()
        .map(|(i, label)| {
            let (w, c) = if i == active {
                (Weight::SemiBold, ACCENT)
            } else {
                (Weight::Medium, TEXT_2)
            };
            (ui.painter().layout_no_wrap((*label).to_owned(), font(12.0, w), c), c)
        })
        .collect();
    let seg_h = segments.iter().map(|(g, _)| g.size().y).fold(0.0, f32::max) + 10.0;
    let total_w = segments.iter().map(|(g, _)| g.size().x + 24.0).sum::<f32>()
        + 2.0 * (segments.len() as f32 - 1.0)
        + 4.0;
    let (outer, base) = ui.allocate_exact_size(vec2(total_w, seg_h + 4.0), Sense::hover());
    ui.painter().rect_filled(outer, 8, FIELD);
    ui.painter()
        .rect_stroke(outer, 8, BORDER_STROKE, egui::StrokeKind::Inside);

    let mut clicked = None;
    let mut x = outer.left() + 2.0;
    for (i, (galley, color)) in segments.into_iter().enumerate() {
        let rect = Rect::from_min_size(egui::pos2(x, outer.top() + 2.0), vec2(galley.size().x + 24.0, seg_h));
        x = rect.right() + 2.0;
        let sense = if enabled { Sense::click() } else { Sense::hover() };
        let resp = ui.interact(rect, base.id.with(("segment", i)), sense);
        if i == active {
            ui.painter().rect_filled(rect, 6, SELECTED);
        } else if enabled && resp.hovered() {
            ui.painter().rect_filled(rect, 6, HOVER);
        }
        ui.painter().galley(rect.min + vec2(12.0, 5.0), galley, color);
        if enabled && resp.on_hover_cursor(CursorIcon::PointingHand).clicked() {
            clicked = Some(i);
        }
    }
    clicked
}

/// Single-line galley cut to `max_w` with an ellipsis.
pub fn truncated(ui: &Ui, s: &str, font_id: egui::FontId, color: Color32, max_w: f32) -> std::sync::Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::single_section(s.to_owned(), egui::TextFormat::simple(font_id, color));
    job.wrap = egui::text::TextWrapping::truncate_at_width(max_w.max(1.0));
    ui.painter().layout_job(job)
}

/// `Выбор / …` (compact settings): a select trigger with field fill, 26px
/// tall, fixed `width`, radius 6, `pl 10 · label 11.5 · spacer · chevron 12
/// · pr 8`. Accent border while open.
pub fn select_button(ui: &mut Ui, label: &str, width: f32, open: bool) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(width, 26.0), Sense::click());
    let border = if open {
        ACCENT
    } else if resp.hovered() {
        TEXT_3
    } else {
        BORDER
    };
    let galley = truncated(ui, label, font(11.5, Weight::Medium), TEXT, width - 10.0 - 6.0 - 12.0 - 8.0);
    let p = ui.painter();
    p.rect_filled(rect, 6, FIELD);
    p.rect_stroke(rect, 6, Stroke::new(1.0, border), egui::StrokeKind::Inside);
    p.galley(egui::pos2(rect.left() + 10.0, rect.center().y - galley.size().y / 2.0), galley, TEXT);
    let chev = Rect::from_min_size(egui::pos2(rect.right() - 8.0 - 12.0, rect.center().y - 6.0), vec2(12.0, 12.0));
    Image::new(icons::CHEV12).paint_at(ui, chev);
    resp.on_hover_cursor(CursorIcon::PointingHand)
}

/// Outlined button (`Кнопка / Проверить`, `/ Открыть`): radius 6, `px 10`,
/// 11.5 Medium text, `height` 26 or 30.
pub fn outline_button(ui: &mut Ui, label: &str, height: f32) -> Response {
    let galley = ui.painter().layout_no_wrap(label.to_owned(), font(11.5, Weight::Medium), TEXT);
    let (rect, resp) = ui.allocate_exact_size(vec2(galley.size().x + 20.0, height), Sense::click());
    let p = ui.painter();
    if resp.hovered() {
        p.rect_filled(rect, 6, HOVER);
    }
    p.rect_stroke(rect, 6, BORDER_STROKE, egui::StrokeKind::Inside);
    p.galley(rect.center() - galley.size() / 2.0, galley, TEXT);
    resp.on_hover_cursor(CursorIcon::PointingHand)
}

/// Accent text link with an optional 12px leading icon (`Кнопка / Где взять
/// ключ`, `/ В глоссарий`): 20px tall, gap 6, 11.5 Medium.
pub fn link_button(ui: &mut Ui, icon: Option<ImageSource<'static>>, label: &str) -> Response {
    let galley = ui.painter().layout_no_wrap(label.to_owned(), font(11.5, Weight::Medium), ACCENT);
    let icon_w = if icon.is_some() { 12.0 + 6.0 } else { 0.0 };
    let (rect, resp) = ui.allocate_exact_size(vec2(icon_w + galley.size().x, 20.0), Sense::click());
    let color = if resp.hovered() { ACCENT_HOVER } else { ACCENT };
    if let Some(src) = icon {
        let r = Rect::from_min_size(egui::pos2(rect.left(), rect.center().y - 6.0), vec2(12.0, 12.0));
        Image::new(src).paint_at(ui, r);
    }
    let pos = egui::pos2(rect.left() + icon_w, rect.center().y - galley.size().y / 2.0);
    ui.painter().galley_with_override_text_color(pos, galley, color);
    resp.on_hover_cursor(CursorIcon::PointingHand)
}

/// `Клавиша / Ctrl`: a filled key cap, radius 4, `px 6 py 1`, 11 Medium.
pub fn kbd(ui: &mut Ui, label: &str, color: Color32) -> Response {
    let galley = ui.painter().layout_no_wrap(label.to_owned(), font(11.0, Weight::Medium), color);
    let (rect, resp) = ui.allocate_exact_size(galley.size() + vec2(14.0, 4.0), Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, 4, PANEL);
    p.rect_stroke(rect, 4, BORDER_STROKE, egui::StrokeKind::Inside);
    p.galley(rect.min + vec2(7.0, 2.0), galley, color);
    resp
}

/// `Ползунок`: 14px tall, 4px track (radius 2), accent fill up to the
/// value, 11px accent knob with a 3px window-coloured ring. Returns the
/// response; `value` follows the pointer while dragging, in `step` increments.
pub fn slider(ui: &mut Ui, value: &mut u32, range: std::ops::RangeInclusive<u32>, step: u32, width: f32) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(width, 14.0), Sense::click_and_drag());
    let (lo, hi) = (*range.start() as f32, *range.end() as f32);
    let knob_r = 5.5;
    let span = rect.x_range().shrink(knob_r);
    if let Some(p) = resp.interact_pointer_pos()
        && (resp.dragged() || resp.clicked() || resp.drag_started())
    {
        let t = ((p.x - span.min) / span.span()).clamp(0.0, 1.0);
        let raw = lo + t * (hi - lo);
        let snapped = ((raw / step as f32).round() as u32 * step).clamp(lo as u32, hi as u32);
        *value = snapped;
    }
    let t = ((*value as f32 - lo) / (hi - lo)).clamp(0.0, 1.0);
    let x = span.min + t * span.span();
    let track = Rect::from_min_max(egui::pos2(rect.left(), rect.center().y - 2.0), egui::pos2(rect.right(), rect.center().y + 2.0));
    let p = ui.painter();
    p.rect_filled(track, 2, SWITCH_OFF_TRACK);
    p.rect_filled(Rect::from_min_max(track.min, egui::pos2(x, track.max.y)), 2, ACCENT);
    let knob = egui::pos2(x, rect.center().y);
    p.circle_filled(knob, knob_r + 1.5, WINDOW_BG);
    p.circle_filled(knob, knob_r - 1.5, if resp.hovered() || resp.dragged() { ACCENT_HOVER } else { ACCENT });
    resp.on_hover_cursor(CursorIcon::PointingHand)
}

/// API-key style input (compact settings): 30px tall, field fill, radius 6,
/// accent border while focused, optional 12px lock icon (`px 10`, gap 6),
/// 11.5px text. `password` masks the text.
pub fn key_field(ui: &mut Ui, value: &mut String, hint: &str, width: f32, password: bool, lock: bool) -> Response {
    let (rect, _) = ui.allocate_exact_size(vec2(width, 30.0), Sense::hover());
    ui.painter().rect_filled(rect, 6, FIELD);
    let inset = if lock { 12.0 + 6.0 } else { 0.0 };
    let inner = Rect::from_min_max(rect.min + vec2(10.0 + inset, 0.0), rect.max - vec2(10.0, 0.0));
    let edit = TextEdit::singleline(value)
        .frame(Frame::NONE)
        .margin(Margin::ZERO)
        .font(font(11.5, Weight::Regular))
        .text_color(TEXT)
        .hint_text(rt(hint, 11.5, Weight::Regular, TEXT_3))
        .password(password)
        .vertical_align(Align::Center)
        .desired_width(inner.width());
    let resp = ui.place(inner, edit);
    if lock {
        let r = Rect::from_min_size(egui::pos2(rect.left() + 10.0, rect.center().y - 6.0), vec2(12.0, 12.0));
        Image::new(icons::LOCK12).paint_at(ui, r);
    }
    let border = if resp.has_focus() {
        ACCENT
    } else if resp.hovered() {
        TEXT_3
    } else {
        BORDER
    };
    ui.painter()
        .rect_stroke(rect, 6, Stroke::new(1.0, border), egui::StrokeKind::Inside);
    resp
}

/// A word that can be picked: hover plate, "Selected word" highlight
/// (`#2e3b78`, radius 3, 2px side padding). The plate bleeds outside the
/// allocated rect so picked and plain words lay out identically.
pub fn word_chip(
    ui: &mut Ui,
    word: &str,
    size: f32,
    weight: Weight,
    color: Color32,
    row_height: f32,
    selected: bool,
) -> Response {
    word_chip_sense(ui, word, size, weight, color, row_height, selected, Sense::click())
}

/// [`word_chip`] that can also sense drags (drag-to-select in sentences).
#[allow(clippy::too_many_arguments)]
pub fn word_chip_sense(
    ui: &mut Ui,
    word: &str,
    size: f32,
    weight: Weight,
    color: Color32,
    row_height: f32,
    selected: bool,
    sense: Sense,
) -> Response {
    let galley = ui
        .painter()
        .layout_no_wrap(word.to_owned(), font(size, weight), color);
    let (rect, resp) = ui.allocate_exact_size(vec2(galley.size().x, row_height), sense);
    let plate = Rect::from_center_size(
        rect.center(),
        vec2(galley.size().x + 4.0, galley.size().y.min(row_height)),
    );
    if selected {
        ui.painter().rect_filled(plate, 3, HIGHLIGHT);
    } else if resp.hovered() {
        ui.painter().rect_filled(plate, 3, HOVER);
    }
    let pos = egui::pos2(rect.left(), rect.center().y - galley.size().y / 2.0);
    ui.painter().galley(pos, galley, color);
    resp.on_hover_cursor(CursorIcon::PointingHand)
}

/// Width of a space in the given font, for laying out word chips.
pub fn space_width(ui: &Ui, size: f32, weight: Weight) -> f32 {
    ui.painter()
        .layout_no_wrap(" ".to_owned(), font(size, weight), TEXT)
        .size()
        .x
}

/// Outlined single-line input, 32px tall, radius 7 (the settings key field).
/// The border turns accent while the field has keyboard focus. `inset`
/// leaves room on the left for a leading glyph.
pub fn text_field(
    ui: &mut Ui,
    value: &mut String,
    hint: &str,
    width: f32,
    password: bool,
    inset: f32,
) -> Response {
    let (rect, _) = ui.allocate_exact_size(vec2(width, 32.0), Sense::hover());
    let inner = Rect::from_min_max(rect.min + vec2(12.0 + inset, 0.0), rect.max - vec2(10.0, 0.0));
    let edit = TextEdit::singleline(value)
        .frame(Frame::NONE)
        .margin(Margin::ZERO)
        .font(font(13.0, Weight::Regular))
        .text_color(TEXT)
        .hint_text(rt(hint, 13.0, Weight::Regular, TEXT_3))
        .password(password)
        .vertical_align(Align::Center)
        .desired_width(inner.width());
    let resp = ui.put(inner, edit);
    let border = if resp.has_focus() {
        ACCENT
    } else if resp.hovered() {
        TEXT_3
    } else {
        BORDER
    };
    ui.painter()
        .rect_stroke(rect, 7, Stroke::new(1.0, border), egui::StrokeKind::Inside);
    resp
}

/// Search box at the top of a long list. Keeps the keyboard focus while
/// the list is open, so the user can type straight away. Returns the
/// field's response; `submitted` is true when Enter was pressed.
pub fn search_field(ui: &mut Ui, query: &mut String, hint: &str, width: f32) -> (Response, bool) {
    let resp = text_field(ui, query, hint, width, false, 20.0);
    let enter = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
    if !resp.has_focus() {
        resp.request_focus();
    }
    // Magnifier glyph (13px, stroke 1.4), left of the text.
    let c = egui::pos2(resp.rect.left() - 13.0, resp.rect.center().y - 1.0);
    let p = ui.painter();
    let stroke = Stroke::new(1.4, TEXT_3);
    p.circle_stroke(c, 4.5, stroke);
    p.line_segment([c + vec2(3.3, 3.3), c + vec2(6.5, 6.5)], stroke);
    (resp, enter)
}

/// What the user did in a window's title bar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TitleAction {
    Minimize,
    Close,
}

/// Height of the title bar of the app's windows.
pub const TITLE_H: f32 = 40.0;

/// A whole app window as in the Figma frames (settings, translator): window
/// fill, 1px border, radius 12, 40px title bar (accent icon tile, title,
/// minimize/close), drag-to-move on the bar. `rect` is the window card in
/// window points (inside the transparent shadow margin). `body` fills the
/// area under the title bar. Esc doesn't close here: callers decide.
pub fn window_frame(
    ui: &mut Ui,
    rect: Rect,
    title: &str,
    extra_buttons: impl FnOnce(&mut Ui),
    body: impl FnOnce(&mut Ui),
) -> Option<TitleAction> {
    let mut action = None;
    let p = ui.painter().clone();
    p.add(SHADOW_WINDOW.as_shape(rect, CornerRadius::same(12)));
    p.rect_filled(rect, 12, WINDOW_BG);

    let bar = Rect::from_min_size(rect.min, vec2(rect.width(), TITLE_H));
    p.rect_filled(
        bar,
        CornerRadius { nw: 12, ne: 12, sw: 0, se: 0 },
        PANEL,
    );
    p.hline(bar.x_range(), bar.bottom() - 0.5, Stroke::new(1.0, BORDER));
    let drag = ui.interact(bar, ui.id().with("title-bar"), Sense::click_and_drag());
    if drag.drag_started() {
        crate::win::begin_window_drag();
    }

    let mut title_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(Rect::from_min_max(bar.min + vec2(14.0, 0.0), bar.max - vec2(6.0, 0.0)))
            .layout(Layout::left_to_right(Align::Center)),
    );
    title_ui.spacing_mut().item_spacing.x = 10.0;
    // The HeliLingo mark (brand kit 326:405) where the accent tile was.
    let (mark, _) = title_ui.allocate_exact_size(vec2(22.0, 22.0), Sense::hover());
    Image::new(icons::SUN).paint_at(&title_ui, mark);
    text(&mut title_ui, title, 13.0, Weight::SemiBold, TEXT);
    title_ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        if title_button(ui, icons::X15, crate::i18n::tr("Close")).clicked() {
            action = Some(TitleAction::Close);
        }
        if title_button(ui, icons::MINUS, crate::i18n::tr("Minimize")).clicked() {
            action = Some(TitleAction::Minimize);
        }
        extra_buttons(ui);
    });

    let body_rect = Rect::from_min_max(egui::pos2(rect.left(), bar.bottom()), rect.max);
    let mut body_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(body_rect)
            .layout(Layout::top_down(Align::Min)),
    );
    body(&mut body_ui);

    // Border last, over the content.
    p.rect_stroke(rect, 12, BORDER_STROKE, egui::StrokeKind::Inside);
    action
}

/// 28px square title-bar button with a 15px icon (radius 8 hover plate).
pub fn title_button(ui: &mut Ui, src: ImageSource<'static>, tip: &str) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(28.0, 28.0), Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(rect, 8, HOVER);
    }
    Image::new(src).paint_at(ui, Rect::from_center_size(rect.center(), vec2(15.0, 15.0)));
    resp.on_hover_cursor(CursorIcon::PointingHand).on_hover_text(tip)
}

/// `Чип` — outlined 30px picker chip: 12 Medium label and a 12px chevron
/// (language and provider pickers of the Quick and Ultra windows).
pub fn chip(ui: &mut Ui, label: &str, open: bool) -> Response {
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_owned(), font(12.0, Weight::Medium), TEXT_2);
    let size = vec2(10.0 + galley.size().x + 6.0 + 12.0 + 8.0, 30.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let border = if open {
        ACCENT
    } else if resp.hovered() {
        TEXT_3
    } else {
        BORDER
    };
    let p = ui.painter();
    if resp.hovered() {
        p.rect_filled(rect, 8, HOVER);
    }
    p.rect_stroke(rect, 8, Stroke::new(1.0, border), egui::StrokeKind::Inside);
    let y = rect.center().y;
    p.galley(egui::pos2(rect.left() + 10.0, y - galley.size().y / 2.0), galley.clone(), TEXT_2);
    let chev = Rect::from_center_size(
        egui::pos2(rect.left() + 10.0 + galley.size().x + 6.0 + 6.0, y),
        vec2(12.0, 12.0),
    );
    Image::new(icons::CHEV12).paint_at(ui, chev);
    resp.on_hover_cursor(CursorIcon::PointingHand)
}

/// 36px window button, radius 10: `primary` = accent fill with dark text
/// ("Replace selection"), otherwise panel fill with a border ("Copy all").
pub fn button(ui: &mut Ui, label: &str, primary: bool) -> Response {
    let (weight, color) = if primary { (Weight::SemiBold, CANVAS) } else { (Weight::Medium, TEXT) };
    let galley = ui.painter().layout_no_wrap(label.to_owned(), font(13.0, weight), color);
    let size = vec2(14.0 + galley.size().x + 16.0, 36.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let p = ui.painter();
    if primary {
        p.rect_filled(rect, 10, if resp.hovered() { ACCENT_HOVER } else { ACCENT });
    } else {
        p.rect_filled(rect, 10, if resp.hovered() { HOVER } else { PANEL });
        p.rect_stroke(rect, 10, BORDER_STROKE, egui::StrokeKind::Inside);
    }
    p.galley(
        egui::pos2(rect.left() + 14.0, rect.center().y - galley.size().y / 2.0),
        galley,
        color,
    );
    resp.on_hover_cursor(CursorIcon::PointingHand)
}

/// Square icon button with a radius-8 hover plate (`Кнопка / x`, `/ vol`).
pub fn square_button(ui: &mut Ui, src: ImageSource<'static>, side: f32, icon_side: f32, tip: &str) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(side, side), Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(rect, 8, HOVER);
    }
    Image::new(src).paint_at(ui, Rect::from_center_size(rect.center(), vec2(icon_side, icon_side)));
    let resp = resp.on_hover_cursor(CursorIcon::PointingHand);
    if tip.is_empty() { resp } else { resp.on_hover_text(tip) }
}

/// [`square_button`] that stays highlighted (selected fill) while `on`,
/// e.g. the Wikipedia button while its card is open.
pub fn toggle_square_button(ui: &mut Ui, src: ImageSource<'static>, side: f32, icon_side: f32, tip: &str, on: bool) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(side, side), Sense::click());
    if on {
        ui.painter().rect_filled(rect, 8, SELECTED);
    } else if resp.hovered() {
        ui.painter().rect_filled(rect, 8, HOVER);
    }
    Image::new(src).paint_at(ui, Rect::from_center_size(rect.center(), vec2(icon_side, icon_side)));
    resp.on_hover_cursor(CursorIcon::PointingHand).on_hover_text(tip)
}

/// `[key] hint` — a key cap followed by a muted 12px hint.
pub fn key_hint(ui: &mut Ui, key: &str, hint: &str) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        key_cap(ui, key);
        text(ui, hint, 12.0, Weight::Regular, TEXT_3);
    });
}

/// A floating list under (or above) `anchor`, e.g. a chip's language menu.
/// With `search`, a search box sits on top, keeps the keyboard focus, and
/// Enter picks the first row. Returns the clicked row and the menu's rect
/// (for click-outside checks).
pub fn floating_list(
    ctx: &egui::Context,
    id: &str,
    anchor: Rect,
    width: f32,
    items: &[(String, bool)],
    search: Option<&mut String>,
) -> (Option<usize>, Rect) {
    let mut picked = None;
    let rows = if search.is_some() { 7 } else { items.len().max(1) };
    let list_h = (rows as f32 * 35.0 - 1.0).min(232.0);
    let resp = egui::Area::new(egui::Id::new(id))
        .order(egui::Order::Foreground)
        .fixed_pos(egui::pos2(anchor.left(), anchor.bottom() + 6.0))
        .constrain(true)
        .show(ctx, |ui| {
            surface(ui, 8, Margin::same(4), |ui| {
                ui.set_width(width);
                let searchable = search.is_some();
                if let Some(q) = search {
                    let (_, enter) = search_field(ui, q, crate::i18n::tr("Search"), width);
                    if enter && !items.is_empty() {
                        picked = Some(0);
                    }
                    ui.add_space(6.0);
                }
                egui::ScrollArea::vertical()
                    .max_height(list_h)
                    .min_scrolled_height(if searchable { list_h } else { 0.0 })
                    .auto_shrink([!searchable, !searchable])
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing = vec2(0.0, 1.0);
                        if items.is_empty() {
                            ui.add_space(8.0);
                            ui.horizontal(|ui| {
                                ui.add_space(10.0);
                                text(ui, crate::i18n::tr("No languages found"), 13.0, Weight::Regular, TEXT_3);
                            });
                        }
                        for (i, (label, selected)) in items.iter().enumerate() {
                            if menu_item(ui, label, *selected, 34.0).clicked() {
                                picked = Some(i);
                            }
                        }
                    });
            });
        })
        .response;
    (picked, resp.rect)
}

/// 40px button with a 15px leading icon, radius 10, `pl 14 · icon · 8 ·
/// label · pr 16`; optional key caps after the label (`Кнопка / Выбрать
/// файл`, `Кнопка / Область экрана`).
pub fn icon_text_button(
    ui: &mut Ui,
    src: ImageSource<'static>,
    label: &str,
    primary: bool,
    keys: &[String],
) -> Response {
    let (weight, color) = if primary { (Weight::SemiBold, CANVAS) } else { (Weight::Medium, TEXT) };
    let galley = ui.painter().layout_no_wrap(label.to_owned(), font(13.0, weight), color);
    let caps: Vec<_> = keys
        .iter()
        .map(|k| ui.painter().layout_no_wrap(k.clone(), font(11.0, Weight::Medium), TEXT_3))
        .collect();
    let caps_w: f32 = caps.iter().map(|g| g.size().x + 14.0).sum::<f32>()
        + 3.0 * caps.len().saturating_sub(1) as f32;
    let extra = if caps.is_empty() { 0.0 } else { 8.0 + caps_w };
    let size = vec2(14.0 + 15.0 + 8.0 + galley.size().x + extra + 16.0, 40.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let p = ui.painter();
    if primary {
        p.rect_filled(rect, 10, if resp.hovered() { ACCENT_HOVER } else { ACCENT });
    } else {
        p.rect_filled(rect, 10, if resp.hovered() { HOVER } else { PANEL });
        p.rect_stroke(rect, 10, BORDER_STROKE, egui::StrokeKind::Inside);
    }
    let y = rect.center().y;
    Image::new(src).paint_at(ui, Rect::from_center_size(egui::pos2(rect.left() + 14.0 + 7.5, y), vec2(15.0, 15.0)));
    let mut x = rect.left() + 14.0 + 15.0 + 8.0;
    let gw = galley.size().x;
    p.galley(egui::pos2(x, y - galley.size().y / 2.0), galley, color);
    x += gw + 8.0;
    for cap in caps {
        let r = Rect::from_min_size(egui::pos2(x, y - (cap.size().y + 4.0) / 2.0), cap.size() + vec2(14.0, 4.0));
        p.rect_stroke(r, 4, BORDER_STROKE, egui::StrokeKind::Inside);
        x = r.right() + 3.0;
        p.galley(r.min + vec2(7.0, 2.0), cap, TEXT_3);
    }
    resp.on_hover_cursor(CursorIcon::PointingHand)
}

/// Dashed outline of a rounded rectangle (the image drop zone).
pub fn dashed_rect(ui: &Ui, rect: Rect, radius: f32, stroke: Stroke, dash: f32, gap: f32) {
    let r = radius.min(rect.width() / 2.0).min(rect.height() / 2.0);
    let mut pts = Vec::new();
    let corner = |pts: &mut Vec<egui::Pos2>, c: egui::Pos2, from: f32| {
        for i in 0..=8 {
            let a = from + std::f32::consts::FRAC_PI_2 * i as f32 / 8.0;
            pts.push(c + r * vec2(a.cos(), a.sin()));
        }
    };
    use std::f32::consts::PI;
    corner(&mut pts, egui::pos2(rect.right() - r, rect.top() + r), -PI / 2.0);
    corner(&mut pts, egui::pos2(rect.right() - r, rect.bottom() - r), 0.0);
    corner(&mut pts, egui::pos2(rect.left() + r, rect.bottom() - r), PI / 2.0);
    corner(&mut pts, egui::pos2(rect.left() + r, rect.top() + r), PI);
    pts.push(egui::pos2(rect.right() - r, rect.top()));
    ui.painter().extend(Shape::dashed_line(&pts, stroke, dash, gap));
}

/// Height of the compact title bar (Settings, Figma 329:2).
pub const COMPACT_TITLE_H: f32 = 36.0;

/// [`window_frame`] in the compact style of the Settings window: 36px bar
/// (`pl 12 pr 6`, gap 8), 22px HeliLingo mark, 12 SemiBold title, 24px
/// buttons with 13px icons (radius 6), tighter window shadow.
pub fn compact_window_frame(
    ui: &mut Ui,
    rect: Rect,
    title: &str,
    body: impl FnOnce(&mut Ui),
) -> Option<TitleAction> {
    let mut action = None;
    let p = ui.painter().clone();
    p.add(SHADOW_WINDOW_COMPACT.as_shape(rect, CornerRadius::same(12)));
    p.rect_filled(rect, 12, WINDOW_BG);

    let bar = Rect::from_min_size(rect.min, vec2(rect.width(), COMPACT_TITLE_H));
    p.rect_filled(bar, CornerRadius { nw: 12, ne: 12, sw: 0, se: 0 }, PANEL);
    p.hline(bar.x_range(), bar.bottom() - 0.5, Stroke::new(1.0, BORDER));
    let drag = ui.interact(bar, ui.id().with("title-bar"), Sense::click_and_drag());
    if drag.drag_started() {
        crate::win::begin_window_drag();
    }

    let mut title_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(Rect::from_min_max(bar.min + vec2(12.0, 0.0), bar.max - vec2(6.0, 0.0)))
            .layout(Layout::left_to_right(Align::Center)),
    );
    title_ui.spacing_mut().item_spacing.x = 8.0;
    let (mark, _) = title_ui.allocate_exact_size(vec2(22.0, 22.0), Sense::hover());
    Image::new(icons::SUN).paint_at(&title_ui, mark);
    text(&mut title_ui, title, 12.0, Weight::SemiBold, TEXT);
    title_ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        let button = |ui: &mut Ui, src: ImageSource<'static>, tip: &str| {
            let (r, resp) = ui.allocate_exact_size(vec2(24.0, 24.0), Sense::click());
            if resp.hovered() {
                ui.painter().rect_filled(r, 6, HOVER);
            }
            Image::new(src).paint_at(ui, Rect::from_center_size(r.center(), vec2(13.0, 13.0)));
            resp.on_hover_cursor(CursorIcon::PointingHand).on_hover_text(tip)
        };
        if button(ui, icons::X15, crate::i18n::tr("Close")).clicked() {
            action = Some(TitleAction::Close);
        }
        if button(ui, icons::MINUS, crate::i18n::tr("Minimize")).clicked() {
            action = Some(TitleAction::Minimize);
        }
    });

    let body_rect = Rect::from_min_max(egui::pos2(rect.left(), bar.bottom()), rect.max);
    let mut body_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(body_rect)
            .layout(Layout::top_down(Align::Min)),
    );
    body(&mut body_ui);

    p.rect_stroke(rect, 12, BORDER_STROKE, egui::StrokeKind::Inside);
    action
}

/// `Переключатель`, 32×18 (compact frames): the exported "on" SVG; the
/// off state mirrors its geometry (`#232c4f` track, 12px muted knob).
pub fn switch(ui: &mut Ui, on: &mut bool) -> Response {
    switch_enabled(ui, on, true)
}

/// [`switch`] that can be greyed out (half opacity, not clickable).
pub fn switch_enabled(ui: &mut Ui, on: &mut bool, enabled: bool) -> Response {
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let (rect, resp) = ui.allocate_exact_size(vec2(32.0, 18.0), sense);
    let fade = |c: Color32| if enabled { c } else { c.gamma_multiply(0.5) };
    if *on {
        Image::new(icons::SWITCH_ON_SM).tint(fade(Color32::WHITE)).paint_at(ui, rect);
    } else {
        let p = ui.painter();
        p.rect_filled(rect, 9, fade(SWITCH_OFF_TRACK));
        p.circle_filled(egui::pos2(rect.left() + 9.0, rect.center().y), 6.0, fade(SWITCH_OFF_KNOB));
    }
    if !enabled {
        return resp;
    }
    if resp.clicked() {
        *on = !*on;
    }
    resp.on_hover_cursor(CursorIcon::PointingHand)
}

/// `Сегменты` in the compact style: field well (radius 7, 2px padding,
/// gap 2), segments `px 10 py 4` radius 5, 11px labels (SemiBold accent on
/// the active one). Segments whose `enabled` entry is false are muted, not
/// clickable and show `disabled_tip` on hover. Returns the clicked index.
pub fn segmented_sm(
    ui: &mut Ui,
    options: &[&str],
    active: usize,
    enabled: &[bool],
    disabled_tip: &str,
) -> Option<usize> {
    let on = |i: usize| enabled.get(i).copied().unwrap_or(true);
    let segments: Vec<_> = options
        .iter()
        .enumerate()
        .map(|(i, label)| {
            let (w, c) = if i == active {
                (Weight::SemiBold, ACCENT)
            } else if on(i) {
                (Weight::Medium, TEXT_2)
            } else {
                (Weight::Medium, TEXT_3)
            };
            (ui.painter().layout_no_wrap((*label).to_owned(), font(11.0, w), c), c)
        })
        .collect();
    let seg_h = segments.iter().map(|(g, _)| g.size().y).fold(0.0, f32::max) + 8.0;
    let total_w = segments.iter().map(|(g, _)| g.size().x + 20.0).sum::<f32>()
        + 2.0 * (segments.len() as f32 - 1.0)
        + 4.0;
    let (outer, base) = ui.allocate_exact_size(vec2(total_w, seg_h + 4.0), Sense::hover());
    ui.painter().rect_filled(outer, 7, FIELD);
    ui.painter().rect_stroke(outer, 7, BORDER_STROKE, egui::StrokeKind::Inside);

    let mut clicked = None;
    let mut x = outer.left() + 2.0;
    for (i, (galley, color)) in segments.into_iter().enumerate() {
        let rect = Rect::from_min_size(egui::pos2(x, outer.top() + 2.0), vec2(galley.size().x + 20.0, seg_h));
        x = rect.right() + 2.0;
        let sense = if on(i) { Sense::click() } else { Sense::hover() };
        let resp = ui.interact(rect, base.id.with(("segment", i)), sense);
        if i == active {
            ui.painter().rect_filled(rect, 5, SELECTED);
        } else if on(i) && resp.hovered() {
            ui.painter().rect_filled(rect, 5, HOVER);
        }
        ui.painter().galley(rect.min + vec2(10.0, 4.0), galley, color);
        if !on(i) {
            resp.on_hover_text(disabled_tip);
        } else if resp.on_hover_cursor(CursorIcon::PointingHand).clicked() && i != active {
            clicked = Some(i);
        }
    }
    clicked
}

/// Status pill: radius 9, `px 7 py 2`, 10.5 Medium,
/// optional 5px dot (gap 5).
pub fn badge(ui: &mut Ui, label: &str, fg: Color32, bg: Color32, dot: Option<ImageSource<'static>>) -> Response {
    let galley = ui.painter().layout_no_wrap(label.to_owned(), font(10.5, Weight::Medium), fg);
    let dot_w = if dot.is_some() { 5.0 + 5.0 } else { 0.0 };
    let size = vec2(14.0 + dot_w + galley.size().x, galley.size().y + 4.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::hover());
    ui.painter().rect_filled(rect, 9, bg);
    if let Some(src) = dot {
        let r = Rect::from_center_size(egui::pos2(rect.left() + 7.0 + 2.5, rect.center().y), vec2(5.0, 5.0));
        Image::new(src).paint_at(ui, r);
    }
    ui.painter().galley(egui::pos2(rect.left() + 7.0 + dot_w, rect.top() + 2.0), galley, fg);
    resp
}

/// A window card that can "unfold" when it opens: at `progress` p (0..1,
/// eased) its background and border are `full` narrowed around the centre
/// to a pill (10% of the width at p = 0) widening to full width; nothing
/// is scaled, so text never squashes. The content is clipped to the
/// background and fades in over the last 40%. `progress: None` = a normal
/// card. `full` is the card's rect from the previous frame (its size
/// doesn't change while it unfolds).
pub fn unfold_card<R>(
    ui: &mut Ui,
    progress: Option<f32>,
    full: Rect,
    radius: u8,
    fill: Color32,
    add: impl FnOnce(&mut Ui) -> R,
) -> InnerResponse<R> {
    let Some(p) = progress.filter(|_| full.is_positive()) else {
        return Frame::new()
            .fill(fill)
            .stroke(BORDER_STROKE)
            .corner_radius(radius)
            .shadow(SHADOW_WINDOW)
            .show(ui, add);
    };
    let w = full.width() * (0.1 + 0.9 * p);
    let rect = Rect::from_center_size(full.center(), vec2(w, full.height()));
    // A pill while narrow; the corners settle to `radius` by halfway, so
    // the card is its normal shape by the time the content fades in.
    let pill = w.min(full.height()) / 2.0;
    let k = (p * 2.0).min(1.0);
    let r = pill + (radius as f32 - pill) * (k * k * (3.0 - 2.0 * k));
    let r = CornerRadius::same(r.clamp(0.0, 255.0) as u8);
    let bg = ui.painter().add(Shape::Noop);
    let content_alpha = ((p - 0.6) / 0.4).clamp(0.0, 1.0);
    ui.set_clip_rect(rect.expand(1.0));
    // Same layout as the normal card: an invisible 1px stroke.
    let inner = Frame::new()
        .stroke(Stroke::new(1.0, Color32::TRANSPARENT))
        .corner_radius(radius)
        .show(ui, |ui| {
            ui.multiply_opacity(content_alpha);
            add(ui)
        });
    let mut shadow = SHADOW_WINDOW;
    shadow.color = shadow.color.gamma_multiply(p);
    ui.painter().set(
        bg,
        Shape::Vec(vec![
            shadow.as_shape(rect, r).into(),
            Shape::rect_filled(rect, r, fill),
            Shape::rect_stroke(rect, r, BORDER_STROKE, egui::StrokeKind::Inside),
        ]),
    );
    inner
}
