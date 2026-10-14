//! What a translation engine implements.
//!
//! An engine gets the job and a [`Sink`] for streamed text. Streaming
//! engines (OpenRouter, llama-server) push pieces into the sink as they
//! arrive; the others just return the result. Cancelling = dropping the
//! future, so engines must not hold work that outlives it.

use std::fmt;
use std::sync::Arc;

use futures_util::future::BoxFuture;
use serde::{Deserialize, Serialize};

use crate::prompt::ContextPair;

/// Stable engine id: "google", "yandex", "deepl", "bing", "openrouter",
/// "offline.opus", "offline.gemma4", …
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
pub struct EngineId(pub String);

impl EngineId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for EngineId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// One translation to do.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Job {
    pub text: String,
    /// Our language code, or [`crate::lang::AUTO`].
    pub source: String,
    pub target: String,
    /// Earlier fragments and their translations («Переводить с контекстом»).
    /// Engines that can use it (DeepL, LLMs) do; the others ignore it.
    pub context: Vec<ContextPair>,
    /// The source language as detected locally (a guess, never sent as the
    /// source): lets an engine start a side request early.
    pub hint: Option<String>,
}

/// Dictionary translations of a word for one part of speech.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DictGroup {
    /// Part of speech as the engine names it, in English ("noun", "verb").
    pub pos: String,
    pub terms: Vec<String>,
    /// The main translations with what they mean (dictionary bubble).
    #[serde(default)]
    pub entries: Vec<DictEntry>,
}

/// One meaning: a translation and the words that explain it
/// (`car` — «автомобиль, грузовик»).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DictEntry {
    pub word: String,
    pub gloss: Vec<String>,
}

/// An engine's finished result.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Output {
    /// The whole translation. A streaming engine may leave it empty: the
    /// router then uses what went through the sink.
    pub text: String,
    pub source_lang: Option<String>,
    pub alternatives: Vec<String>,
    /// Dictionary groups for a word (Google, Yandex).
    pub dict: Vec<DictGroup>,
    /// How the source word sounds ("kɑː"), when the engine knows.
    pub transcription: Option<String>,
    /// Example sentences with the source word, in the source language.
    pub examples: Vec<String>,
}

/// Where a streaming engine pushes text as it arrives.
#[derive(Clone)]
pub struct Sink(Arc<dyn Fn(String) + Send + Sync>);

impl Sink {
    pub fn new(f: impl Fn(String) + Send + Sync + 'static) -> Self {
        Self(Arc::new(f))
    }
    /// A sink that drops everything (for non-streaming callers).
    pub fn discard() -> Self {
        Self::new(|_| {})
    }
    pub fn send(&self, text: impl Into<String>) {
        let text = text.into();
        if !text.is_empty() {
            (self.0)(text);
        }
    }
}

/// Why an engine failed. `Serialize` because the UI shows it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, thiserror::Error)]
#[serde(tag = "kind", content = "detail", rename_all = "camelCase")]
pub enum EngineError {
    #[error("no API key")]
    NoKey,
    #[error("the key was rejected")]
    Unauthorized,
    #[error("out of credits")]
    PaymentRequired,
    #[error("too many requests")]
    RateLimited,
    #[error("this language pair isn't supported")]
    UnsupportedPair,
    #[error("no connection: {0}")]
    Network(String),
    #[error("no answer in time")]
    Timeout,
    #[error("server error: {0}")]
    Server(String),
    #[error("no engine is enabled")]
    NoEngines,
    #[error("{0}")]
    Other(String),
}

impl EngineError {
    /// Errors after which the engine is left alone for a while.
    pub fn benches(&self) -> bool {
        matches!(self, Self::RateLimited | Self::PaymentRequired | Self::Unauthorized)
    }
}

/// A translation engine. Implementations are cheap to share (`Arc`).
pub trait Engine: Send + Sync + 'static {
    fn id(&self) -> EngineId;

    /// Cloud engines are skipped at once when the PC is offline.
    fn needs_network(&self) -> bool {
        true
    }

    fn translate(self: Arc<Self>, job: Arc<Job>, sink: Sink) -> BoxFuture<'static, Result<Output, EngineError>>;
}
