//! SVG icons exported from the Figma file (assets/icons). Rasterised by
//! egui_extras at the exact size and DPI they are drawn at.

use egui::{ImageSource, include_image};

/// 15px speaker (full popups).
pub const VOL: ImageSource<'static> = include_image!("../assets/icons/vol.svg");
/// 14px speaker (compact tooltip).
pub const VOL_COMPACT: ImageSource<'static> = include_image!("../assets/icons/vol_compact.svg");
pub const COPY: ImageSource<'static> = include_image!("../assets/icons/copy.svg");
pub const EXPAND: ImageSource<'static> = include_image!("../assets/icons/expand.svg");
pub const DOT_LOADING: ImageSource<'static> = include_image!("../assets/icons/dot_loading.svg");
pub const WIFI_OFF: ImageSource<'static> = include_image!("../assets/icons/wifioff.svg");
pub const REFRESH: ImageSource<'static> = include_image!("../assets/icons/refresh.svg");
pub const X: ImageSource<'static> = include_image!("../assets/icons/x.svg");
pub const CHECK: ImageSource<'static> = include_image!("../assets/icons/check.svg");
/// 15px accent "translate" glyph used in the tray menu header.
pub const LANG: ImageSource<'static> = include_image!("../assets/icons/lang.svg");
pub const DOT_ON: ImageSource<'static> = include_image!("../assets/icons/dot_on.svg");
/// 14px chevron-right (tray menu).
pub const CHEVR: ImageSource<'static> = include_image!("../assets/icons/chevr.svg");
/// 16px chevron-right between the "How it works" steps.
pub const CHEVR_STEPS: ImageSource<'static> = include_image!("../assets/icons/chevr_steps.svg");

/// 15px title-bar buttons.
pub const MINUS: ImageSource<'static> = include_image!("../assets/icons/minus.svg");
pub const X15: ImageSource<'static> = include_image!("../assets/icons/x15.svg");
/// 16px settings sidebar icons.
pub const SLIDERS: ImageSource<'static> = include_image!("../assets/icons/sliders.svg");
pub const GLOBE: ImageSource<'static> = include_image!("../assets/icons/globe.svg");
pub const CPU: ImageSource<'static> = include_image!("../assets/icons/cpu.svg");
pub const TEXT_LINES: ImageSource<'static> = include_image!("../assets/icons/text.svg");
pub const KEYBOARD: ImageSource<'static> = include_image!("../assets/icons/kb.svg");
pub const INFO: ImageSource<'static> = include_image!("../assets/icons/info.svg");
/// Accent variants of the sidebar icons for the selected section, and the
/// grey globe (`GLOBE` is the accent one).
pub const SLIDERS_ON: ImageSource<'static> = include_image!("../assets/icons/sliders_on.svg");
pub const GLOBE_DIM: ImageSource<'static> = include_image!("../assets/icons/globe_dim.svg");
pub const CPU_ON: ImageSource<'static> = include_image!("../assets/icons/cpu_on.svg");
pub const TEXT_LINES_ON: ImageSource<'static> = include_image!("../assets/icons/text_on.svg");
pub const KEYBOARD_ON: ImageSource<'static> = include_image!("../assets/icons/kb_on.svg");
pub const INFO_ON: ImageSource<'static> = include_image!("../assets/icons/info_on.svg");
/// 5px green dot of the "Installed" / "Works" badges.
pub const DOT_OK: ImageSource<'static> = include_image!("../assets/icons/dot_ok.svg");
/// 16px drag handle of reorderable rows.
pub const GRIP: ImageSource<'static> = include_image!("../assets/icons/grip.svg");
/// 5px amber dot of the "No key" badge.
pub const DOT_WARN: ImageSource<'static> = include_image!("../assets/icons/dot_warn.svg");
/// 14px info icon of footnotes.
pub const INFO14: ImageSource<'static> = include_image!("../assets/icons/info14.svg");

/// Quick and Ultra windows: 12px chip chevron, 14px swap, 15px close
/// (muted), 17px speaker/copy, 15px Ultra glyph.
pub const CHEV12: ImageSource<'static> = include_image!("../assets/icons/chev12.svg");
pub const SWAP14: ImageSource<'static> = include_image!("../assets/icons/swap14.svg");
pub const X15_MUTED: ImageSource<'static> = include_image!("../assets/icons/x15q.svg");
pub const VOL17: ImageSource<'static> = include_image!("../assets/icons/vol17.svg");
pub const COPY17: ImageSource<'static> = include_image!("../assets/icons/copy17.svg");
pub const ZAP: ImageSource<'static> = include_image!("../assets/icons/zap.svg");

/// 15px clock: History button (no Figma frame; drawn in the icon set's style).
pub const CLOCK: ImageSource<'static> = include_image!("../assets/icons/clock.svg");

/// HeliLingo mark (brand kit 326:405): the sun in a speech bubble, accent
/// bubble with a dark sun. `SUN_MINI` is the simplified 20px version.
pub const SUN_SVG: &[u8] = include_bytes!("../assets/brand/sun_dark.svg");
pub const SUN_MINI_SVG: &[u8] = include_bytes!("../assets/brand/sun_mini32.svg");
/// The mark as an egui image (title bars, About).
pub const SUN: ImageSource<'static> = include_image!("../assets/brand/sun_dark.svg");

/// Tray icon states (brand kit "Значок в трее · 20 px").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrayState {
    Active,
    Paused,
    /// The last translation failed (offline, busy, key rejected).
    Error,
}

/// Renders an SVG into a `size`×`size` pixmap area at `(x, y, w)` (w = drawn width).
fn render_svg(pixmap: &mut resvg::tiny_skia::Pixmap, svg: &[u8], x: f32, y: f32, w: f32) {
    use resvg::tiny_skia::Transform;
    if let Ok(tree) = resvg::usvg::Tree::from_data(svg, &resvg::usvg::Options::default()) {
        let s = w / tree.size().width();
        resvg::render(&tree, Transform::from_row(s, 0.0, 0.0, s, x, y), &mut pixmap.as_mut());
    }
}

fn to_rgba(pixmap: &resvg::tiny_skia::Pixmap) -> Vec<u8> {
    pixmap
        .pixels()
        .iter()
        .flat_map(|p| {
            let c = p.demultiply();
            [c.red(), c.green(), c.blue(), c.alpha()]
        })
        .collect()
}

/// Rounded square path (the icon tile).
fn tile_path(x: f32, w: f32, r: f32) -> Option<resvg::tiny_skia::Path> {
    let mut pb = resvg::tiny_skia::PathBuilder::new();
    let k = r * 0.447_715; // cubic approximation of a quarter circle
    let (a, b) = (x, x + w);
    pb.move_to(a + r, a);
    pb.line_to(b - r, a);
    pb.cubic_to(b - k, a, b, a + k, b, a + r);
    pb.line_to(b, b - r);
    pb.cubic_to(b, b - k, b - k, b, b - r, b);
    pb.line_to(a + r, b);
    pb.cubic_to(a + k, b, a, b - k, a, b - r);
    pb.line_to(a, a + r);
    pb.cubic_to(a, a + k, a + k, a, a + r, a);
    pb.close();
    pb.finish()
}

/// The dark app icon ("Иконка 64"): `#121830` tile with a `#242d52` border,
/// radius ≈ 22% of the side, the mark at 62.5%; the simplified mark below
/// 48px. Straight (non-premultiplied) RGBA, `size`×`size`.
pub fn app_icon_rgba(size: u32) -> Vec<u8> {
    use resvg::tiny_skia::{FillRule, Paint, Pixmap, Stroke, Transform};

    let mut pixmap = Pixmap::new(size, size).expect("non-zero icon size");
    let side = size as f32;
    if let Some(path) = tile_path(0.5, side - 1.0, side * 0.22) {
        let mut paint = Paint { anti_alias: true, ..Default::default() };
        paint.set_color_rgba8(0x12, 0x18, 0x30, 0xff);
        pixmap.fill_path(&path, &paint, FillRule::Winding, Transform::identity(), None);
        paint.set_color_rgba8(0x24, 0x2d, 0x52, 0xff);
        let stroke = Stroke { width: 1.0, ..Default::default() };
        pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
    }
    let mark = side * 0.625;
    let svg = if size >= 48 { SUN_SVG } else { SUN_MINI_SVG };
    render_svg(&mut pixmap, svg, (side - mark) / 2.0, (side - mark) / 2.0, mark);
    to_rgba(&pixmap)
}

/// The tray icon for a state, `size`×`size` (the 20px mark in a 28px box,
/// with the red dot in the corner for errors).
pub fn tray_icon_rgba(state: TrayState, size: u32) -> Vec<u8> {
    let mut pixmap = resvg::tiny_skia::Pixmap::new(size, size).expect("non-zero icon size");
    let side = size as f32;
    match state {
        TrayState::Error => render_svg(&mut pixmap, include_bytes!("../assets/brand/tray_error.svg"), 0.0, 0.0, side),
        _ => {
            let svg: &[u8] = if state == TrayState::Paused {
                include_bytes!("../assets/brand/tray_paused.svg")
            } else {
                include_bytes!("../assets/brand/tray_active.svg")
            };
            // 20 of 28: same mark size as the error variant.
            let w = side * 20.0 / 28.0;
            render_svg(&mut pixmap, svg, (side - w) / 2.0, (side - w) / 2.0, w);
        }
    }
    to_rgba(&pixmap)
}

// Main translator window (Figma 315:2, 315:127, 315:244).
/// 15px title-bar gear (opens Settings).
pub const GEAR: ImageSource<'static> = include_image!("../assets/icons/gear.svg");
/// 16px mode pills: accent (active) and muted variants.
pub const MODE_TEXT_ON: ImageSource<'static> = include_image!("../assets/icons/mode_lang.svg");
pub const MODE_TEXT_OFF: ImageSource<'static> = include_image!("../assets/icons/mode_lang_off.svg");
pub const MODE_IMAGE_ON: ImageSource<'static> = include_image!("../assets/icons/mode_image_on.svg");
pub const MODE_IMAGE_OFF: ImageSource<'static> = include_image!("../assets/icons/mode_image.svg");
/// 16px language-strip chevron and the swap button.
pub const CHEV16: ImageSource<'static> = include_image!("../assets/icons/chev16.svg");
pub const SWAP: ImageSource<'static> = include_image!("../assets/icons/swap.svg");
/// 16px clear button of the source panel.
pub const X16: ImageSource<'static> = include_image!("../assets/icons/x16.svg");
/// 16px favourite star: outline and filled (accent).
pub const STAR: ImageSource<'static> = include_image!("../assets/icons/star.svg");
pub const STAR_ON: ImageSource<'static> = include_image!("../assets/icons/star_on.svg");
/// Images tab: 24px drop-zone glyph, 15px "Choose file" (dark) and
/// "Screen area" glyphs, 14px OCR chip glyph, 6px status dot, 17px save.
pub const IMAGE24: ImageSource<'static> = include_image!("../assets/icons/image24.svg");
pub const IMAGE15: ImageSource<'static> = include_image!("../assets/icons/image15.svg");
pub const CROP: ImageSource<'static> = include_image!("../assets/icons/crop.svg");
pub const LANG14: ImageSource<'static> = include_image!("../assets/icons/lang14.svg");
pub const DOT_OK6: ImageSource<'static> = include_image!("../assets/icons/dot_ok6.svg");
pub const DOWNLOAD: ImageSource<'static> = include_image!("../assets/icons/dl17.svg");

// Compact Settings window (Figma 329:2 … 329:838).
/// 14px Statistics sidebar icon: muted and accent (selected).
pub const CHART: ImageSource<'static> = include_image!("../assets/icons/chart.svg");
pub const CHART_ON: ImageSource<'static> = include_image!("../assets/icons/chart_on.svg");
/// 12px lock (key fields, "Stored only on this PC") and external-link glyph.
pub const LOCK12: ImageSource<'static> = include_image!("../assets/icons/lock12.svg");
pub const EXTERNAL12: ImageSource<'static> = include_image!("../assets/icons/ext12.svg");
/// `Переключатель / вкл`, 32×18 (accent track, dark knob).
pub const SWITCH_ON_SM: ImageSource<'static> = include_image!("../assets/icons/switch_on18.svg");

// Translation windows: web links and capture modes (drawn in the icon
// set's style: 24-unit grid, round caps, 1.7px at the drawn size).
/// 17px "Open in Google Translate" and Wikipedia "W" (main window, Quick).
pub const EXTERNAL17: ImageSource<'static> = include_image!("../assets/icons/ext17.svg");
pub const WIKI17: ImageSource<'static> = include_image!("../assets/icons/wiki17.svg");
/// 15px muted variants for the popup header, and the accent W of the
/// Wikipedia card.
pub const EXTERNAL15: ImageSource<'static> = include_image!("../assets/icons/ext15.svg");
pub const WIKI15: ImageSource<'static> = include_image!("../assets/icons/wiki15.svg");
pub const WIKI15_ON: ImageSource<'static> = include_image!("../assets/icons/wiki15_on.svg");
/// 15px "Whole screen" button glyph.
pub const MONITOR15: ImageSource<'static> = include_image!("../assets/icons/monitor15.svg");
/// White 15px capture-overlay toolbar glyphs, tinted where drawn.
pub const CROP_WHITE: ImageSource<'static> = include_image!("../assets/icons/crop_w.svg");
pub const WINDOW_WHITE: ImageSource<'static> = include_image!("../assets/icons/window_w.svg");
pub const MONITOR_WHITE: ImageSource<'static> = include_image!("../assets/icons/monitor_w.svg");
