//! "Screen area" overlay: the frozen, dimmed screenshot across all
//! monitors. A toolbar at the top of the monitor under the cursor switches
//! between Area (drag a rectangle), Window (click the window under the
//! cursor) and Entire screen (that monitor at once). Esc or right-click
//! cancels; Tab switches between Area and Window.

use std::sync::Arc;

use egui::{
    Align2, Area, Color32, ColorImage, CursorIcon, Id, Image, ImageSource, Key, Order, Pos2, Rect,
    Sense, Stroke, TextureHandle, TextureOptions, Ui, pos2, vec2,
};

use crate::i18n::tr;
use crate::icons;
use crate::theme::*;
use crate::ui::widgets::key_hint;
use crate::win::image::RgbaImage;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CaptureMode {
    /// Drag a rectangle.
    #[default]
    Area,
    /// Click the window under the cursor.
    Window,
}

pub struct CaptureView {
    pub image: Arc<RgbaImage>,
    /// Where to show the toolbar: centre-top of the monitor under the
    /// cursor, in screenshot pixels.
    pub hint_at: (f32, f32),
    pub mode: CaptureMode,
    /// Top-level windows, topmost first, in screenshot pixels.
    pub windows: Vec<Rect>,
    /// The monitor the overlay opened on, in screenshot pixels ("Entire screen").
    pub monitor: Rect,
    texture: Option<TextureHandle>,
    start: Option<Pos2>,
    current: Option<Pos2>,
}

impl CaptureView {
    pub fn new(image: Arc<RgbaImage>, hint_at: (f32, f32), windows: Vec<Rect>, monitor: Rect) -> Self {
        Self {
            image,
            hint_at,
            mode: CaptureMode::Area,
            windows,
            monitor,
            texture: None,
            start: None,
            current: None,
        }
    }

    /// The topmost window under a point (screenshot pixels).
    fn window_at(&self, p: Pos2) -> Option<Rect> {
        self.windows.iter().find(|r| r.contains(p)).copied()
    }
}

pub enum CaptureAction {
    Cancel,
    /// The selected rectangle in screenshot pixels: x, y, width, height.
    Select(u32, u32, u32, u32),
}

/// Smallest selection (pixels) that counts; smaller drags are ignored.
const MIN_SIDE: f32 = 6.0;

/// A rectangle in screenshot pixels as the action, clamped to the picture.
fn select(r: Rect, img: &RgbaImage) -> Option<CaptureAction> {
    let full = Rect::from_min_size(Pos2::ZERO, vec2(img.width as f32, img.height as f32));
    let r = r.intersect(full);
    if r.width() < MIN_SIDE || r.height() < MIN_SIDE {
        return None;
    }
    let (x, y) = (r.left().round().max(0.0) as u32, r.top().round().max(0.0) as u32);
    let w = (r.width().round() as u32).min(img.width - x);
    let h = (r.height().round() as u32).min(img.height - y);
    Some(CaptureAction::Select(x, y, w, h))
}

/// Dims everything but `s` and outlines it with its size in pixels.
fn highlight(ui: &Ui, screen: Rect, s: Rect, ppp: f32) {
    let p = ui.painter();
    for r in [
        Rect::from_min_max(screen.min, pos2(screen.right(), s.top())),
        Rect::from_min_max(pos2(screen.left(), s.bottom()), screen.max),
        Rect::from_min_max(pos2(screen.left(), s.top()), pos2(s.left(), s.bottom())),
        Rect::from_min_max(pos2(s.right(), s.top()), pos2(screen.right(), s.bottom())),
    ] {
        p.rect_filled(r, 0, CAPTURE_DIM);
    }
    p.rect_stroke(s, 0, Stroke::new(1.5, ACCENT), egui::StrokeKind::Outside);
    let (w, h) = ((s.width() * ppp).round(), (s.height() * ppp).round());
    let g = p.layout_no_wrap(format!("{w} × {h}"), font(12.0, Weight::Medium), TEXT);
    let size = g.size() + vec2(16.0, 8.0);
    let above = s.top() - size.y - 6.0;
    let pos = if above >= screen.top() { pos2(s.left(), above) } else { pos2(s.left() + 6.0, s.top() + 6.0) };
    let pill = Rect::from_min_size(pos, size);
    p.rect(pill, 6, CARD, BORDER_STROKE, egui::StrokeKind::Inside);
    p.galley(pill.min + vec2(8.0, 4.0), g, TEXT);
}

pub fn show(ui: &mut Ui, v: &mut CaptureView) -> Option<CaptureAction> {
    let ctx = ui.ctx().clone();
    let ppp = ctx.pixels_per_point();
    let img = v.image.clone();
    let tex = v
        .texture
        .get_or_insert_with(|| {
            let color = ColorImage::from_rgba_unmultiplied([img.width as usize, img.height as usize], &img.rgba);
            ctx.load_texture("qt-capture", color, TextureOptions::NEAREST)
        })
        .clone();
    let screen = Rect::from_min_size(Pos2::ZERO, vec2(img.width as f32, img.height as f32) / ppp);
    ui.painter().image(tex.id(), screen, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);

    // Toolbar first, so the screen below doesn't react under it.
    let (bar_action, bar_rect) = toolbar(&ctx, v, ppp);
    let over_bar = ctx.pointer_hover_pos().is_some_and(|p| bar_rect.contains(p));

    let resp = ui.interact(screen, ui.id().with("qt-capture"), Sense::click_and_drag());
    if !over_bar {
        ctx.set_cursor_icon(if v.mode == CaptureMode::Area { CursorIcon::Crosshair } else { CursorIcon::PointingHand });
    }
    if ctx.input(|i| i.key_pressed(Key::Tab)) {
        v.mode = if v.mode == CaptureMode::Area { CaptureMode::Window } else { CaptureMode::Area };
        v.start = None;
        v.current = None;
    }

    let mut result = bar_action;
    match v.mode {
        CaptureMode::Area => {
            if resp.drag_started() {
                v.start = resp.interact_pointer_pos();
            }
            if resp.dragged() || resp.drag_stopped() {
                v.current = resp.interact_pointer_pos().or(v.current);
            }
            let sel = match (v.start, v.current) {
                (Some(a), Some(b)) => Some(Rect::from_two_pos(a, b).intersect(screen)),
                _ => None,
            };
            match sel {
                Some(s) => highlight(ui, screen, s, ppp),
                None => {
                    ui.painter().rect_filled(screen, 0, CAPTURE_DIM);
                }
            }
            if resp.drag_stopped() {
                v.start = None;
                v.current = None;
                if let Some(s) = sel {
                    let px = Rect::from_min_max((s.min.to_vec2() * ppp).to_pos2(), (s.max.to_vec2() * ppp).to_pos2());
                    result = result.or_else(|| select(px, &img));
                }
            }
        }
        CaptureMode::Window => {
            let hovered = ctx
                .pointer_hover_pos()
                .filter(|_| !over_bar)
                .and_then(|p| v.window_at((p.to_vec2() * ppp).to_pos2()));
            match hovered {
                Some(px) => {
                    let s = Rect::from_min_max((px.min.to_vec2() / ppp).to_pos2(), (px.max.to_vec2() / ppp).to_pos2())
                        .intersect(screen);
                    highlight(ui, screen, s, ppp);
                    if resp.clicked() {
                        result = result.or_else(|| select(px, &img));
                    }
                }
                None => {
                    ui.painter().rect_filled(screen, 0, CAPTURE_DIM);
                }
            }
        }
    }

    let cancel = ctx.input(|i| i.key_pressed(Key::Escape)) || resp.secondary_clicked();
    if cancel {
        return Some(CaptureAction::Cancel);
    }
    result
}

/// `[Area | Window | Entire screen]  hint  [Esc]` at the top of the monitor.
/// Returns "Entire screen" as a selection, and the toolbar's rect.
fn toolbar(ctx: &egui::Context, v: &mut CaptureView, ppp: f32) -> (Option<CaptureAction>, Rect) {
    let mut action = None;
    let c = pos2(v.hint_at.0 / ppp, v.hint_at.1 / ppp);
    let resp = Area::new(Id::new("qt-capture-toolbar"))
        .order(Order::Foreground)
        .fixed_pos(c)
        .pivot(Align2::CENTER_TOP)
        .constrain(false)
        .show(ctx, |ui| {
            crate::ui::widgets::surface(ui, 12, egui::Margin { left: 6, right: 14, top: 6, bottom: 6 }, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    let modes = [
                        (icons::CROP_WHITE, tr("Area"), Some(CaptureMode::Area)),
                        (icons::WINDOW_WHITE, tr("Window"), Some(CaptureMode::Window)),
                        (icons::MONITOR_WHITE, tr("Entire screen"), None),
                    ];
                    for (icon, label, mode) in modes {
                        let on = mode == Some(v.mode);
                        if segment(ui, icon, label, on).clicked() {
                            match mode {
                                Some(m) => {
                                    v.mode = m;
                                    v.start = None;
                                    v.current = None;
                                }
                                None => action = select(v.monitor, &v.image),
                            }
                        }
                    }
                    ui.add_space(12.0);
                    let hint = match v.mode {
                        CaptureMode::Area => tr("Drag over the text to translate"),
                        CaptureMode::Window => tr("Click the window to translate"),
                    };
                    crate::ui::widgets::text(ui, hint, 13.0, Weight::Medium, TEXT_2);
                    ui.add_space(12.0);
                    key_hint(ui, "Esc", tr("cancel"));
                });
            });
        });
    (action, resp.response.rect)
}

/// One toolbar segment: 15px glyph and 13 Medium label, 32px tall,
/// radius 8; selected = accent on the selected fill.
fn segment(ui: &mut Ui, icon: ImageSource<'static>, label: &str, on: bool) -> egui::Response {
    let color = if on { ACCENT } else { TEXT_2 };
    let galley = ui.painter().layout_no_wrap(label.to_owned(), font(13.0, Weight::Medium), color);
    let size = vec2(10.0 + 15.0 + 7.0 + galley.size().x + 12.0, 32.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    if on {
        ui.painter().rect_filled(rect, 8, SELECTED);
    } else if resp.hovered() {
        ui.painter().rect_filled(rect, 8, HOVER);
    }
    let y = rect.center().y;
    Image::new(icon).tint(color).paint_at(ui, Rect::from_center_size(pos2(rect.left() + 10.0 + 7.5, y), vec2(15.0, 15.0)));
    ui.painter().galley(pos2(rect.left() + 32.0, y - galley.size().y / 2.0), galley, color);
    resp.on_hover_cursor(CursorIcon::PointingHand)
}
