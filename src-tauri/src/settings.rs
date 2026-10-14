//! User settings: `%APPDATA%\HeliLingo\settings.json`, one field per control
//! on the Figma settings pages. Every field has a default, so an older or
//! hand-edited file still loads. API keys are not here (see `hl_win::secret`).

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    // Общие
    pub ui_language: UiLanguage,
    pub theme: Theme,
    pub autostart: bool,
    pub animations: bool,
    pub hardware_acceleration: bool,
    /// «Переводить на».
    pub target_lang: String,
    /// «Переводить с»: a code or "auto".
    pub source_lang: String,
    /// «Если текст уже на русском» (on the target language): translate to this.
    pub second_lang: String,
    pub save_history: bool,
    /// Double-Ctrl popup look.
    pub popup_style: PopupStyle,
    pub bubble: Bubble,
    pub quick: Quick,
    /// Ctrl+C while the popup is open.
    pub popup_ctrl_c: PopupCtrlC,
    pub read_aloud: bool,
    pub show_wiki: bool,
    /// Shortcuts do nothing while one of these is in front (`game.exe`).
    pub ignored_apps: Vec<String>,
    /// …or while any fullscreen app is.
    pub skip_fullscreen: bool,
    /// «Скрывать номера»: phone / passport / card numbers in the history
    /// show as •••• (display only; copies and the stored history keep the
    /// real text).
    pub mask_numbers: bool,

    // Провайдеры
    /// Cloud engines in the user's order.
    pub engines: Vec<EngineEntry>,
    /// «Переключаться при ошибке».
    pub fallback: bool,
    /// «Время ожидания ответа», seconds.
    pub timeout_secs: u32,
    pub google_mode: GoogleMode,
    /// Not decided by the user yet: no default model.
    pub openrouter_model: Option<String>,
    pub yandex_folder: String,
    pub bing_region: String,

    // Офлайн и ускорение
    pub offline: Offline,

    // Контекст и режимы
    pub context_enabled: bool,
    pub context_fragments: u32,
    pub programmer_mode: bool,
    /// «Расширение VS Code»: the local API editors talk to (`api.rs`).
    pub editor_api: bool,
    pub ultra_detect_each: bool,
    /// «Не трогать фрагменты на русском» (on the target language).
    pub ultra_skip_target: bool,
    /// «Показывать окно «Ультра»»: off = replace the selection directly.
    pub ultra_window: bool,

    // Изображения и файлы (Figma 53:562)
    pub images: Images,
    pub files: Files,

    // Горячие клавиши
    pub hotkeys: Hotkeys,
    /// Double-tap interval, ms.
    pub double_tap_ms: u32,

    // Статистика
    pub keep_stats: bool,

    // О программе
    pub auto_update: bool,
    pub update_channel: UpdateChannel,

    /// The welcome window was shown.
    pub welcomed: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum UiLanguage {
    Ru,
    En,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Theme {
    Light,
    Dark,
    Blue,
    System,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PopupStyle {
    /// The card: language pair, translation, Replace / Copy.
    Full,
    /// Only the translation (Figma «Только перевод»).
    #[serde(alias = "compact")]
    Translation,
    /// The translation with mini-buttons (Figma «С кнопками»).
    Buttons,
}

/// Settings → «Баблы и Вики».
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Bubble {
    /// Long translations show 2–3 lines and «Развернуть».
    pub collapse_long: bool,
    /// Esc and a click outside close the bubble.
    pub close_on_outside: bool,
    pub wiki_lang: WikiLang,
    pub wiki_size: WikiSize,
    pub wiki_image: bool,
    pub placement: Placement,
    /// Wait before showing, ms.
    pub delay_ms: u32,
    pub word_mode: WordMode,
    /// Compact bubbles: part of speech («сущ.», «существительное») by words.
    pub show_pos: bool,
    /// Compact bubbles: other translations of a word («также …», the
    /// dictionary). Off: only the translation itself.
    pub show_variants: bool,
    /// 0 = never.
    pub auto_close_secs: u32,
    pub keep_on_hover: bool,
    pub remember_pinned: bool,
    /// Where a pinned bubble was last left (screen pixels).
    pub pinned_at: Option<(i32, i32)>,
    /// Text size, px.
    pub font_size: u32,
    /// Background opacity, %.
    pub opacity: u32,
    pub animation: bool,
}

impl Default for Bubble {
    fn default() -> Self {
        Self {
            collapse_long: true,
            close_on_outside: true,
            wiki_lang: WikiLang::Word,
            wiki_size: WikiSize::Short,
            wiki_image: false,
            placement: Placement::Selection,
            delay_ms: 0,
            word_mode: WordMode::Dictionary,
            show_pos: true,
            show_variants: true,
            auto_close_secs: 0,
            keep_on_hover: true,
            remember_pinned: false,
            pinned_at: None,
            font_size: 15,
            opacity: 100,
            animation: true,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WikiLang {
    /// The selected word's language.
    Word,
    /// The translation's language.
    Translation,
    /// The interface language.
    Interface,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WikiSize {
    Short,
    Long,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Placement {
    /// Under (or above) the selection.
    Selection,
    /// At the mouse pointer.
    Cursor,
    /// In the middle of the screen, a third of the way down.
    Center,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WordMode {
    Dictionary,
    Wiki,
    Both,
    /// «Ничего»: a word gets just its translation.
    #[serde(rename = "none")]
    Nothing,
}

/// Quick translate layout.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Quick {
    pub layout: QuickLayout,
    /// «Другие варианты» under the translation.
    pub suggestions: bool,
    /// Recent translations under the result; an empty field shows only them.
    pub history: bool,
    /// Icon buttons with tooltips instead of labelled ones.
    pub mini_buttons: bool,
}

impl Default for Quick {
    fn default() -> Self {
        Self { layout: QuickLayout::Window, suggestions: true, history: false, mini_buttons: false }
    }
}

/// Settings → «Изображения и файлы» → «Область экрана»: how a translated
/// screen area is shown (the result views are TODO §8).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Images {
    /// «Режим по умолчанию».
    pub mode: AreaMode,
    /// «Прозрачность подложки», % (the «Поверх» blocks).
    pub backdrop_opacity: u32,
    /// «Оригинал по удержанию Alt».
    pub alt_original: bool,
    /// The area's text language ("auto") and the language to translate it
    /// to ("" = «Переводить на»): a game can be Japanese → Russian while
    /// everything else is English → Russian.
    pub source_lang: String,
    pub target_lang: String,
    /// Engine for areas (an offline model too); `None` = the usual chain.
    pub engine: Option<String>,
    /// «Субтитры»: text size, px.
    pub subtitle_size: u32,
}

impl Default for Images {
    fn default() -> Self {
        Self {
            mode: AreaMode::Over,
            backdrop_opacity: 80,
            alt_original: true,
            source_lang: "auto".into(),
            target_lang: String::new(),
            engine: None,
            subtitle_size: 26,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AreaMode {
    /// «Поверх»: translucent blocks over the original text.
    Over,
    /// «Плашка»: one card with the whole translation.
    Plate,
    /// «Рядом»: a numbered list next to the area.
    Side,
    /// «Субтитры»: one line of translation at the bottom of the screen.
    Subtitles,
}

/// Settings → «Изображения и файлы» → «Файлы».
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Files {
    /// «Формат результата».
    pub format: FileFormat,
    /// «Сохранять форматирование».
    pub keep_formatting: bool,
    /// «Двуязычный документ»: original and translation paragraph by paragraph.
    pub bilingual: bool,
    /// «Папка для результатов»; empty = Documents\HeliLingo.
    pub out_dir: String,
}

impl Default for Files {
    fn default() -> Self {
        Self { format: FileFormat::Same, keep_formatting: true, bilingual: false, out_dir: String::new() }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FileFormat {
    /// «Как у оригинала» (a PDF becomes DOCX: its layout can't be rebuilt).
    Same,
    Docx,
    Txt,
}

/// Where translated documents go: the setting, or Documents\HeliLingo.
pub fn files_dir(setting: &str) -> PathBuf {
    if !setting.trim().is_empty() {
        return PathBuf::from(setting);
    }
    hl_win::documents_dir()
        .or_else(|| std::env::var_os("USERPROFILE").map(|h| PathBuf::from(h).join("Documents")))
        .unwrap_or_else(std::env::temp_dir)
        .join("HeliLingo")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum QuickLayout {
    /// Language bar, input, result.
    Window,
    /// One line like a launcher: type on top, the translation right below.
    Line,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PopupCtrlC {
    CopyClose,
    Copy,
    Ignore,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GoogleMode {
    Auto,
    Web,
    Api,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum UpdateChannel {
    Stable,
    Beta,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineEntry {
    pub id: String,
    pub enabled: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Offline {
    /// Off: nothing local is downloaded, started or loaded.
    pub enabled: bool,
    pub device: Device,
    /// 0 = auto (physical cores).
    pub threads: u32,
    pub precision: Precision,
    /// Empty = `%LOCALAPPDATA%\HeliLingo\models`.
    pub model_dir: String,
    /// Installed models the user switched on, in order (ids: "opus", …).
    pub tiers: Vec<String>,
    /// Who reads text from images and screen areas.
    pub screen_ocr: ScreenOcr,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ScreenOcr {
    /// Windows' own OCR (language packs of the system).
    #[default]
    Windows,
    /// Granite Vision, offline (when installed).
    Granite,
}

impl Offline {
    /// What the offline models run with.
    pub fn config(&self) -> hl_offline::Config {
        hl_offline::Config {
            model_dir: hl_offline::model_dir(&self.model_dir),
            device: match self.device {
                Device::Auto => hl_offline::Device::Auto,
                Device::Cpu => hl_offline::Device::Cpu,
                Device::Gpu => hl_offline::Device::Gpu,
            },
            threads: self.threads,
            precision: match self.precision {
                Precision::Auto => hl_offline::Precision::Auto,
                Precision::Int8 => hl_offline::Precision::Int8,
                Precision::Int8Float16 => hl_offline::Precision::Int8Float16,
                Precision::Float32 => hl_offline::Precision::Float32,
            },
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Device {
    Auto,
    Cpu,
    Gpu,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Precision {
    Auto,
    Int8,
    Int8Float16,
    Float32,
}

/// Shortcut strings: "Ctrl+Shift+L", "Ctrl×2" (double tap), "Mouse4",
/// "Mouse5×2", or "" (off).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Hotkeys {
    /// «Перевести выделенное».
    pub translate_selection: String,
    /// «Быстрое окно».
    pub quick: String,
    /// «Окно перевода».
    pub translator: String,
    /// «Режим «Ультра»».
    pub ultra: String,
    /// «Скопировать перевод»: translate the selection, copy, no window.
    pub copy_translation: String,
    /// «Перевод по скриншоту».
    pub screenshot: String,
    /// «Перевод окна»: the capture overlay starts in «Окно». Empty by
    /// default (Ctrl+Shift+W closes browser windows).
    pub window_capture: String,
    /// Translate and paste over the selection, no window.
    pub paste_translation: String,
    /// Ctrl+C twice translates what was copied.
    pub double_copy: bool,
}

impl Default for Hotkeys {
    fn default() -> Self {
        Self {
            translate_selection: "Ctrl×2".into(),
            quick: "Ctrl+Shift+L".into(),
            translator: "Ctrl+Alt+T".into(),
            ultra: "Ctrl+Shift+S".into(),
            copy_translation: "Ctrl+Shift+C".into(),
            screenshot: "Ctrl+Shift+O".into(),
            window_capture: String::new(),
            paste_translation: "Ctrl+Alt+R".into(),
            double_copy: true,
        }
    }
}

impl Default for Offline {
    fn default() -> Self {
        Self {
            enabled: false,
            device: Device::Auto,
            threads: 0,
            precision: Precision::Auto,
            model_dir: String::new(),
            tiers: Vec::new(),
            screen_ocr: ScreenOcr::Windows,
        }
    }
}

pub const CLOUD_ENGINES: [&str; 5] = ["google", "yandex", "deepl", "bing", "openrouter"];

impl Default for Settings {
    fn default() -> Self {
        Self {
            ui_language: UiLanguage::Ru,
            theme: Theme::Blue,
            autostart: true,
            animations: true,
            hardware_acceleration: true,
            target_lang: "ru".into(),
            source_lang: "auto".into(),
            second_lang: "en".into(),
            save_history: true,
            popup_style: PopupStyle::Full,
            bubble: Bubble::default(),
            quick: Quick::default(),
            images: Images::default(),
            files: Files::default(),
            popup_ctrl_c: PopupCtrlC::CopyClose,
            read_aloud: false,
            show_wiki: true,
            ignored_apps: Vec::new(),
            skip_fullscreen: true,
            mask_numbers: false,
            engines: CLOUD_ENGINES
                .iter()
                .map(|id| EngineEntry { id: (*id).into(), enabled: matches!(*id, "google" | "yandex") })
                .collect(),
            fallback: true,
            timeout_secs: 5,
            google_mode: GoogleMode::Auto,
            openrouter_model: None,
            yandex_folder: String::new(),
            bing_region: String::new(),
            offline: Offline::default(),
            context_enabled: false,
            context_fragments: 5,
            editor_api: true,
            programmer_mode: false,
            ultra_detect_each: true,
            ultra_skip_target: false,
            ultra_window: true,
            hotkeys: Hotkeys::default(),
            double_tap_ms: 400,
            keep_stats: true,
            auto_update: true,
            update_channel: UpdateChannel::Stable,
            welcomed: false,
        }
    }
}

/// `%APPDATA%\HeliLingo`.
pub fn data_dir() -> PathBuf {
    let base = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    base.join("HeliLingo")
}

/// `%LOCALAPPDATA%\HeliLingo` (logs, models: machine-local data).
pub fn local_dir() -> PathBuf {
    let base = std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    base.join("HeliLingo")
}

fn file() -> PathBuf {
    data_dir().join("settings.json")
}

impl Settings {
    /// Loads settings.json; a missing or broken file gives the defaults
    /// (a broken one is kept as settings.json.bad for the user).
    pub fn load() -> Self {
        let path = file();
        let Ok(text) = std::fs::read_to_string(&path) else { return Self::default() };
        match serde_json::from_str::<Settings>(&text) {
            Ok(mut s) => {
                s.fix_up();
                s
            }
            Err(e) => {
                tracing::warn!("settings.json unreadable ({e}); using defaults");
                let _ = std::fs::rename(&path, path.with_extension("json.bad"));
                Self::default()
            }
        }
    }

    /// Writes atomically (temp file + rename).
    pub fn save(&self) -> std::io::Result<()> {
        let path = file();
        std::fs::create_dir_all(path.parent().expect("has parent"))?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(self).expect("serializable"))?;
        std::fs::rename(tmp, path)
    }

    /// Keeps lists valid after loading: every cloud engine listed once.
    fn fix_up(&mut self) {
        self.engines.retain(|e| CLOUD_ENGINES.contains(&e.id.as_str()));
        let mut seen = std::collections::HashSet::new();
        self.engines.retain(|e| seen.insert(e.id.clone()));
        for id in CLOUD_ENGINES {
            if !self.engines.iter().any(|e| e.id == id) {
                self.engines.push(EngineEntry { id: id.into(), enabled: false });
            }
        }
        self.timeout_secs = self.timeout_secs.clamp(1, 60);
        self.double_tap_ms = self.double_tap_ms.clamp(150, 1000);
        self.context_fragments = self.context_fragments.clamp(1, 20);
    }

    /// Applies a partial JSON object from a window ("only the changed
    /// fields"), validating the result as a whole.
    pub fn merged(&self, patch: serde_json::Value) -> Result<Settings, String> {
        let mut current = serde_json::to_value(self).expect("serializable");
        merge(&mut current, patch);
        let mut next: Settings = serde_json::from_value(current).map_err(|e| e.to_string())?;
        next.fix_up();
        Ok(next)
    }
}

/// JSON merge-patch: objects merge key by key, anything else replaces.
fn merge(base: &mut serde_json::Value, patch: serde_json::Value) {
    match (base, patch) {
        (serde_json::Value::Object(b), serde_json::Value::Object(p)) => {
            for (k, v) in p {
                merge(b.entry(k).or_insert(serde_json::Value::Null), v);
            }
        }
        (b, p) => *b = p,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partial_file_gets_defaults() {
        let s: Settings = serde_json::from_str(r#"{"theme":"light","hotkeys":{"quick":"Ctrl+Q"}}"#).unwrap();
        assert_eq!(s.theme, Theme::Light);
        assert_eq!(s.hotkeys.quick, "Ctrl+Q");
        assert_eq!(s.hotkeys.translator, "Ctrl+Alt+T");
        assert_eq!(s.target_lang, "ru");
    }

    #[test]
    fn patch_merges_nested() {
        let s = Settings::default();
        let n = s.merged(serde_json::json!({ "offline": { "enabled": true }, "timeoutSecs": 99 })).unwrap();
        assert!(n.offline.enabled);
        assert_eq!(n.offline.device, Device::Auto);
        assert_eq!(n.timeout_secs, 60);
        assert!(s.merged(serde_json::json!({ "theme": "pink" })).is_err());
    }

    #[test]
    fn engines_are_completed() {
        let mut s: Settings = serde_json::from_str(r#"{"engines":[{"id":"deepl","enabled":true},{"id":"x","enabled":true}]}"#).unwrap();
        s.fix_up();
        assert_eq!(s.engines[0].id, "deepl");
        assert_eq!(s.engines.len(), 5);
    }
}
