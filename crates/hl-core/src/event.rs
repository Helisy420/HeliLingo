//! What one translation streams back to the window that asked for it.

use serde::Serialize;

use crate::engine::{DictGroup, EngineError, EngineId};
use crate::request::RequestId;

/// Events of one request, in order: `Started`, then `Delta`s (and maybe a
/// `Replace` when the engine changes mid-way), then `Done` or `Error`.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum TranslateEvent {
    /// Before any engine runs: the languages actually used (the source as
    /// detected locally, and the target after "already in the target
    /// language → second language").
    Planned { id: RequestId, source_lang: Option<String>, target: String },
    /// An engine produced the first text; `engine` is the one shown.
    Started { id: RequestId, engine: EngineId },
    /// More text to append.
    Delta { id: RequestId, text: String },
    /// Throw away what was shown: another engine took over with this text.
    Replace { id: RequestId, engine: EngineId, text: String },
    /// The full translation.
    Done {
        id: RequestId,
        engine: EngineId,
        text: String,
        /// Detected source language, when the engine reported one.
        source_lang: Option<String>,
        /// Other translations of a word or short phrase.
        alternatives: Vec<String>,
        /// Dictionary groups of a word, by part of speech.
        dict: Vec<DictGroup>,
        transcription: Option<String>,
        examples: Vec<String>,
        cached: bool,
    },
    /// Every engine failed; `kind` is the most useful of their errors.
    Error { id: RequestId, kind: EngineError },
}

impl TranslateEvent {
    pub fn id(&self) -> RequestId {
        match self {
            Self::Planned { id, .. }
            | Self::Started { id, .. }
            | Self::Delta { id, .. }
            | Self::Replace { id, .. }
            | Self::Done { id, .. }
            | Self::Error { id, .. } => *id,
        }
    }
}
