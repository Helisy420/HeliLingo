//! Design tokens from the Figma file ("Quick popup" page) and font setup.

use std::sync::Arc;

use egui::{
    Color32, FontData, FontDefinitions, FontFamily, FontId, Margin, RichText, Shadow, Stroke,
    TextStyle, Vec2,
};

pub const CARD: Color32 = Color32::from_rgb(0x12, 0x18, 0x30);
pub const BORDER: Color32 = Color32::from_rgb(0x24, 0x2d, 0x52);
pub const TEXT: Color32 = Color32::from_rgb(0xe4, 0xe8, 0xf6);
pub const TEXT_2: Color32 = Color32::from_rgb(0xa5, 0xae, 0xcb);
pub const TEXT_3: Color32 = Color32::from_rgb(0x6b, 0x76, 0x99);
pub const ACCENT: Color32 = Color32::from_rgb(0x75, 0x89, 0xdb);
pub const ACCENT_HOVER: Color32 = Color32::from_rgb(0x93, 0xa4, 0xe8);
pub const SKELETON: Color32 = Color32::from_rgb(0x23, 0x2c, 0x4f);
/// Selected menu item / active segment background.
pub const SELECTED: Color32 = Color32::from_rgb(0x22, 0x2b, 0x52);
/// Hovered menu item background.
pub const HOVER: Color32 = Color32::from_rgb(0x1a, 0x22, 0x42);
/// "Selected word" text highlight.
pub const HIGHLIGHT: Color32 = Color32::from_rgb(0x2e, 0x3b, 0x78);
pub const CANVAS: Color32 = Color32::from_rgb(0x0b, 0x0f, 0x1e);
pub const WELL: Color32 = Color32::from_rgb(0x09, 0x0c, 0x18);
pub const SWITCH_OFF_KNOB: Color32 = TEXT_3;
/// Off switch track (`Переключатель / выкл`).
pub const SWITCH_OFF_TRACK: Color32 = Color32::from_rgb(0x23, 0x2c, 0x4f);
/// Window background behind panels (settings, translator, quick, Ultra).
pub const WINDOW_BG: Color32 = CANVAS;
/// Title bars, sidebars and grouped lists inside windows.
pub const PANEL: Color32 = CARD;
/// Text inputs and segmented-control wells.
pub const FIELD: Color32 = Color32::from_rgb(0x0e, 0x13, 0x28);
/// "No key" badge: amber text on a dark amber plate.
pub const WARN: Color32 = Color32::from_rgb(0xe0, 0xb0, 0x60);
pub const WARN_BG: Color32 = Color32::from_rgb(0x2a, 0x25, 0x16);
/// "Available" / "Installed" / "Works": green text on a dark green plate.
pub const OK: Color32 = Color32::from_rgb(0x7c, 0xcb, 0x9e);
pub const OK_BG: Color32 = Color32::from_rgb(0x14, 0x2a, 0x26);

/// Result band of the Quick window.
pub const RESULT_BG: Color32 = Color32::from_rgb(0x15, 0x1d, 0x3a);
/// Ultra mode language colours, most frequent first; the last one marks
/// fragments already in the target language.
pub const ULTRA_LANG: [Color32; 3] = [
    ACCENT,
    Color32::from_rgb(0x52, 0x61, 0x9f),
    Color32::from_rgb(0x3a, 0x46, 0x76),
];
pub const ULTRA_UNCHANGED: Color32 = SWITCH_OFF_TRACK;

/// `0 16 40 rgba(0,0,0,.55)` — the shadow of whole windows.
pub const SHADOW_WINDOW: Shadow = Shadow {
    offset: [0, 16],
    blur: 40,
    spread: 0,
    color: Color32::from_black_alpha(140),
};

/// `0 10 28 rgba(0,0,0,.45)` — the large drop shadow every floating surface uses.
pub const SHADOW_LG: Shadow = Shadow {
    offset: [0, 10],
    blur: 28,
    spread: 0,
    color: Color32::from_black_alpha(115),
};
/// `0 1 3 rgba(0,0,0,.45)` — the tight contact shadow.
pub const SHADOW_SM: Shadow = Shadow {
    offset: [0, 1],
    blur: 3,
    spread: 0,
    color: Color32::from_black_alpha(115),
};

/// Padding of the full popups: `pt 14, pb 12, px 16`.
pub const POPUP_PADDING: Margin = Margin {
    left: 16,
    right: 16,
    top: 14,
    bottom: 12,
};

pub const BORDER_STROKE: Stroke = Stroke {
    width: 1.0,
    color: BORDER,
};

#[derive(Clone, Copy, Debug)]
pub enum Weight {
    Regular,
    Medium,
    SemiBold,
}

const MEDIUM: &str = "montserrat-medium";
const SEMIBOLD: &str = "montserrat-semibold";

pub fn font(size: f32, weight: Weight) -> FontId {
    let family = match weight {
        Weight::Regular => FontFamily::Proportional,
        Weight::Medium => FontFamily::Name(MEDIUM.into()),
        Weight::SemiBold => FontFamily::Name(SEMIBOLD.into()),
    };
    FontId::new(size, family)
}

pub fn rt(text: impl Into<String>, size: f32, weight: Weight, color: Color32) -> RichText {
    RichText::new(text).font(font(size, weight)).color(color)
}

fn font_definitions(fallbacks: &[(String, Arc<FontData>)]) -> FontDefinitions {
    let mut defs = FontDefinitions::default();
    let builtin = defs
        .families
        .get(&FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();

    for (name, bytes) in [
        ("montserrat-regular", MONTSERRAT_REGULAR),
        (MEDIUM, MONTSERRAT_MEDIUM),
        (SEMIBOLD, MONTSERRAT_SEMIBOLD),
    ] {
        defs.font_data
            .insert(name.to_owned(), Arc::new(FontData::from_static(bytes)));
    }

    // Montserrat first, then system fonts for scripts it lacks (CJK, Indic, Thai,
    // arrows), then egui's built-ins (emoji).
    let mut tail: Vec<String> = Vec::new();
    for (name, data) in fallbacks {
        defs.font_data.insert(name.clone(), data.clone());
        tail.push(name.clone());
    }
    tail.extend(builtin);

    let chain = |first: &str| {
        let mut v = vec![first.to_owned()];
        v.extend(tail.iter().cloned());
        v
    };
    defs.families
        .insert(FontFamily::Proportional, chain("montserrat-regular"));
    defs.families
        .insert(FontFamily::Name(MEDIUM.into()), chain(MEDIUM));
    defs.families
        .insert(FontFamily::Name(SEMIBOLD.into()), chain(SEMIBOLD));
    defs
}

fn load_system_fallbacks() -> Vec<(String, Arc<FontData>)> {
    let dir = std::env::var("WINDIR").unwrap_or_else(|_| r"C:\Windows".into());
    [
        "segoeui.ttf",
        "seguisym.ttf",
        "msyh.ttc",
        "YuGothM.ttc",
        "malgun.ttf",
        "Nirmala.ttc",
        "LeelawUI.ttf",
    ]
    .iter()
    .filter_map(|file| {
        let bytes = std::fs::read(format!(r"{dir}\Fonts\{file}")).ok()?;
        Some((format!("sys-{file}"), Arc::new(FontData::from_owned(bytes))))
    })
    .collect()
}

/// Installs Montserrat immediately; system fallback fonts are loaded on a
/// background thread so startup isn't blocked by multi-megabyte CJK fonts.
pub fn install_fonts(ctx: &egui::Context) {
    ctx.set_fonts(font_definitions(&[]));
    let ctx = ctx.clone();
    std::thread::spawn(move || {
        let fallbacks = load_system_fallbacks();
        if !fallbacks.is_empty() {
            ctx.set_fonts(font_definitions(&fallbacks));
            ctx.request_repaint();
        }
    });
}

/// Settings → General → Animations: egui's fades/slides (areas appearing,
/// hover and page transitions) take `ANIMATION_TIME`, or nothing at all.
pub const ANIMATION_TIME: f32 = 0.14;

/// Open animation of windows ("unfold"): progress at `t` seconds after
/// opening, 0 → 1 with a cubic ease-out, or `None` once it's over.
pub fn unfold_progress(t: f32) -> Option<f32> {
    const DURATION: f32 = 0.2;
    let x = t / DURATION;
    if x >= 1.0 {
        return None;
    }
    let x = x.max(0.0);
    Some(1.0 - (1.0 - x).powi(3))
}

pub fn set_animations(ctx: &egui::Context, on: bool) {
    ctx.all_styles_mut(|s| s.animation_time = if on { ANIMATION_TIME } else { 0.0 });
}

pub fn install_style(ctx: &egui::Context) {
    ctx.set_theme(egui::Theme::Dark);
    ctx.all_styles_mut(|s| {
        s.text_styles.insert(TextStyle::Body, font(12.0, Weight::Regular));
        s.text_styles.insert(TextStyle::Button, font(13.0, Weight::Medium));
        s.text_styles.insert(TextStyle::Small, font(11.0, Weight::Regular));
        s.spacing.item_spacing = Vec2::new(8.0, 6.0);
        s.spacing.scroll = egui::style::ScrollStyle::thin();
        s.interaction.selectable_labels = false;
        s.interaction.tooltip_delay = 0.35;

        let v = &mut s.visuals;
        v.panel_fill = Color32::TRANSPARENT;
        v.window_fill = CARD;
        v.window_stroke = BORDER_STROKE;
        v.window_corner_radius = 8.into();
        v.window_shadow = SHADOW_LG;
        v.popup_shadow = SHADOW_SM;
        v.menu_corner_radius = 8.into();
        v.extreme_bg_color = CARD;
        v.override_text_color = Some(TEXT_2);
        v.widgets.inactive.bg_fill = BORDER;
        v.widgets.hovered.bg_fill = TEXT_3;
        v.widgets.active.bg_fill = TEXT_2;
        v.selection.bg_fill = HIGHLIGHT;
        v.selection.stroke = Stroke::new(1.0, TEXT);
        v.text_cursor.stroke = Stroke::new(1.5, ACCENT);
    });
}

/// Montserrat font files (UI fonts; also used to draw translated text into
/// saved pictures).
pub const MONTSERRAT_REGULAR: &[u8] = include_bytes!("../assets/fonts/Montserrat-Regular.ttf");
pub const MONTSERRAT_MEDIUM: &[u8] = include_bytes!("../assets/fonts/Montserrat-Medium.ttf");
pub const MONTSERRAT_SEMIBOLD: &[u8] = include_bytes!("../assets/fonts/Montserrat-SemiBold.ttf");

/// Main window: the source panel uses `PANEL`, the translation panel this.
pub const TRANSLATION_PANEL: Color32 = RESULT_BG;
/// Found-text boxes on the original picture: accent at 12 %.
pub const FOUND_FILL: Color32 = Color32::from_rgba_premultiplied(14, 16, 26, 31);
/// Dimmed screen around the screen-area selection.
pub const CAPTURE_DIM: Color32 = Color32::from_black_alpha(140);

/// `0 14 36 rgba(0,0,0,.55)` — the compact Settings window (Figma 329:2).
pub const SHADOW_WINDOW_COMPACT: Shadow = Shadow {
    offset: [0, 14],
    blur: 36,
    spread: 0,
    color: Color32::from_black_alpha(140),
};
/// Statistics → Languages: stacked-bar colours of the top pairs, then "Other".
pub const STATS_LANG: [Color32; 4] = [ULTRA_LANG[0], ULTRA_LANG[1], ULTRA_LANG[2], SWITCH_OFF_TRACK];
