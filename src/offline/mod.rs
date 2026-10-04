//! Offline translation: six downloadable model tiers, from OPUS-MT
//! (tiny, one language pair) to TranslateGemma 12B (large, uses context).
//!
//! - [`catalog`]: what each tier downloads (Hugging Face, the Argos package
//!   index, llama.cpp releases on GitHub), with sizes and SHA-256 sums.
//! - [`manager`]: install / cancel / delete, progress for the Settings page.
//! - [`download`]: resumable HTTP downloads with checksum verification.
//! - [`ct2`]: tiers 1–4 through CTranslate2 (`ct2rs`), run inside
//!   [`worker`] processes so a tier switched off really frees its memory.
//! - [`llama`]: tiers 5–6 through a hidden `llama-server` child process.
//! - [`prompts`], [`glossary`], [`detect`]: prompt building, the user's
//!   glossary file and a script-based language guess for "auto".
//! - [`sys`]: GPU / VRAM / CUDA detection, free disk space, folder picker.
//!
//! Which tiers are installed is read from the model folder (an
//! `installed.json` marker per tier), never stored in settings.

pub mod catalog;
mod ct2;
pub mod detect;
mod download;
pub mod glossary;
mod llama;
pub mod manager;
pub mod prompts;
pub mod sys;
pub mod worker;

use std::path::{Path, PathBuf};

use crate::settings::{OfflineDevice, OfflinePrecision, ProviderKind, Settings};
use crate::translate::Error;

/// User-visible engine errors (Russian in i18n's OFFLINE table).
pub const UNSUPPORTED_PAIR: &str = "This offline model doesn't support this language pair";
pub const MODEL_FAILED: &str = "The offline model failed";
pub const RUNTIME_FAILED: &str = "The local model server didn't start";

/// The six offline tiers, fastest first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Tier {
    /// OPUS-MT per language pair (CTranslate2 int8).
    SuperMegaFast,
    /// Argos Translate packages, pivot through English.
    SuperFast,
    /// NLLB-200 distilled 600M (CTranslate2 int8).
    Fast,
    /// NLLB-200 distilled 1.3B (CTranslate2 int8).
    Normal,
    /// TranslateGemma 4B (GGUF Q4_K_M, llama.cpp), with context.
    Medium,
    /// TranslateGemma 12B (GGUF Q4_K_M, llama.cpp), with context.
    Heavy,
}

/// How a tier runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Runtime {
    Ct2,
    Llama,
}

impl Tier {
    pub const ALL: [Tier; 6] = [Tier::SuperMegaFast, Tier::SuperFast, Tier::Fast, Tier::Normal, Tier::Medium, Tier::Heavy];

    pub fn kind(self) -> ProviderKind {
        match self {
            Tier::SuperMegaFast => ProviderKind::OpusMt,
            Tier::SuperFast => ProviderKind::Argos,
            Tier::Fast => ProviderKind::Nllb600,
            Tier::Normal => ProviderKind::Nllb13,
            Tier::Medium => ProviderKind::Gemma4,
            Tier::Heavy => ProviderKind::Gemma12,
        }
    }

    /// Stable id: the tier's folder name and its `--provider` name.
    pub fn id(self) -> &'static str {
        match self {
            Tier::SuperMegaFast => "opus",
            Tier::SuperFast => "argos",
            Tier::Fast => "nllb600",
            Tier::Normal => "nllb13",
            Tier::Medium => "gemma4",
            Tier::Heavy => "gemma12",
        }
    }

    /// From an id or the tier number "1"–"6".
    pub fn from_id(s: &str) -> Option<Tier> {
        let s = s.trim().to_ascii_lowercase();
        Tier::ALL
            .into_iter()
            .enumerate()
            .find(|(i, t)| t.id() == s || (i + 1).to_string() == s)
            .map(|(_, t)| t)
    }

    /// The owner's tier names ("супер-мега-быстрая" …).
    pub fn label(self) -> &'static str {
        use crate::i18n::tr;
        match self {
            Tier::SuperMegaFast => tr("super-mega-fast"),
            Tier::SuperFast => tr("super-fast"),
            Tier::Fast => tr("fast"),
            Tier::Normal => tr("normal"),
            Tier::Medium => tr("medium (with context)"),
            Tier::Heavy => tr("heavy (with context)"),
        }
    }

    pub fn runtime(self) -> Runtime {
        match self {
            Tier::Medium | Tier::Heavy => Runtime::Llama,
            _ => Runtime::Ct2,
        }
    }
}

/// The part of [`Settings`] the offline tiers work from.
#[derive(Clone, Debug)]
pub struct OfflineConfig {
    pub model_dir: PathBuf,
    pub device: OfflineDevice,
    /// 0 = automatic.
    pub threads: u32,
    pub precision: OfflinePrecision,
    pub offline_only: bool,
    pub use_context: bool,
    pub context_fragments: u32,
}

impl From<&Settings> for OfflineConfig {
    fn from(s: &Settings) -> Self {
        Self {
            model_dir: model_dir(&s.model_dir),
            device: s.offline_device,
            threads: s.offline_threads,
            precision: s.offline_precision,
            offline_only: s.offline_only,
            use_context: s.use_context,
            context_fragments: s.context_fragments,
        }
    }
}

impl Default for OfflineConfig {
    fn default() -> Self {
        (&Settings::default()).into()
    }
}

impl OfflineConfig {
    /// Installed = its marker is in the model folder (and, for the
    /// llama.cpp tiers, the server runtime too).
    pub fn installed(&self, tier: Tier) -> bool {
        catalog::is_installed(&self.model_dir, tier)
    }

    /// CPU threads to use: the setting, or the physical core count.
    pub fn thread_count(&self) -> usize {
        match self.threads {
            0 => sys::physical_cores().max(1),
            n => n as usize,
        }
    }
}

/// The default model folder: %LOCALAPPDATA%\HeliLingo\Models.
pub fn default_model_dir() -> PathBuf {
    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    base.join("HeliLingo").join("Models")
}

/// The model folder for the `model_dir` setting (empty = default).
pub fn model_dir(setting: &str) -> PathBuf {
    if setting.trim().is_empty() { default_model_dir() } else { PathBuf::from(setting.trim()) }
}

/// One previous fragment and its translation (the engine's context ring).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContextPair {
    pub source: String,
    pub translation: String,
}

/// Result of one offline translation.
#[derive(Clone, Debug)]
pub struct Output {
    pub text: String,
    /// The source language used (detected when `from` was "auto").
    pub src_lang: String,
}

/// Translates with `tier`. `context` is only used by the context tiers.
/// Blocks (model load on first use: seconds; llama-server start: longer).
pub fn translate(
    cfg: &OfflineConfig,
    tier: Tier,
    text: &str,
    from: &str,
    to: &str,
    context: &[ContextPair],
) -> Result<Output, Error> {
    if !cfg.installed(tier) {
        return Err(Error::Other(crate::translate::NOT_INSTALLED.into()));
    }
    let src = if from == "auto" { detect::guess(text) } else { base_code(from).to_owned() };
    let tgt = base_code(to);
    if same(&src, tgt) {
        // Nothing to translate; the engine's "already in target" logic
        // decides what to do with it.
        return Ok(Output { text: text.to_owned(), src_lang: src });
    }
    let text = match tier.runtime() {
        Runtime::Ct2 => worker::translate(cfg, tier, text, &src, tgt)?,
        Runtime::Llama => {
            let context = if cfg.use_context {
                let n = cfg.context_fragments as usize;
                &context[context.len().saturating_sub(n)..]
            } else {
                &[]
            };
            let glossary = if cfg.use_context { glossary::relevant(text) } else { Vec::new() };
            llama::translate(cfg, tier, text, &src, tgt, context, &glossary)?
        }
    };
    Ok(Output { text, src_lang: src })
}

/// Loads the tier's model in the background (or starts its server), so
/// the first real translation doesn't pay for it.
pub fn preload(cfg: &OfflineConfig, tier: Tier) {
    let cfg = cfg.clone();
    std::thread::spawn(move || {
        if cfg.installed(tier) {
            let _ = match tier.runtime() {
                Runtime::Ct2 => worker::preload(&cfg, tier),
                Runtime::Llama => llama::preload(&cfg, tier),
            };
        }
    });
}

/// Stops the model workers and the llama-server child. Call on app exit
/// (they also die with the app through a Job object, this is just faster).
pub fn shutdown() {
    worker::stop_all();
    llama::stop();
}

/// Unloads `tier`'s model, freeing its memory (RAM or video memory): its
/// worker or llama-server ends. It loads again the next time it is used.
pub fn unload(tier: Tier) {
    match tier.runtime() {
        Runtime::Ct2 => worker::stop(tier),
        Runtime::Llama => llama::stop_if(tier),
    }
}

/// Is `tier`'s model loaded in memory right now?
pub fn loaded(tier: Tier) -> bool {
    match tier.runtime() {
        Runtime::Ct2 => worker::running(tier),
        Runtime::Llama => llama::running().is_some_and(|(t, _)| t == tier),
    }
}

/// Unloads every tier whose processes depend on the device, threads or
/// precision (after those settings change).
pub fn unload_all() {
    worker::stop_all();
    llama::stop();
}

/// "zh-CN" → "zh", "pt" → "pt".
pub fn base_code(code: &str) -> &str {
    code.split('-').next().unwrap_or(code)
}

fn same(a: &str, b: &str) -> bool {
    base_code(a).eq_ignore_ascii_case(base_code(b))
}

/// Size of a folder's files, recursively (installed size on the page).
pub fn dir_size(dir: &Path) -> u64 {
    let Ok(rd) = std::fs::read_dir(dir) else { return 0 };
    rd.flatten()
        .map(|e| match e.file_type() {
            Ok(t) if t.is_dir() => dir_size(&e.path()),
            Ok(_) => e.metadata().map(|m| m.len()).unwrap_or(0),
            Err(_) => 0,
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tier_ids_round_trip() {
        for (i, t) in Tier::ALL.into_iter().enumerate() {
            assert_eq!(Tier::from_id(t.id()), Some(t));
            assert_eq!(Tier::from_id(&(i + 1).to_string()), Some(t));
            assert_eq!(t.kind().tier(), Some(t));
        }
        assert_eq!(Tier::from_id("google"), None);
        assert_eq!(ProviderKind::Google.tier(), None);
        assert_eq!(ProviderKind::from_cli("gemma4"), Some(ProviderKind::Gemma4));
        assert_eq!(ProviderKind::from_cli("2"), Some(ProviderKind::Argos));
        assert_eq!(ProviderKind::from_cli("deepl"), Some(ProviderKind::DeepL));
        assert_eq!(ProviderKind::from_cli("NLLB600M"), Some(ProviderKind::Nllb600));
    }

    #[test]
    fn labels_are_russian_by_default() {
        crate::i18n::set_lang("ru");
        assert_eq!(Tier::SuperMegaFast.label(), "супер-мега-быстрая");
        assert_eq!(Tier::Heavy.label(), "тяжёлая (с контекстом)");
    }

    #[test]
    fn missing_tier_is_not_installed() {
        let cfg = OfflineConfig { model_dir: std::env::temp_dir().join("hl-no-such-dir"), ..Default::default() };
        for t in Tier::ALL {
            assert!(!cfg.installed(t));
            assert!(matches!(translate(&cfg, t, "Hello", "en", "ru", &[]), Err(Error::Other(m)) if m == crate::translate::NOT_INSTALLED));
        }
    }
}
