//! Offline models: nothing here runs, downloads or loads until the user
//! switches offline on and installs a model (AGENTS.md, «Офлайн»).
//!
//! - [`catalog`]: what each model downloads (Hugging Face files pinned to a
//!   commit with their SHA-256, Argos packages, llama.cpp releases).
//! - [`manager`]: install / cancel / delete with progress for Settings.
//! - [`download`]: resumable downloads with checksum checks.
//! - [`worker`] (+ `ct2`): OPUS-MT, Argos and NLLB through CTranslate2, in a
//!   worker process per model so switching one off really frees its memory.
//! - [`llama`]: TranslateGemma and Granite Vision through `llama-server`.
//! - [`engine`]: the models as `hl_core::Engine`s for the router.
//! - [`vision`]: text from a screenshot with Granite Vision.
//! - [`sys`]: graphics card / VRAM / CUDA, cores, free disk, Job objects.
//!
//! Which models are installed is read from the model folder (an
//! `installed.json` marker per model), never stored in settings.

pub mod catalog;
#[cfg(feature = "ct2")]
mod ct2;
pub mod download;
pub mod engine;
pub mod llama;
pub mod manager;
pub mod sys;
pub mod vision;
pub mod worker;

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Downloadable models, fastest translator first; Granite reads screens.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Model {
    /// OPUS-MT, English ⇄ Russian (CTranslate2 int8).
    Opus,
    /// Argos Translate packages, English ⇄ Russian, pivot through English.
    Argos,
    /// NLLB-200 distilled 600M (CTranslate2 int8), 200 languages.
    Nllb600,
    /// NLLB-200 distilled 1.3B (CTranslate2 int8).
    Nllb13,
    /// TranslateGemma 4B (GGUF Q4_K_M), uses context and the glossary.
    Gemma4,
    /// TranslateGemma 12B (GGUF Q4_K_M).
    Gemma12,
    /// IBM Granite Vision 4.1 (GGUF Q4_K_M + projector): text from screenshots.
    Granite,
}

/// How a model runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Runtime {
    Ct2,
    Llama,
}

impl Model {
    pub const ALL: [Model; 7] =
        [Model::Opus, Model::Argos, Model::Nllb600, Model::Nllb13, Model::Gemma4, Model::Gemma12, Model::Granite];

    /// Stable id: the model's folder, `offline.<id>` as an engine id.
    pub fn id(self) -> &'static str {
        match self {
            Model::Opus => "opus",
            Model::Argos => "argos",
            Model::Nllb600 => "nllb600",
            Model::Nllb13 => "nllb13",
            Model::Gemma4 => "gemma4",
            Model::Gemma12 => "gemma12",
            Model::Granite => "granite",
        }
    }

    pub fn from_id(s: &str) -> Option<Model> {
        let s = s.trim().trim_start_matches("offline.").to_ascii_lowercase();
        Model::ALL.into_iter().find(|m| m.id() == s)
    }

    pub fn name(self) -> &'static str {
        match self {
            Model::Opus => "OPUS-MT",
            Model::Argos => "Argos Translate",
            Model::Nllb600 => "NLLB-200 600M",
            Model::Nllb13 => "NLLB-200 1.3B",
            Model::Gemma4 => "TranslateGemma 4B",
            Model::Gemma12 => "TranslateGemma 12B",
            Model::Granite => "Granite Vision 4.1",
        }
    }

    pub fn runtime(self) -> Runtime {
        match self {
            Model::Gemma4 | Model::Gemma12 | Model::Granite => Runtime::Llama,
            _ => Runtime::Ct2,
        }
    }

    /// Translates text (all but Granite, which reads images).
    pub fn translates(self) -> bool {
        self != Model::Granite
    }

    /// Uses earlier fragments and the glossary.
    pub fn uses_context(self) -> bool {
        matches!(self, Model::Gemma4 | Model::Gemma12)
    }

    /// Translates only English ⇄ Russian.
    pub fn only_en_ru(self) -> bool {
        matches!(self, Model::Opus | Model::Argos)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Device {
    #[default]
    Auto,
    Cpu,
    Gpu,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Precision {
    #[default]
    Auto,
    Int8,
    Int8Float16,
    Float32,
}

/// What the models run with (from Settings → Офлайн).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Config {
    pub model_dir: PathBuf,
    pub device: Device,
    /// 0 = physical cores.
    pub threads: u32,
    pub precision: Precision,
}

impl Default for Config {
    fn default() -> Self {
        Self { model_dir: default_model_dir(), device: Device::Auto, threads: 0, precision: Precision::Auto }
    }
}

impl Config {
    pub fn installed(&self, m: Model) -> bool {
        catalog::is_installed(&self.model_dir, m)
    }

    pub fn thread_count(&self) -> usize {
        match self.threads {
            0 => sys::physical_cores().max(1),
            n => n as usize,
        }
    }
}

/// `%LOCALAPPDATA%\HeliLingo\models`.
pub fn default_model_dir() -> PathBuf {
    let base = std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    base.join("HeliLingo").join("models")
}

/// The folder for the `model_dir` setting (empty = default).
pub fn model_dir(setting: &str) -> PathBuf {
    if setting.trim().is_empty() { default_model_dir() } else { PathBuf::from(setting.trim()) }
}

/// "zh-CN" → "zh".
pub fn base_code(code: &str) -> &str {
    code.split('-').next().unwrap_or(code)
}

pub(crate) fn same(a: &str, b: &str) -> bool {
    base_code(a).eq_ignore_ascii_case(base_code(b))
}

/// Size of a folder's files, recursively.
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

/// Ends every model process (app exit; the Job object would anyway).
pub fn shutdown() {
    worker::stop_all();
    llama::stop_all();
}

/// Frees `m`'s memory; it loads again on next use.
pub fn unload(m: Model) {
    match m.runtime() {
        Runtime::Ct2 => worker::stop(m),
        Runtime::Llama => llama::stop(m),
    }
}

/// Is `m` loaded right now?
pub fn loaded(m: Model) -> bool {
    match m.runtime() {
        Runtime::Ct2 => worker::running(m),
        Runtime::Llama => llama::running(m),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_round_trip() {
        for m in Model::ALL {
            assert_eq!(Model::from_id(m.id()), Some(m));
            assert_eq!(Model::from_id(&format!("offline.{}", m.id())), Some(m));
        }
        assert_eq!(Model::from_id("google"), None);
        assert!(!Model::Granite.translates() && Model::Gemma4.uses_context());
        assert_eq!(serde_json::to_string(&Precision::Int8Float16).unwrap(), "\"int8Float16\"");
    }
}
