//! The Images tab: the drop zone (315:127), the recognised picture with
//! its text blocks and the translated picture or text (315:244), and the
//! rasteriser that draws the translated picture for Copy / Save.

use std::sync::{Arc, OnceLock};

use egui::text::LayoutJob;
use egui::{
    Align, Color32, ColorImage, Image, Layout, Pos2, Rect, ScrollArea, Sense, Stroke,
    TextureHandle, TextureOptions, Ui, Vec2, pos2, vec2,
};

use super::{Env, MainAction, MainState, PANEL_FOOT_H, child, error_text, panel_frame, provider_chip};
use crate::i18n::{is_russian, plural, tr, trf};
use crate::icons;
use crate::settings::{ProviderKind, lang_name};
use crate::theme::*;
use crate::translate::Error;
use crate::ui::widgets::*;
use crate::win::image::{LoadError, RgbaImage};
use crate::win::ocr::{Block, Bounds, OcrError};

/// Why there is no translated picture.
#[derive(Clone, Debug)]
pub enum Failure {
    Load(LoadError),
    Ocr(OcrError),
    /// OCR found nothing.
    NoText,
    Translate(Error),
    /// The screen could not be captured.
    Capture,
    /// Ctrl+V with no picture on the clipboard.
    EmptyClipboard,
}

pub enum ImageSlot {
    Empty,
    /// Reading / decoding a file or the clipboard.
    Loading,
    Failed(Failure),
    Doc(ImageDoc),
}

pub enum Stage {
    Recognizing,
    /// OCR is done; the blocks are being translated.
    Translating(Vec<Block>),
    Done(ImageResult),
    Failed(Failure),
}

/// A picture in the Images tab and how far its translation got.
pub struct ImageDoc {
    pub image: Arc<RgbaImage>,
    pub stage: Stage,
    /// "As text" instead of "As picture".
    pub as_text: bool,
    texture: Option<TextureHandle>,
    /// The zoom viewer, while open.
    zoom: Option<Zoom>,
}

/// The enlarged view of the original or the translated picture.
struct Zoom {
    /// The translated picture (else the original with its text boxes).
    result: bool,
    /// Points per picture pixel; `None` = fit to the viewer.
    scale: Option<f32>,
    /// Offset of the picture centre from the viewer centre, in points.
    pan: Vec2,
}

impl ImageDoc {
    pub fn new(image: Arc<RgbaImage>) -> Self {
        Self { image, stage: Stage::Recognizing, as_text: false, texture: None, zoom: None }
    }

    fn texture(&mut self, ctx: &egui::Context) -> TextureHandle {
        self.texture
            .get_or_insert_with(|| {
                // Very large photos are shown downscaled (GPU texture limit).
                let max = 8192u32;
                let img = &self.image;
                let shown = if img.width.max(img.height) > max {
                    let k = max as f32 / img.width.max(img.height) as f32;
                    img.resized((img.width as f32 * k) as u32, (img.height as f32 * k) as u32)
                } else {
                    (**img).clone()
                };
                let color = ColorImage::from_rgba_unmultiplied(
                    [shown.width as usize, shown.height as usize],
                    &shown.rgba,
                );
                ctx.load_texture("qt-main-image", color, TextureOptions::LINEAR)
            })
            .clone()
    }
}

pub struct ImageResult {
    pub blocks: Vec<TranslatedBlock>,
    /// BCP-47 tag of the OCR recogniser.
    pub ocr_lang: String,
    /// Source language the translation service detected (or the chosen one).
    pub src_lang: Option<String>,
    pub provider: Option<ProviderKind>,
    pub to: String,
}

impl ImageResult {
    /// The recognised text, block by block.
    pub fn source_text(&self) -> String {
        self.blocks.iter().map(|b| b.block.text.as_str()).collect::<Vec<_>>().join("\n")
    }

    /// The translated text, block by block.
    pub fn translated_text(&self) -> String {
        self.blocks.iter().map(|b| b.text.as_str()).collect::<Vec<_>>().join("\n")
    }
}

pub struct TranslatedBlock {
    pub block: Block,
    pub text: String,
    /// Background around the block and its text colour, from the picture.
    pub bg: Color32,
    pub fg: Color32,
    fit: Option<Fit>,
}

impl TranslatedBlock {
    pub fn new(block: Block, text: String, (bg, fg): (Color32, Color32)) -> Self {
        Self { block, text, bg, fg, fit: None }
    }
}

/// How a translation fits its block, in picture pixels.
#[derive(Clone, Copy, Debug)]
struct Fit {
    size: f32,
    wrap: f32,
    center: bool,
    /// The area painted over (original block and the new text).
    cover: Rect,
    /// Top-left of the text (centre-top when centred).
    origin: Pos2,
}

/// Measuring is done at one size and scaled, so fitting doesn't fill the
/// font atlas with every size tried.
const REF_SIZE: f32 = 32.0;

fn job(text: &str, size: f32, color: Color32, wrap: f32, center: bool) -> LayoutJob {
    let mut job = LayoutJob::simple(text.to_owned(), font(size, Weight::Medium), color, wrap);
    if center {
        job.halign = Align::Center;
    }
    job
}

fn compute_fit(ui: &Ui, tb: &TranslatedBlock, img: &RgbaImage) -> Fit {
    let b = tb.block.bounds;
    let lh = tb.block.line_height.max(4.0);
    let pad = lh * 0.25;
    let center = tb.block.lines == 1;
    let max_w = if center { (b.w * 1.35).max(b.w + 2.0 * lh) } else { b.w + 2.0 * pad }
        .min(img.width as f32);
    let max_h = b.h + 2.0 * pad;
    let measure = |s: f32| {
        let g = ui.painter().layout_job(job(&tb.text, REF_SIZE, TEXT, max_w * REF_SIZE / s, center));
        let k = s / REF_SIZE;
        (g.size().x * k, g.size().y * k)
    };
    // egui breaks a word that is wider than the wrap width; never shrink
    // less than what keeps the longest word whole.
    let longest = tb
        .text
        .split_whitespace()
        .map(|w| ui.painter().layout_no_wrap(w.to_owned(), font(REF_SIZE, Weight::Medium), TEXT).size().x)
        .fold(0.0, f32::max);
    let fits = |s: f32| {
        let (w, h) = measure(s);
        w <= max_w + 0.5 && h <= max_h + 0.5 && longest * s / REF_SIZE <= max_w
    };
    let (mut lo, mut hi) = (3.0f32, (lh * 1.3).max(3.5));
    if fits(hi) {
        lo = hi;
    } else {
        for _ in 0..9 {
            let mid = (lo + hi) / 2.0;
            if fits(mid) { lo = mid } else { hi = mid }
        }
    }
    let size = lo;
    let (tw, th) = measure(size);
    let (cx, cy) = (b.x + b.w / 2.0, b.y + b.h / 2.0);
    let left = if center { cx - tw / 2.0 } else { b.x };
    let text_rect = Rect::from_min_size(pos2(left, cy - th / 2.0), vec2(tw, th));
    let block_rect = Rect::from_min_size(pos2(b.x, b.y), vec2(b.w, b.h)).expand(pad);
    let full = Rect::from_min_size(Pos2::ZERO, vec2(img.width as f32, img.height as f32));
    let cover = block_rect.union(text_rect.expand(pad * 0.5)).intersect(full);
    let origin = if center { pos2(cx, text_rect.top()) } else { text_rect.min };
    Fit { size, wrap: max_w, center, cover, origin }
}

/// Background (median of a ring around the block) and text colour (median
/// of the pixels inside that differ from it; dark or light as a fallback).
pub fn block_colors(img: &RgbaImage, b: &Bounds) -> (Color32, Color32) {
    let pad = b.h.min(b.w) * 0.25 + 2.0;
    let (x0, y0) = ((b.x - pad).max(0.0), (b.y - pad).max(0.0));
    let (x1, y1) = ((b.right() + pad).min(img.width as f32 - 1.0), (b.bottom() + pad).min(img.height as f32 - 1.0));
    let mut ring = Vec::new();
    let steps = 60;
    for i in 0..=steps {
        let t = i as f32 / steps as f32;
        let x = x0 + (x1 - x0) * t;
        let y = y0 + (y1 - y0) * t;
        for (px, py) in [(x, y0), (x, y1), (x0, y), (x1, y)] {
            ring.push(img.pixel(px as u32, py as u32));
        }
    }
    let bg = median(&mut ring);
    let mut inside = Vec::new();
    let (nx, ny) = (48, 24);
    for j in 0..ny {
        for i in 0..nx {
            let x = b.x + b.w * (i as f32 + 0.5) / nx as f32;
            let y = b.y + b.h * (j as f32 + 0.5) / ny as f32;
            let p = img.pixel(x as u32, y as u32);
            if distance(p, bg) > 80.0 {
                inside.push(p);
            }
        }
    }
    let bg_c = Color32::from_rgb(bg[0], bg[1], bg[2]);
    let fallback = if luminance(bg) > 0.5 { Color32::from_rgb(0x26, 0x26, 0x2c) } else { Color32::from_rgb(0xf4, 0xf5, 0xf8) };
    let fg = if inside.len() >= nx * ny / 40 {
        let m = median(&mut inside);
        if (luminance(m) - luminance(bg)).abs() > 0.3 { Color32::from_rgb(m[0], m[1], m[2]) } else { fallback }
    } else {
        fallback
    };
    (bg_c, fg)
}

fn median(px: &mut [[u8; 4]]) -> [u8; 4] {
    let mut out = [0, 0, 0, 255];
    if px.is_empty() {
        return out;
    }
    for (k, o) in out.iter_mut().enumerate().take(3) {
        px.sort_unstable_by_key(|p| p[k]);
        *o = px[px.len() / 2][k];
    }
    out
}

fn distance(a: [u8; 4], b: [u8; 4]) -> f32 {
    (0..3).map(|k| (a[k] as f32 - b[k] as f32).powi(2)).sum::<f32>().sqrt()
}

fn luminance(p: [u8; 4]) -> f32 {
    (0.2126 * p[0] as f32 + 0.7152 * p[1] as f32 + 0.0722 * p[2] as f32) / 255.0
}

// ------------------------------------------------------------------ UI

pub fn panels(ui: &mut Ui, left: Rect, right: Rect, st: &mut MainState, out: &mut Vec<MainAction>) {
    let target = target_phrase(&st.to);
    match &mut st.image {
        ImageSlot::Empty => {
            drop_zone(ui, left, &st.env, None, out);
            placeholder(ui, right, &target);
        }
        ImageSlot::Loading => {
            drop_zone(ui, left, &st.env, Some(None), out);
            placeholder(ui, right, &target);
        }
        ImageSlot::Failed(f) => {
            let f = f.clone();
            drop_zone(ui, left, &st.env, Some(Some(&f)), out);
            placeholder(ui, right, &target);
        }
        ImageSlot::Doc(_) => {
            let mut another = false;
            if let ImageSlot::Doc(doc) = &mut st.image {
                another = original_panel(ui, left, doc);
            }
            result_panel(ui, right, st, out);
            if let ImageSlot::Doc(doc) = &mut st.image
                && st.zoom_when_done
                && matches!(doc.stage, Stage::Done(_))
            {
                st.zoom_when_done = false;
                doc.zoom = Some(Zoom { result: true, scale: None, pan: Vec2::ZERO });
            }
            if let ImageSlot::Doc(doc) = &mut st.image
                && doc.zoom.is_some()
            {
                zoom_view(ui, left.union(right), doc);
            }
            if another {
                st.image = ImageSlot::Empty;
            }
        }
    }
}

/// "The text in the picture will be replaced in Russian".
fn target_phrase(to: &str) -> String {
    let mut name = lang_name(to);
    if is_russian() {
        name = name.to_lowercase();
    }
    trf("The text in the picture will be replaced in {lang}", &[("lang", &name)])
}

/// `Чип / Распознавание: Windows OCR` and the format note.
pub fn footer(ui: &mut Ui) {
    ui.spacing_mut().item_spacing.x = 10.0;
    let label = tr("Recognition: Windows OCR");
    let galley = ui.painter().layout_no_wrap(label.to_owned(), font(12.0, Weight::Medium), TEXT_2);
    let (rect, _) = ui.allocate_exact_size(vec2(10.0 + 14.0 + 6.0 + galley.size().x + 10.0, 30.0), Sense::hover());
    ui.painter().rect_stroke(rect, 8, BORDER_STROKE, egui::StrokeKind::Inside);
    Image::new(icons::LANG14).paint_at(ui, Rect::from_center_size(pos2(rect.left() + 17.0, rect.center().y), vec2(14.0, 14.0)));
    ui.painter().galley(pos2(rect.left() + 30.0, rect.center().y - galley.size().y / 2.0), galley, TEXT_2);
    text(ui, tr("PNG, JPG, WEBP · up to 20 MB"), 12.0, Weight::Regular, TEXT_3);
}

/// The dashed drop zone. `state`: `None` = idle, `Some(None)` = opening,
/// `Some(Some(f))` = the last attempt failed.
fn drop_zone(ui: &mut Ui, panel: Rect, env: &Env, state: Option<Option<&Failure>>, out: &mut Vec<MainAction>) {
    let hovering = ui.ctx().input(|i| !i.raw.hovered_files.is_empty());
    ui.painter().rect_filled(panel, 16, if hovering { SELECTED } else { PANEL });
    let stroke = Stroke::new(if hovering { 1.5 } else { 1.0 }, if hovering { ACCENT } else { BORDER });
    dashed_rect(ui, panel.shrink(0.5), 16.0, stroke, 6.0, 4.0);

    let mut col = child(ui, panel, Layout::top_down(Align::Center));
    col.spacing_mut().item_spacing.y = 10.0;
    let content_h = 44.0 + 10.0 + 20.0 + 10.0 + 18.0 + 10.0 + 10.0 + 40.0;
    col.add_space(((panel.height() - content_h) / 2.0).max(8.0));

    // 52×44 pill (radius 22) with the picture glyph.
    let (circle, _) = col.allocate_exact_size(vec2(52.0, 44.0), Sense::hover());
    col.painter().rect_filled(circle, 22, SELECTED);
    if matches!(state, Some(None)) {
        egui::Spinner::new().size(22.0).color(ACCENT).paint_at(&col, Rect::from_center_size(circle.center(), vec2(22.0, 22.0)));
    } else {
        Image::new(icons::IMAGE24).paint_at(&col, Rect::from_center_size(circle.center(), vec2(24.0, 24.0)));
    }
    match state {
        Some(Some(f)) => {
            let (msg, hint) = failure_text(f);
            text(&mut col, msg, 16.0, Weight::Medium, WARN);
            col.add(egui::Label::new(rt(hint, 13.0, Weight::Regular, TEXT_3)).wrap_mode(egui::TextWrapMode::Wrap));
        }
        Some(None) => {
            text(&mut col, tr("Opening the image…"), 16.0, Weight::Medium, TEXT);
            col.add_space(18.0);
        }
        None => {
            let title = if hovering { tr("Drop the image to translate it") } else { tr("Drag an image here") };
            text(&mut col, title, 16.0, Weight::Medium, TEXT);
            col.horizontal(|ui| {
                let w = clipboard_hint_width(ui);
                ui.add_space(((ui.available_width() - w) / 2.0).max(0.0));
                ui.spacing_mut().item_spacing.x = 6.0;
                text(ui, tr("or paste from the clipboard"), 13.0, Weight::Regular, TEXT_3);
                key_cap(ui, "Ctrl");
                key_cap(ui, "V");
            });
        }
    }
    col.add_space(10.0);
    col.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 10.0;
        // Key caps on "Screen area" only when the row still fits the panel;
        // otherwise they move into the tooltip.
        let room = panel.width() - 24.0;
        let caps = if buttons_width(ui, &env.screen_keys) <= room { env.screen_keys.as_slice() } else { &[] };
        let w = buttons_width(ui, caps);
        ui.add_space(((ui.available_width() - w) / 2.0).max(0.0));
        if icon_text_button(ui, icons::IMAGE15, tr("Choose file"), true, &[]).clicked() {
            out.push(MainAction::ChooseFile);
        }
        let mut area = icon_text_button(ui, icons::CROP, tr("Screen area"), false, caps);
        if caps.is_empty() && !env.screen_keys.is_empty() {
            area = area.on_hover_text(env.screen_keys.join(" + "));
        }
        if area.clicked() {
            out.push(MainAction::ScreenArea);
        }
        let mut tip = tr("Translate the text on the whole screen").to_owned();
        if !env.whole_screen_keys.is_empty() {
            tip = format!("{tip} · {}", env.whole_screen_keys.join(" + "));
        }
        if square_outline_button(ui, icons::MONITOR15, &tip).clicked() {
            out.push(MainAction::WholeScreen);
        }
    });
}

/// 40px square companion of [`icon_text_button`] (panel fill, border,
/// radius 10, 15px glyph): "Entire screen" next to "Screen area".
fn square_outline_button(ui: &mut Ui, src: egui::ImageSource<'static>, tip: &str) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(40.0, 40.0), Sense::click());
    let p = ui.painter();
    p.rect_filled(rect, 10, if resp.hovered() { HOVER } else { PANEL });
    p.rect_stroke(rect, 10, BORDER_STROKE, egui::StrokeKind::Inside);
    Image::new(src).paint_at(ui, Rect::from_center_size(rect.center(), vec2(15.0, 15.0)));
    resp.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(tip)
}

fn clipboard_hint_width(ui: &Ui) -> f32 {
    let w = |s: &str, size, weight| ui.painter().layout_no_wrap(s.to_owned(), font(size, weight), TEXT).size().x;
    w(tr("or paste from the clipboard"), 13.0, Weight::Regular) + 6.0 + (w("Ctrl", 11.0, Weight::Medium) + 14.0) + 6.0 + (w("V", 11.0, Weight::Medium) + 14.0)
}

fn buttons_width(ui: &Ui, keys: &[String]) -> f32 {
    let w = |s: &str, size, weight| ui.painter().layout_no_wrap(s.to_owned(), font(size, weight), TEXT).size().x;
    let caps: f32 = keys.iter().map(|k| w(k, 11.0, Weight::Medium) + 14.0).sum::<f32>() + 3.0 * keys.len().saturating_sub(1) as f32;
    let first = 14.0 + 15.0 + 8.0 + w(tr("Choose file"), 13.0, Weight::SemiBold) + 16.0;
    let second = 14.0 + 15.0 + 8.0 + w(tr("Screen area"), 13.0, Weight::Medium) + if keys.is_empty() { 0.0 } else { 8.0 + caps } + 16.0;
    first + 10.0 + second + 10.0 + 40.0
}

/// Right panel before there is a picture.
fn placeholder(ui: &mut Ui, panel: Rect, target: &str) {
    panel_frame(ui, panel, TRANSLATION_PANEL);
    centered_message(ui, panel, tr("The translation will appear here"), target, TEXT_3);
}

fn centered_message(ui: &mut Ui, rect: Rect, title: &str, hint: &str, title_color: Color32) {
    let mut col = child(ui, rect.shrink2(vec2(24.0, 0.0)), Layout::top_down(Align::Center));
    col.spacing_mut().item_spacing.y = 6.0;
    col.add_space((rect.height() / 2.0 - 22.0).max(0.0));
    text(&mut col, title, 15.0, Weight::Medium, title_color);
    col.add(egui::Label::new(rt(hint, 13.0, Weight::Regular, TEXT_3)).wrap_mode(egui::TextWrapMode::Wrap).halign(Align::Center));
}

/// Message and hint for a failure.
pub fn failure_text(f: &Failure) -> (String, String) {
    let s = |a: &str, b: &str| (a.to_owned(), b.to_owned());
    match f {
        Failure::Load(LoadError::TooLarge) => s(tr("The image is larger than 20 MB"), tr("Choose a smaller image")),
        Failure::Load(LoadError::Unsupported) => s(tr("Can’t open this image"), tr("Use a PNG, JPG or WEBP image")),
        Failure::Load(LoadError::Unreadable) => s(tr("Can’t read the file"), tr("Check that the file still exists")),
        Failure::Ocr(OcrError::NoLanguage(Some(code))) => {
            let name = lang_name(code);
            (
                trf("Windows can’t recognise {lang} text", &[("lang", &name)]),
                trf(
                    "Add the {lang} language with “Optical character recognition” in Windows Settings → Time & language → Language & region",
                    &[("lang", &name)],
                ),
            )
        }
        Failure::Ocr(OcrError::NoLanguage(None)) => s(
            tr("No text recognition language is installed"),
            tr("Add a language with “Optical character recognition” in Windows Settings → Time & language → Language & region"),
        ),
        Failure::Ocr(OcrError::Failed(m)) => s(tr("Text recognition failed"), m),
        Failure::NoText => s(tr("No text found in the picture"), tr("Try a sharper or larger image, or choose the source language")),
        Failure::Translate(e) => {
            let (a, b) = error_text(e);
            s(a, b)
        }
        Failure::Capture => s(tr("Couldn’t capture the screen"), tr("Try again")),
        Failure::EmptyClipboard => s(tr("There is no picture on the clipboard"), tr("Copy an image or a screenshot, then press Ctrl+V")),
    }
}

/// Fits a picture into `area` (centred, at most 3× enlarged).
fn fit_rect(area: Rect, img: &RgbaImage) -> (Rect, f32) {
    let k = (area.width() / img.width as f32).min(area.height() / img.height as f32).min(3.0);
    let size = vec2(img.width as f32 * k, img.height as f32 * k);
    (Rect::from_center_size(area.center(), size), k)
}

fn to_screen(b: &Bounds, origin: Pos2, k: f32) -> Rect {
    Rect::from_min_size(origin + vec2(b.x, b.y) * k, vec2(b.w, b.h) * k)
}

/// Left panel with a picture: found blocks outlined, status, "Another
/// image". Returns true when "Another image" was clicked.
fn original_panel(ui: &mut Ui, panel: Rect, doc: &mut ImageDoc) -> bool {
    panel_frame(ui, panel, PANEL);
    let area = Rect::from_min_max(panel.min + vec2(10.0, 10.0), pos2(panel.right() - 10.0, panel.bottom() - PANEL_FOOT_H));
    let tex = doc.texture(ui.ctx());
    let (rect, k) = fit_rect(area, &doc.image);
    Image::new(&tex).corner_radius(10).paint_at(ui, rect);
    paint_found(&ui.painter_at(area.expand(4.0)), doc, rect, k);
    if zoom_trigger(ui, rect, "qt-zoom-original") {
        doc.zoom = Some(Zoom { result: false, scale: None, pan: Vec2::ZERO });
    }

    let foot = Rect::from_min_max(pos2(panel.left() + 16.0, panel.bottom() - PANEL_FOOT_H), pos2(panel.right() - 16.0, panel.bottom()));
    let mut f = child(ui, foot, Layout::left_to_right(Align::Center));
    f.spacing_mut().item_spacing.x = 10.0;
    let (dot, status) = match &doc.stage {
        Stage::Recognizing => (icons::DOT_LOADING, tr("Recognizing text…").to_owned()),
        Stage::Translating(b) => (icons::DOT_LOADING, format!("{} · {}", found_blocks(b.len()), tr("translating…"))),
        Stage::Done(r) => {
            let lang = r
                .src_lang
                .clone()
                .unwrap_or_else(|| r.ocr_lang.split('-').next().unwrap_or("").to_owned());
            let mut name = lang_name(&lang);
            if is_russian() {
                name = name.to_lowercase();
            }
            (icons::DOT_OK6, format!("{} · {}", found_blocks(r.blocks.len()), name))
        }
        Stage::Failed(_) => (icons::DOT_WARN, tr("No translation").to_owned()),
    };
    let (dot_rect, _) = f.allocate_exact_size(vec2(6.0, 6.0), Sense::hover());
    Image::new(dot).paint_at(&f, dot_rect);
    // The status gets what the "Another image" link leaves, truncated with
    // the full text as a tooltip, so the two never overlap.
    let link = f.painter().layout_no_wrap(tr("Another image").to_owned(), font(13.0, Weight::Medium), ACCENT);
    let room = (f.available_width() - link.size().x - 16.0).max(40.0);
    let full_w = f.painter().layout_no_wrap(status.clone(), font(12.0, Weight::Regular), TEXT_2).size().x;
    let galley = truncated(&f, &status, font(12.0, Weight::Regular), TEXT_2, room);
    let (r, resp) = f.allocate_exact_size(galley.size(), Sense::hover());
    f.painter().galley(r.min, galley, TEXT_2);
    if full_w > room {
        resp.on_hover_text(status);
    }
    let mut another = false;
    f.with_layout(Layout::right_to_left(Align::Center), |ui| {
        another = text_button(ui, tr("Another image"), 13.0, Weight::Medium, ACCENT, ACCENT_HOVER).clicked();
    });
    another
}

/// The found text blocks, outlined over the original picture drawn at
/// `rect` (`k` points per picture pixel).
fn paint_found(p: &egui::Painter, doc: &ImageDoc, rect: Rect, k: f32) {
    let blocks: Vec<&Block> = match &doc.stage {
        Stage::Translating(b) => b.iter().collect(),
        Stage::Done(r) => r.blocks.iter().map(|t| &t.block).collect(),
        _ => Vec::new(),
    };
    for b in blocks {
        let r = to_screen(&b.bounds, rect.min, k).expand(3.0);
        p.rect(r, 3, FOUND_FILL, Stroke::new(1.5, ACCENT), egui::StrokeKind::Middle);
    }
}

/// The translated text over the picture drawn at `rect`: each block's
/// background covered, the translation fitted into it.
fn paint_translated(ui: &Ui, p: &egui::Painter, r: &ImageResult, rect: Rect, k: f32) {
    for tb in &r.blocks {
        let Some(fit) = tb.fit else { continue };
        let cover = Rect::from_min_max(rect.min + fit.cover.min.to_vec2() * k, rect.min + fit.cover.max.to_vec2() * k);
        p.rect_filled(cover, 2, tb.bg);
        let size = ((fit.size * k) * 4.0).round() / 4.0;
        if size < 3.0 {
            continue;
        }
        // A little slack: rounding at the final size must not break a word
        // the fit measured as fitting.
        let wrap = fit.wrap * k * 1.04 + 2.0;
        let galley = ui.painter().layout_job(job(&tb.text, size, tb.fg, wrap, fit.center));
        p.galley(rect.min + fit.origin.to_vec2() * k, galley, tb.fg);
    }
}

/// A picture that opens the zoom viewer: click it, or its expand button
/// in the top-right corner. Returns true when asked to open.
fn zoom_trigger(ui: &mut Ui, rect: Rect, id: &str) -> bool {
    let resp = ui.interact(rect, egui::Id::new(id), Sense::click()).on_hover_cursor(egui::CursorIcon::ZoomIn);
    let button = Rect::from_min_size(pos2(rect.right() - 34.0, rect.top() + 6.0), vec2(28.0, 28.0));
    let b = ui.interact(button, egui::Id::new(id).with("button"), Sense::click());
    let fill = if b.hovered() { CARD } else { Color32::from_black_alpha(150) };
    ui.painter().rect_filled(button, 7, fill);
    Image::new(icons::EXPAND).paint_at(ui, Rect::from_center_size(button.center(), vec2(15.0, 15.0)));
    let b = b.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(tr("Enlarge"));
    resp.clicked() || b.clicked()
}

/// The zoom viewer over both panels (`bounds`): fit by default, the mouse
/// wheel zooms around the cursor, dragging pans, "Fit" / "1:1"; Esc, the
/// close button or a click beside the picture closes it. The original keeps
/// its text boxes, the translation its translated text.
fn zoom_view(ui: &mut Ui, bounds: Rect, doc: &mut ImageDoc) {
    let ctx = ui.ctx().clone();
    let tex = doc.texture(&ctx);
    let (iw, ih) = (doc.image.width as f32, doc.image.height as f32);
    let mut close = false;
    egui::Area::new(egui::Id::new("qt-image-zoom"))
        .order(egui::Order::Foreground)
        .fixed_pos(bounds.min)
        .constrain(false)
        .show(&ctx, |ui| {
            let (bounds, _) = ui.allocate_exact_size(bounds.size(), Sense::hover());
            ui.painter().rect_filled(bounds, 12, Color32::from_rgba_unmultiplied(9, 12, 24, 248));
            ui.painter().rect_stroke(bounds, 12, BORDER_STROKE, egui::StrokeKind::Inside);
            let Some(zoom) = doc.zoom.as_mut() else { return };

            // Toolbar: title left; percent, Fit, 1:1, close right.
            let bar = Rect::from_min_size(bounds.min + vec2(16.0, 8.0), vec2(bounds.width() - 24.0, 30.0));
            let area = Rect::from_min_max(pos2(bounds.left() + 12.0, bar.bottom() + 6.0), bounds.max - vec2(12.0, 12.0));
            let fit = (area.width() / iw).min(area.height() / ih);
            let scale = zoom.scale.unwrap_or(fit);
            let mut bar_ui = child(ui, bar, Layout::left_to_right(Align::Center));
            let title = if zoom.result { tr("Translation") } else { tr("Original") };
            text(&mut bar_ui, title, 14.0, Weight::SemiBold, TEXT);
            bar_ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 14.0;
                if square_button(ui, icons::X15_MUTED, 28.0, 15.0, tr("Close")).clicked() {
                    close = true;
                }
                if text_button(ui, "1:1", 13.0, Weight::Medium, TEXT_2, TEXT).clicked() {
                    zoom.scale = Some(1.0);
                    zoom.pan = Vec2::ZERO;
                }
                if text_button(ui, tr("Fit"), 13.0, Weight::Medium, TEXT_2, TEXT).clicked() {
                    zoom.scale = None;
                    zoom.pan = Vec2::ZERO;
                }
                text(ui, format!("{:.0}%", scale * 100.0), 12.0, Weight::Regular, TEXT_3);
            });

            // Wheel: zoom around the cursor; drag: pan.
            let resp = ui.interact(area, egui::Id::new("qt-image-zoom-area"), Sense::click_and_drag());
            let center = area.center() + zoom.pan;
            if resp.hovered() {
                let wheel = ui.input(|i| i.smooth_scroll_delta.y);
                if wheel != 0.0 {
                    let new = (scale * (1.0 + wheel * 0.0025)).clamp(fit.min(1.0) * 0.5, 8.0);
                    if let Some(c) = ui.input(|i| i.pointer.hover_pos()) {
                        let new_center = c - (c - center) * (new / scale);
                        zoom.pan = new_center - area.center();
                    }
                    zoom.scale = Some(new);
                }
            }
            if resp.dragged() {
                zoom.pan += resp.drag_delta();
            }
            let scale = zoom.scale.unwrap_or(fit);
            let rect = Rect::from_center_size(area.center() + zoom.pan, vec2(iw, ih) * scale);
            let p = ui.painter_at(area);
            p.image(tex.id(), rect, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
            let showing_result = zoom.result;
            let cursor = if resp.dragged() { egui::CursorIcon::Grabbing } else { egui::CursorIcon::Grab };
            let resp = resp.on_hover_cursor(cursor);
            // A click (not a drag) beside the picture closes the viewer.
            if resp.clicked() && resp.interact_pointer_pos().is_some_and(|pos| !rect.contains(pos)) {
                close = true;
            }
            if showing_result {
                if let Stage::Done(r) = &doc.stage {
                    paint_translated(ui, &p, r, rect, scale);
                }
            } else {
                paint_found(&p, doc, rect, scale);
            }
        });
    if close || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        doc.zoom = None;
    }
}

/// "Found 3 text blocks" (Russian: "Найдено 3 блока текста").
pub fn found_blocks(n: usize) -> String {
    if is_russian() {
        format!(
            "{} {n} {}",
            plural(n, "Найден", "Найдено", "Найдено"),
            plural(n, "блок текста", "блока текста", "блоков текста")
        )
    } else if n == 1 {
        "Found 1 text block".to_owned()
    } else {
        format!("Found {n} text blocks")
    }
}

/// Right panel with a picture: the translated picture or text, view
/// switch, provider, copy and save.
fn result_panel(ui: &mut Ui, panel: Rect, st: &mut MainState, out: &mut Vec<MainAction>) {
    panel_frame(ui, panel, TRANSLATION_PANEL);
    let area = Rect::from_min_max(panel.min + vec2(10.0, 10.0), pos2(panel.right() - 10.0, panel.bottom() - PANEL_FOOT_H));
    let ImageSlot::Doc(doc) = &mut st.image else { return };
    let tex = doc.texture(ui.ctx());
    let mut provider = None;
    let mut done = false;
    let mut zoom_request = false;
    match &mut doc.stage {
        Stage::Recognizing | Stage::Translating(_) => {
            let mut col = child(ui, area, Layout::top_down(Align::Center));
            col.add_space(area.height() / 2.0 - 24.0);
            col.add(egui::Spinner::new().size(18.0).color(TEXT_3));
            col.add_space(8.0);
            text(&mut col, tr("Translating…"), 15.0, Weight::Medium, TEXT_3);
        }
        Stage::Failed(f) => {
            let (msg, hint) = failure_text(f);
            centered_message(ui, area, &msg, &hint, WARN);
        }
        Stage::Done(r) => {
            done = true;
            provider = r.provider;
            if doc.as_text {
                let text_area = Rect::from_min_max(panel.min + vec2(20.0, 18.0), pos2(panel.right() - 12.0, panel.bottom() - PANEL_FOOT_H));
                let mut tui = child(ui, text_area, Layout::top_down(Align::Min));
                ScrollArea::vertical().id_salt("qt-main-image-text").auto_shrink([false, false]).show(&mut tui, |ui| {
                    ui.spacing_mut().item_spacing.y = 12.0;
                    for b in &r.blocks {
                        ui.add(egui::Label::new(rt(&b.text, 15.0, Weight::Regular, TEXT)).wrap().selectable(true));
                    }
                });
            } else {
                let (rect, k) = fit_rect(area, &doc.image);
                Image::new(&tex).corner_radius(10).paint_at(ui, rect);
                for tb in &mut r.blocks {
                    if tb.fit.is_none() {
                        tb.fit = Some(compute_fit(ui, tb, &doc.image));
                    }
                }
                paint_translated(ui, &ui.painter_at(rect), r, rect, k);
                if zoom_trigger(ui, rect, "qt-zoom-result") {
                    zoom_request = true;
                }
            }
        }
    }

    if zoom_request {
        doc.zoom = Some(Zoom { result: true, scale: None, pan: Vec2::ZERO });
    }
    let foot = Rect::from_min_max(pos2(panel.left() + 16.0, panel.bottom() - PANEL_FOOT_H), pos2(panel.right() - 12.0, panel.bottom()));
    let mut f = child(ui, foot, Layout::left_to_right(Align::Center));
    f.spacing_mut().item_spacing.x = 8.0;
    if !done {
        return;
    }
    // Segmented "As picture | As text" on a panel-coloured well.
    let well = f.painter().add(egui::Shape::Noop);
    let active = if doc.as_text { 1 } else { 0 };
    let seg = f.horizontal(|ui| segmented(ui, &[tr("As picture"), tr("As text")], active));
    f.painter().set(well, egui::epaint::RectShape::filled(seg.response.rect, 8, PANEL));
    let mut as_text = doc.as_text;
    if let Some(i) = seg.inner {
        as_text = i == 1;
    }
    doc.as_text = as_text;
    let (copy, save) = f
        .with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            let save_tip = if as_text { tr("Save the text") } else { tr("Save the picture") };
            let save = square_button(ui, icons::DOWNLOAD, 36.0, 17.0, save_tip).clicked();
            let copy = square_button(ui, icons::COPY17, 36.0, 17.0, tr("Copy")).clicked();
            (copy, save)
        })
        .inner;
    let job = (copy || save).then(|| render_job(ui, doc)).flatten();
    let joined = match &doc.stage {
        Stage::Done(r) => r.translated_text(),
        _ => String::new(),
    };
    if copy {
        match (as_text, job.clone()) {
            (false, Some(j)) => out.push(MainAction::CopyPicture(j)),
            _ => out.push(MainAction::Copy(joined.clone())),
        }
    }
    if save {
        match (as_text, job) {
            (false, Some(j)) => out.push(MainAction::SavePicture(j)),
            _ => out.push(MainAction::SaveText(joined)),
        }
    }
    // Provider chip sits after the segmented control.
    let mut chip_ui = child(ui, Rect::from_min_max(pos2(seg.response.rect.right() + 8.0, foot.top()), foot.max), Layout::left_to_right(Align::Center));
    provider_chip(&mut chip_ui, st, provider);
}

// ------------------------------------------------------------ rendering

/// One translated block, ready to rasterise (picture pixels).
#[derive(Clone)]
pub struct RenderBlock {
    pub cover: [f32; 4],
    pub bg: Color32,
    pub fg: Color32,
    pub size: f32,
    /// Text of each row, x of its first glyph and its baseline.
    pub rows: Vec<(String, f32, f32)>,
}

/// The translated picture, as data a worker can rasterise.
#[derive(Clone)]
pub struct RenderJob {
    pub image: Arc<RgbaImage>,
    pub blocks: Vec<RenderBlock>,
}

fn render_job(ui: &Ui, doc: &ImageDoc) -> Option<RenderJob> {
    let Stage::Done(r) = &doc.stage else { return None };
    let blocks = r
        .blocks
        .iter()
        .filter_map(|tb| {
            let fit = tb.fit?;
            let k = fit.size / REF_SIZE;
            let g = ui.painter().layout_job(job(&tb.text, REF_SIZE, TEXT, fit.wrap / k * 1.04 + 1.0, fit.center));
            let rows = g
                .rows
                .iter()
                .filter_map(|row| {
                    let first = row.glyphs.first()?;
                    let text = row.text().trim_end().to_owned();
                    (!text.is_empty()).then_some({
                        (
                            text,
                            fit.origin.x + (row.pos.x + first.pos.x) * k,
                            fit.origin.y + (row.pos.y + first.pos.y) * k,
                        )
                    })
                })
                .collect();
            Some(RenderBlock {
                cover: [fit.cover.min.x, fit.cover.min.y, fit.cover.width(), fit.cover.height()],
                bg: tb.bg,
                fg: tb.fg,
                size: fit.size,
                rows,
            })
        })
        .collect();
    Some(RenderJob { image: doc.image.clone(), blocks })
}

fn fontdb() -> Arc<resvg::usvg::fontdb::Database> {
    static DB: OnceLock<Arc<resvg::usvg::fontdb::Database>> = OnceLock::new();
    DB.get_or_init(|| {
        let mut db = resvg::usvg::fontdb::Database::new();
        for bytes in [MONTSERRAT_REGULAR, MONTSERRAT_MEDIUM, MONTSERRAT_SEMIBOLD] {
            db.load_font_data(bytes.to_vec());
        }
        // Fallbacks for scripts Montserrat lacks.
        let dir = std::env::var("WINDIR").unwrap_or_else(|_| r"C:\Windows".into());
        for file in ["segoeui.ttf", "msyh.ttc", "YuGothM.ttc", "malgun.ttf", "Nirmala.ttc"] {
            let _ = db.load_font_file(format!(r"{dir}\Fonts\{file}"));
        }
        Arc::new(db)
    })
    .clone()
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn hex(c: Color32) -> String {
    format!("#{:02x}{:02x}{:02x}", c.r(), c.g(), c.b())
}

/// Draws `svg` over `pixmap`.
fn draw_svg(svg: &str, pixmap: &mut resvg::tiny_skia::Pixmap) -> bool {
    let opts = resvg::usvg::Options { fontdb: fontdb(), font_family: "Montserrat".into(), ..Default::default() };
    match resvg::usvg::Tree::from_str(svg, &opts) {
        Ok(tree) => {
            resvg::render(&tree, resvg::tiny_skia::Transform::identity(), &mut pixmap.as_mut());
            true
        }
        Err(_) => false,
    }
}

fn pixmap_to_image(pixmap: &resvg::tiny_skia::Pixmap) -> RgbaImage {
    let rgba = pixmap
        .pixels()
        .iter()
        .flat_map(|p| {
            let c = p.demultiply();
            [c.red(), c.green(), c.blue(), 255]
        })
        .collect();
    RgbaImage { width: pixmap.width(), height: pixmap.height(), rgba }
}

/// Rasterises the translated picture: block backgrounds, then the text
/// (Montserrat Medium, through resvg).
pub fn render_picture(job: &RenderJob) -> Option<RgbaImage> {
    use resvg::tiny_skia::{IntSize, Paint, Pixmap, Rect as SkRect, Transform};
    let img = &job.image;
    let mut pixmap = Pixmap::from_vec(img.rgba.clone(), IntSize::from_wh(img.width, img.height)?)?;
    for b in &job.blocks {
        let mut paint = Paint::default();
        paint.set_color_rgba8(b.bg.r(), b.bg.g(), b.bg.b(), 255);
        if let Some(r) = SkRect::from_xywh(b.cover[0], b.cover[1], b.cover[2], b.cover[3]) {
            pixmap.fill_rect(r, &paint, Transform::identity(), None);
        }
    }
    let mut svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{}" height="{}" viewBox="0 0 {} {}">"#,
        img.width, img.height, img.width, img.height
    );
    for b in &job.blocks {
        for (text, x, y) in &b.rows {
            svg.push_str(&format!(
                r#"<text x="{x:.2}" y="{y:.2}" font-family="Montserrat" font-weight="500" font-size="{:.2}" fill="{}" xml:space="preserve">{}</text>"#,
                b.size,
                hex(b.fg),
                xml_escape(text)
            ));
        }
    }
    svg.push_str("</svg>");
    draw_svg(&svg, &mut pixmap);
    Some(pixmap_to_image(&pixmap))
}

/// The sign from the Figma frame (OPEN / Mon–Fri 9:00–18:00 / Sale −30%),
/// for `--preview main-image-result`.
pub fn sample_picture() -> Option<RgbaImage> {
    let (w, h) = (800, 520);
    let mut pixmap = resvg::tiny_skia::Pixmap::new(w, h)?;
    let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="800" height="520" viewBox="0 0 400 260">
      <defs><linearGradient id="g" x1="0" y1="0" x2="1" y2="1">
        <stop offset="0" stop-color="#3a4260"/><stop offset="0.714" stop-color="#1b2036"/></linearGradient></defs>
      <rect width="400" height="260" fill="url(#g)"/>
      <rect x="79" y="37" width="242" height="186" rx="6" fill="#d9d4c6"/>
      <text x="200" y="102" text-anchor="middle" font-family="Montserrat" font-weight="600" font-size="40" fill="#26262c">OPEN</text>
      <text x="200" y="146" text-anchor="middle" font-family="Montserrat" font-weight="500" font-size="18" fill="#26262c">Mon–Fri 9:00 – 18:00</text>
      <text x="200" y="192" text-anchor="middle" font-family="Montserrat" font-weight="600" font-size="24" fill="#26262c">Sale −30%</text>
    </svg>"##;
    draw_svg(svg, &mut pixmap).then(|| pixmap_to_image(&pixmap))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// End-to-end OCR on the preview sign (needs the English OCR pack).
    #[test]
    #[ignore = "needs Windows OCR with English installed"]
    fn sample_sign_is_recognised() {
        let img = sample_picture().unwrap();
        let r = crate::win::ocr::recognize(&img, Some("en")).unwrap();
        let texts: Vec<_> = r.blocks.iter().map(|b| b.text.as_str()).collect();
        assert_eq!(texts.len(), 3, "{texts:?}");
        assert_eq!(texts[0], "OPEN");
    }

    #[test]
    fn colours_come_from_the_picture() {
        // Dark text on a light plate.
        let (w, h) = (40u32, 20u32);
        let mut rgba = Vec::new();
        for y in 0..h {
            for x in 0..w {
                let ink = (8..32).contains(&x) && (6..14).contains(&y) && x % 3 == 0;
                rgba.extend_from_slice(if ink { &[20, 20, 30, 255] } else { &[217, 212, 198, 255] });
            }
        }
        let img = RgbaImage { width: w, height: h, rgba };
        let (bg, fg) = block_colors(&img, &Bounds { x: 8.0, y: 6.0, w: 24.0, h: 8.0 });
        assert_eq!(bg, Color32::from_rgb(217, 212, 198));
        assert_eq!(fg, Color32::from_rgb(20, 20, 30));
    }
}