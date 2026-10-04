//! User preferences, persisted as JSON in %APPDATA%\HeliLingo.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::i18n::{tr, trf};
pub use crate::win::secret::Secret;

/// A key chord such as Ctrl + Shift + T. `vk` is a Win32 virtual-key code.
///
/// `double`: the last key is tapped twice while the modifiers are held
/// ("Ctrl + Alt + A ×2"). A lone modifier with `double` (vk = VK_CONTROL,
/// VK_MENU or VK_SHIFT, no modifier flags) is a modifier double tap
/// ("Shift ×2").
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Combo {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub win: bool,
    pub vk: u32,
    #[serde(default)]
    pub double: bool,
}

/// Generic virtual-key codes of the modifiers that can be double-tapped.
pub const VK_SHIFT: u32 = 0x10;
pub const VK_CONTROL: u32 = 0x11;
pub const VK_MENU: u32 = 0x12;
/// Mouse side buttons ("Mouse 4" = back, "Mouse 5" = forward). They bind
/// like keys: alone, with modifiers, or ×2.
pub const VK_XBUTTON1: u32 = 0x05;
pub const VK_XBUTTON2: u32 = 0x06;

pub fn is_mouse_button(vk: u32) -> bool {
    matches!(vk, VK_XBUTTON1 | VK_XBUTTON2)
}

impl Combo {
    /// A double tap of a lone modifier (`VK_CONTROL`, `VK_MENU`, `VK_SHIFT`).
    pub const fn modifier_tap(vk: u32) -> Combo {
        Combo { ctrl: false, shift: false, alt: false, win: false, vk, double: true }
    }

    pub fn is_modifier_tap(&self) -> bool {
        self.double && matches!(self.vk, VK_SHIFT | VK_CONTROL | VK_MENU) && !(self.ctrl || self.shift || self.alt || self.win)
    }

    pub const fn ctrl_alt(vk: u32) -> Combo {
        Combo { ctrl: true, shift: false, alt: true, win: false, vk, double: false }
    }

    /// Key caps without the "×2" ("Ctrl", "Alt", "A"); see [`Combo::label`].
    pub fn keys(&self) -> Vec<String> {
        if self.is_modifier_tap() {
            return vec![modifier_name(self.vk).to_owned()];
        }
        let mut keys = Vec::new();
        if self.ctrl {
            keys.push("Ctrl".to_owned());
        }
        if self.shift {
            keys.push("Shift".to_owned());
        }
        if self.alt {
            keys.push("Alt".to_owned());
        }
        if self.win {
            keys.push("Win".to_owned());
        }
        keys.push(crate::win::input::vk_name(self.vk));
        keys
    }

    /// "Ctrl + Alt + A", "Ctrl + Alt + A ×2", "Shift ×2".
    pub fn label(&self) -> String {
        let keys = self.keys().join(" + ");
        if self.double { format!("{keys} ×2") } else { keys }
    }
}

fn modifier_name(vk: u32) -> &'static str {
    match vk {
        VK_CONTROL => "Ctrl",
        VK_MENU => "Alt",
        _ => "Shift",
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Shortcut {
    DoubleCtrl,
    DoubleAlt,
    DoubleShift,
    Combo(Combo),
}

impl Shortcut {
    pub fn label(&self) -> String {
        match self {
            Shortcut::DoubleCtrl => tr("Double-tap Ctrl").to_owned(),
            Shortcut::DoubleAlt => tr("Double-tap Alt").to_owned(),
            Shortcut::DoubleShift => tr("Double-tap Shift").to_owned(),
            Shortcut::Combo(c) => c.label(),
        }
    }

    /// A recorded binding for the main shortcut: modifier double taps map
    /// to their variants, everything else is a chord.
    pub fn from_combo(c: Combo) -> Shortcut {
        if c.is_modifier_tap() {
            match c.vk {
                VK_CONTROL => return Shortcut::DoubleCtrl,
                VK_MENU => return Shortcut::DoubleAlt,
                _ => return Shortcut::DoubleShift,
            }
        }
        Shortcut::Combo(c)
    }

    /// The same binding as a [`Combo`] (modifier double taps included), for
    /// matching and conflict checks.
    pub fn as_combo(&self) -> Combo {
        match self {
            Shortcut::DoubleCtrl => Combo::modifier_tap(VK_CONTROL),
            Shortcut::DoubleAlt => Combo::modifier_tap(VK_MENU),
            Shortcut::DoubleShift => Combo::modifier_tap(VK_SHIFT),
            Shortcut::Combo(c) => *c,
        }
    }

    /// Key caps shown in the tray menu, e.g. `[Ctrl] [Ctrl]`.
    pub fn keys(&self) -> Vec<String> {
        match self {
            Shortcut::DoubleCtrl => vec!["Ctrl".into(), "Ctrl".into()],
            Shortcut::DoubleAlt => vec!["Alt".into(), "Alt".into()],
            Shortcut::DoubleShift => vec!["Shift".into(), "Shift".into()],
            Shortcut::Combo(c) if c.double => {
                let mut k = c.keys();
                k.push("×2".into());
                k
            }
            Shortcut::Combo(c) => c.keys(),
        }
    }

    pub fn is_double_tap(&self) -> bool {
        matches!(self, Shortcut::DoubleCtrl | Shortcut::DoubleAlt | Shortcut::DoubleShift)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PopupStyle {
    Compact,
    Full,
}

/// Translation services, tried in the user's order (Settings → Providers).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ProviderKind {
    /// Public `translate_a/single` endpoint, or Cloud Translation with a key.
    Google,
    /// Yandex Cloud Translate (API key, optional folder id).
    Yandex,
    /// Microsoft Translator with an Azure key + region. (The keyless Edge
    /// token endpoint the engine can also use stopped working in 2026, so
    /// Bing counts as usable only with a key.)
    Bing,
    /// DeepL API (Free keys end in ":fx").
    DeepL,
    /// Offline tier 2 ("super-fast"): Argos Translate packages, pivoting
    /// through English. Usable once installed (see `crate::offline`).
    Argos,
    // The other offline tiers (`crate::offline::Tier`). Separate unit
    // variants keep settings.json plain strings, as "Argos" always was.
    /// Offline tier 1 ("super-mega-fast"): OPUS-MT per language pair.
    OpusMt,
    /// Offline tier 3 ("fast"): NLLB-200 distilled 600M.
    Nllb600,
    /// Offline tier 4 ("normal"): NLLB-200 distilled 1.3B.
    Nllb13,
    /// Offline tier 5 ("medium, with context"): TranslateGemma 4B.
    Gemma4,
    /// Offline tier 6 ("heavy, with context"): TranslateGemma 12B.
    Gemma12,
}

impl ProviderKind {
    pub const ALL: [ProviderKind; 10] = [
        ProviderKind::Google,
        ProviderKind::Yandex,
        ProviderKind::Bing,
        ProviderKind::DeepL,
        ProviderKind::OpusMt,
        ProviderKind::Argos,
        ProviderKind::Nllb600,
        ProviderKind::Nllb13,
        ProviderKind::Gemma4,
        ProviderKind::Gemma12,
    ];

    /// Short name for chips ("Google", "DeepL").
    pub fn short_name(self) -> &'static str {
        match self {
            ProviderKind::Google => "Google",
            ProviderKind::Yandex => "Yandex",
            ProviderKind::Bing => "Bing",
            ProviderKind::DeepL => "DeepL",
            ProviderKind::Argos => "Argos",
            ProviderKind::OpusMt => "OPUS-MT",
            ProviderKind::Nllb600 => "NLLB 600M",
            ProviderKind::Nllb13 => "NLLB 1.3B",
            ProviderKind::Gemma4 => "Gemma 4B",
            ProviderKind::Gemma12 => "Gemma 12B",
        }
    }

    /// The offline tier this provider is, if any.
    pub fn tier(self) -> Option<crate::offline::Tier> {
        crate::offline::Tier::ALL.into_iter().find(|t| t.kind() == self)
    }

    pub fn is_offline(self) -> bool {
        self.tier().is_some()
    }

    /// `--provider` names: the short name, the tier id (`opus`, `argos`,
    /// `nllb600`, `nllb13`, `gemma4`, `gemma12`) or the tier number 1–6.
    pub fn from_cli(name: &str) -> Option<ProviderKind> {
        let n = name.trim();
        ProviderKind::ALL
            .into_iter()
            .find(|k| k.short_name().eq_ignore_ascii_case(n) || k.short_name().replace(' ', "").eq_ignore_ascii_case(n))
            .or_else(|| crate::offline::Tier::from_id(n).map(|t| t.kind()))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderEntry {
    pub kind: ProviderKind,
    pub enabled: bool,
}

/// API keys and their companion settings. Keys are DPAPI-encrypted in
/// settings.json (see [`Secret`]).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ProviderKeys {
    /// Optional Google Cloud Translation API key.
    pub google: Secret,
    /// Yandex Cloud Translate API key.
    pub yandex: Secret,
    /// Yandex Cloud folder id (needed for user IAM keys, optional for
    /// service-account API keys).
    pub yandex_folder: String,
    /// Optional Azure Translator key; without it the keyless Edge endpoint is used.
    pub azure: Secret,
    /// Azure resource region, e.g. "westeurope" (empty = global resource).
    pub azure_region: String,
    /// DeepL API key. `DEEPL_AUTH_KEY` is used when this is empty.
    pub deepl: Secret,
}

impl ProviderKeys {
    pub fn deepl_key(&self) -> Option<String> {
        if self.deepl.is_set() {
            return Some(self.deepl.expose().to_owned());
        }
        std::env::var("DEEPL_AUTH_KEY").ok().filter(|k| !k.trim().is_empty())
    }

    /// Whether `kind` can run with these keys (all but Google need one).
    pub fn usable(&self, kind: ProviderKind) -> bool {
        match kind {
            // Keyless by default; a key switches to the paid cloud API.
            ProviderKind::Google | ProviderKind::Yandex => true,
            ProviderKind::Bing => self.azure.is_set(),
            ProviderKind::DeepL => self.deepl_key().is_some(),
            // Offline tiers need no key; whether they are installed is
            // checked against the model folder (`EngineConfig::chain`).
            k => k.is_offline(),
        }
    }
}

/// Where offline models run (Settings → Offline & acceleration → Device).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum OfflineDevice {
    /// The graphics card when it can hold the model, else the processor.
    #[default]
    Auto,
    Cpu,
    Gpu,
}

/// Numeric precision of the CTranslate2 tiers (Settings → Offline &
/// acceleration → Precision). llama.cpp tiers use their GGUF quantisation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum OfflinePrecision {
    /// int8 on the processor, int8 + float16 on the graphics card.
    #[default]
    Auto,
    /// int8: fastest, least memory.
    Int8,
    /// int8 weights, float16 maths (graphics card only).
    Int8Float16,
    /// float32: slowest, the reference quality.
    Float32,
}

pub const CONTEXT_FRAGMENTS_DEFAULT: u32 = 5;
/// Range of the "Fragments to remember" slider.
pub const CONTEXT_FRAGMENTS_RANGE: std::ops::RangeInclusive<u32> = 1..=10;

/// How the keyless Google provider reaches Google (Settings → Providers →
/// Google). Google rate-limits its public endpoints per IP; the web mode
/// rotates through alternate endpoints when one answers 429/503.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum GoogleMode {
    /// `translate_a/single` first, alternate web endpoints when it's busy.
    Auto,
    /// Only `translate_a/single` (with dictionary data).
    Api,
    /// Web endpoints first (dict-chrome-ex, the mobile page), then the API.
    Web,
}

/// What Ctrl+C does while the double-Ctrl popup is open.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PopupCtrlC {
    /// Copy the translation and close the popup.
    CopyAndClose,
    /// Copy the translation, keep the popup.
    CopyOnly,
    /// Leave Ctrl+C to the app underneath.
    Ignore,
}

/// Which global shortcut a recorded key chord is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HotkeySlot {
    /// The main shortcut (`Settings::shortcut`): translate the selection.
    Translate,
    Quick,
    Ultra,
    ScreenArea,
    /// Translate the whole monitor under the cursor.
    WholeScreen,
    MainWindow,
    /// Translate the selection and paste the translation over it, with no
    /// window.
    TranslatePaste,
    /// Ctrl+C pressed twice (DeepL style): translate what was copied. Not a
    /// recordable slot; on/off in `Settings::copy_twice`.
    CopyTwice,
}

/// Shortcuts besides the main one; each is a chord, `None` = off.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Hotkeys {
    /// Quick translation window (Ctrl+Alt+T).
    pub quick: Option<Combo>,
    /// Ultra mode on the selection (Ctrl+Alt+U).
    pub ultra: Option<Combo>,
    /// Translate a screen area (Ctrl+Shift+S).
    pub screen_area: Option<Combo>,
    /// Translate the whole monitor under the cursor (Ctrl+Shift+A).
    pub whole_screen: Option<Combo>,
    /// Open the main translator window (Ctrl+Alt+Q).
    pub main_window: Option<Combo>,
    /// Translate the selection and paste it in place, no window (Ctrl+Alt+R).
    pub translate_paste: Option<Combo>,
}

impl Default for Hotkeys {
    fn default() -> Self {
        Self {
            quick: Some(Combo::ctrl_alt(0x54)),
            ultra: Some(Combo::ctrl_alt(0x55)),
            screen_area: Some(Combo { ctrl: true, shift: true, vk: 0x53, ..Default::default() }),
            whole_screen: Some(Combo { ctrl: true, shift: true, vk: 0x41, ..Default::default() }),
            main_window: Some(Combo::ctrl_alt(0x51)),
            translate_paste: Some(Combo::ctrl_alt(0x52)),
        }
    }
}

impl Hotkeys {
    pub fn get(&self, slot: HotkeySlot) -> Option<Combo> {
        match slot {
            HotkeySlot::Translate | HotkeySlot::CopyTwice => None,
            HotkeySlot::Quick => self.quick,
            HotkeySlot::Ultra => self.ultra,
            HotkeySlot::ScreenArea => self.screen_area,
            HotkeySlot::WholeScreen => self.whole_screen,
            HotkeySlot::MainWindow => self.main_window,
            HotkeySlot::TranslatePaste => self.translate_paste,
        }
    }

    pub fn set(&mut self, slot: HotkeySlot, combo: Option<Combo>) {
        match slot {
            HotkeySlot::Translate | HotkeySlot::CopyTwice => {}
            HotkeySlot::Quick => self.quick = combo,
            HotkeySlot::Ultra => self.ultra = combo,
            HotkeySlot::ScreenArea => self.screen_area = combo,
            HotkeySlot::WholeScreen => self.whole_screen = combo,
            HotkeySlot::MainWindow => self.main_window = combo,
            HotkeySlot::TranslatePaste => self.translate_paste = combo,
        }
    }
}

/// Compact popup: translations from the other providers in the chain.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompactVariants {
    /// The default: compact shows just the translation.
    #[default]
    Off,
    /// Always listed under the translation.
    List,
    /// Behind a small "▾ 3" button.
    Menu,
}

/// At most this many other providers' translations in the compact popup.
pub const COMPACT_VARIANTS_MAX: usize = 3;

/// What to do when the selection is already in the target language.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AlreadyInTarget {
    /// Translate to English instead (to Russian when the target is English).
    TranslateBack,
    /// Show it as is.
    Keep,
}

/// Settings → Context & modes → Ultra mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct UltraOptions {
    /// Detect the language of every fragment (else: one language for all).
    pub detect_each: bool,
    /// Leave fragments already in the target language unchanged.
    pub skip_target: bool,
    /// Show the Ultra window; when off, the result replaces the selection.
    pub show_window: bool,
}

impl Default for UltraOptions {
    fn default() -> Self {
        Self { detect_each: true, skip_target: true, show_window: true }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Interface language: "ru" or "en".
    pub ui_lang: String,
    pub shortcut: Shortcut,
    /// Target language code, e.g. "ru".
    pub target: String,
    /// Source language code or "auto".
    pub source: String,
    pub already_in_target: AlreadyInTarget,
    pub popup_style: PopupStyle,
    /// Seconds before the popup hides itself; 0 = never.
    pub hide_after_secs: u32,
    /// Speak every translation as it arrives (popup and quick window).
    pub speak_results: bool,
    /// Keep translations in history.json (see `history`).
    pub save_history: bool,
    pub start_with_windows: bool,
    pub onboarded: bool,
    /// Providers in the order they are tried; disabled ones are skipped.
    pub providers: Vec<ProviderEntry>,
    pub keys: ProviderKeys,
    pub hotkeys: Hotkeys,
    /// Max time between the two presses of a double tap, in ms.
    pub double_tap_ms: u32,
    /// Executable names (e.g. "game.exe") where all shortcuts are ignored.
    pub ignored_apps: Vec<String>,
    /// Ignore shortcuts while a fullscreen app is in the foreground.
    pub ignore_fullscreen: bool,
    pub ultra: UltraOptions,
    pub google_mode: GoogleMode,
    pub popup_ctrl_c: PopupCtrlC,
    /// Ctrl+C twice in a row translates what was copied (like DeepL).
    pub copy_twice: bool,
    /// Other providers' translations in the compact popup.
    pub compact_variants: CompactVariants,
    /// Fade/slide animations for popups, windows and page switches.
    pub animations: bool,
    /// OpenGL rendering; off = software rendering. Applies on restart.
    pub hardware_acceleration: bool,
    /// Count usage locally for Settings → Statistics (see `stats`).
    pub keep_stats: bool,
    /// Translate only string literals and comments when the text is code.
    pub programmer_mode: bool,
    /// Wikipedia button in popups and translator windows.
    pub show_wiki: bool,
    /// Offline models (Settings → Offline & acceleration): where they run.
    pub offline_device: OfflineDevice,
    /// CPU threads for offline models; 0 = automatic (physical cores).
    pub offline_threads: u32,
    /// Numeric precision of the CTranslate2 tiers.
    pub offline_precision: OfflinePrecision,
    /// Folder for downloaded models; empty = %LOCALAPPDATA%\HeliLingo\Models.
    /// Which tiers are installed is read from this folder, not stored.
    pub model_dir: String,
    /// Settings → Providers → "Offline only": the chain uses only the
    /// installed offline tiers.
    pub offline_only: bool,
    /// Settings → Context & modes: give context tiers (and DeepL) the
    /// previous fragments and their translations.
    pub use_context: bool,
    /// How many previous fragments the context keeps (1..=10).
    pub context_fragments: u32,
    /// Size of the Settings window card (it can be resized).
    pub settings_size: Option<[f32; 2]>,
    /// Plain-text DeepL key from older versions; moved into `keys.deepl`
    /// (encrypted) on load and never written back.
    #[serde(skip_serializing)]
    deepl_api_key: Option<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            ui_lang: "ru".into(),
            shortcut: Shortcut::DoubleCtrl,
            target: "ru".into(),
            source: "auto".into(),
            already_in_target: AlreadyInTarget::TranslateBack,
            popup_style: PopupStyle::Compact,
            hide_after_secs: 4,
            speak_results: false,
            save_history: true,
            start_with_windows: false,
            onboarded: false,
            providers: default_providers(),
            keys: ProviderKeys::default(),
            hotkeys: Hotkeys::default(),
            double_tap_ms: DOUBLE_TAP_DEFAULT_MS,
            ignored_apps: Vec::new(),
            ignore_fullscreen: true,
            ultra: UltraOptions::default(),
            google_mode: GoogleMode::Auto,
            popup_ctrl_c: PopupCtrlC::CopyAndClose,
            copy_twice: true,
            compact_variants: CompactVariants::Off,
            animations: true,
            hardware_acceleration: true,
            keep_stats: true,
            programmer_mode: false,
            show_wiki: true,
            offline_device: OfflineDevice::Auto,
            offline_threads: 0,
            offline_precision: OfflinePrecision::Auto,
            model_dir: String::new(),
            offline_only: false,
            use_context: true,
            context_fragments: CONTEXT_FRAGMENTS_DEFAULT,
            settings_size: None,
            deepl_api_key: None,
        }
    }
}

pub const DOUBLE_TAP_DEFAULT_MS: u32 = 400;
/// Range of the "Interval between presses" slider.
pub const DOUBLE_TAP_RANGE: std::ops::RangeInclusive<u32> = 200..=800;

fn default_providers() -> Vec<ProviderEntry> {
    ProviderKind::ALL
        .iter()
        .map(|&kind| ProviderEntry { kind, enabled: kind != ProviderKind::Argos })
        .collect()
}

/// %APPDATA%\HeliLingo: settings, history, statistics. The folder of the
/// app's old name (QuickTranslate) is renamed on first use.
pub fn data_dir() -> Option<PathBuf> {
    let base = PathBuf::from(std::env::var_os("APPDATA")?);
    let dir = base.join("HeliLingo");
    let old = base.join("QuickTranslate");
    if !dir.exists() && old.exists() {
        let _ = std::fs::rename(&old, &dir);
    }
    Some(dir)
}

fn path() -> Option<PathBuf> {
    Some(data_dir()?.join("settings.json"))
}

/// Preview instances (`--preview`) run next to the real app and must never
/// write its files: their settings copy may be older.
static READ_ONLY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn set_read_only() {
    READ_ONLY.store(true, std::sync::atomic::Ordering::Relaxed);
}

pub fn read_only() -> bool {
    READ_ONLY.load(std::sync::atomic::Ordering::Relaxed)
}

impl Settings {
    pub fn load() -> Self {
        let mut s: Settings = path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|json| serde_json::from_str(&json).ok())
            .unwrap_or_default();
        // The registry is the source of truth for autostart.
        s.start_with_windows = crate::win::autostart_enabled();
        s.normalize();
        if let Some(old) = s.deepl_api_key.take() {
            if !s.keys.deepl.is_set() && !old.trim().is_empty() {
                s.keys.deepl = Secret::new(old);
            }
            // Rewrite the file now so the plain-text key is gone from disk.
            s.save();
        }
        s
    }

    /// Every provider exactly once (in the saved order), values in range.
    fn normalize(&mut self) {
        let mut seen = Vec::new();
        self.providers.retain(|p| {
            let new = !seen.contains(&p.kind);
            seen.push(p.kind);
            new
        });
        for d in default_providers() {
            if !seen.contains(&d.kind) {
                self.providers.push(d);
            }
        }
        self.double_tap_ms = self
            .double_tap_ms
            .clamp(*DOUBLE_TAP_RANGE.start(), *DOUBLE_TAP_RANGE.end());
    }

    pub fn save(&self) {
        if read_only() {
            return;
        }
        let Some(p) = path() else { return };
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(p, json);
        }
    }

    /// Is the executable name (any case, with or without ".exe") ignored?
    pub fn is_ignored_app(&self, exe: &str) -> bool {
        let norm = |s: &str| s.trim().to_lowercase().trim_end_matches(".exe").to_owned();
        let exe = norm(exe);
        !exe.is_empty() && self.ignored_apps.iter().any(|a| norm(a) == exe)
    }
}

pub const HIDE_AFTER_OPTIONS: &[u32] = &[2, 4, 8, 15, 0];

pub fn hide_after_label(secs: u32) -> String {
    if secs == 0 {
        tr("Never").to_owned()
    } else {
        trf("{n} s", &[("n", &secs.to_string())])
    }
}

/// Languages offered in the pickers (Google/DeepL codes): code, English
/// name, Russian name.
pub const LANGUAGES: &[(&str, &str, &str)] = &[
    ("en", "English", "Английский"),
    ("ru", "Russian", "Русский"),
    ("uk", "Ukrainian", "Украинский"),
    ("de", "German", "Немецкий"),
    ("fr", "French", "Французский"),
    ("es", "Spanish", "Испанский"),
    ("it", "Italian", "Итальянский"),
    ("pt", "Portuguese", "Португальский"),
    ("nl", "Dutch", "Нидерландский"),
    ("pl", "Polish", "Польский"),
    ("cs", "Czech", "Чешский"),
    ("sk", "Slovak", "Словацкий"),
    ("sv", "Swedish", "Шведский"),
    ("da", "Danish", "Датский"),
    ("no", "Norwegian", "Норвежский"),
    ("fi", "Finnish", "Финский"),
    ("et", "Estonian", "Эстонский"),
    ("lv", "Latvian", "Латышский"),
    ("lt", "Lithuanian", "Литовский"),
    ("hu", "Hungarian", "Венгерский"),
    ("ro", "Romanian", "Румынский"),
    ("bg", "Bulgarian", "Болгарский"),
    ("el", "Greek", "Греческий"),
    ("tr", "Turkish", "Турецкий"),
    ("be", "Belarusian", "Белорусский"),
    ("kk", "Kazakh", "Казахский"),
    ("ka", "Georgian", "Грузинский"),
    ("hy", "Armenian", "Армянский"),
    ("he", "Hebrew", "Иврит"),
    ("ar", "Arabic", "Арабский"),
    ("fa", "Persian", "Персидский"),
    ("hi", "Hindi", "Хинди"),
    ("th", "Thai", "Тайский"),
    ("vi", "Vietnamese", "Вьетнамский"),
    ("id", "Indonesian", "Индонезийский"),
    ("ja", "Japanese", "Японский"),
    ("ko", "Korean", "Корейский"),
    ("zh-CN", "Chinese", "Китайский"),
];

/// Search in a language list: case-insensitive match on the English or
/// Russian name, or the code ("ger", "нем", "de" all find German). An empty
/// query matches all.
pub fn lang_matches(code: &str, names: &[&str], query: &str) -> bool {
    let q = query.trim().to_lowercase();
    q.is_empty()
        || code.to_lowercase().starts_with(&q)
        || names.iter().any(|n| n.to_lowercase().contains(&q))
}

/// `LANGUAGES` filtered by a search query, in list order: (code, name in
/// the interface language).
pub fn search_languages(query: &str) -> Vec<(&'static str, &'static str)> {
    LANGUAGES
        .iter()
        .filter(|(c, en, ru)| lang_matches(c, &[en, ru], query))
        .map(|&(c, en, ru)| (c, if crate::i18n::is_russian() { ru } else { en }))
        .collect()
}

fn find_lang(code: &str) -> Option<&'static (&'static str, &'static str, &'static str)> {
    LANGUAGES
        .iter()
        .find(|(c, _, _)| c.eq_ignore_ascii_case(code))
        .or_else(|| LANGUAGES.iter().find(|(c, _, _)| c.split('-').next() == Some(code)))
}

/// Language name in the interface language ("Russian" / "Русский").
pub fn lang_name(code: &str) -> String {
    if code == "auto" {
        return tr("Auto-detect").to_owned();
    }
    find_lang(code)
        .map(|&(_, en, ru)| if crate::i18n::is_russian() { ru } else { en }.to_owned())
        .unwrap_or_else(|| code.to_uppercase())
}

/// Short badge for a language code: "en" → "EN", "zh-CN" → "ZH".
pub fn lang_badge(code: &str) -> String {
    code.split('-').next().unwrap_or(code).to_uppercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn language_search() {
        assert!(lang_matches("de", &["German", "Немецкий"], "GER"));
        assert!(lang_matches("de", &["German", "Немецкий"], "нем"));
        assert!(lang_matches("zh-CN", &["Chinese", "Китайский"], "zh"));
        assert!(!lang_matches("de", &["German", "Немецкий"], "fr"));
    }

    #[test]
    fn ignored_apps_match_loosely() {
        let s = Settings { ignored_apps: vec!["Game.exe".into()], ..Default::default() };
        assert!(s.is_ignored_app("game.EXE"));
        assert!(s.is_ignored_app("game"));
        assert!(!s.is_ignored_app("gamer.exe"));
    }

    #[test]
    fn old_settings_files_still_load() {
        let mut s: Settings =
            serde_json::from_str(r#"{"target":"de","deepl_api_key":"k:fx","providers":[{"kind":"DeepL","enabled":true}]}"#)
                .unwrap();
        s.normalize();
        assert_eq!(s.target, "de");
        assert_eq!(s.deepl_api_key.as_deref(), Some("k:fx"));
        assert_eq!(s.providers.len(), ProviderKind::ALL.len());
        assert_eq!(s.providers[0].kind, ProviderKind::DeepL);
        let json = serde_json::to_string(&s).unwrap();
        assert!(!json.contains("deepl_api_key"));
    }
}
