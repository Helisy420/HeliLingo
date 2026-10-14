//! HeliLingo core: the translation pipeline without any UI or OS code.
//!
//! - [`lang`]: our language codes and names.
//! - [`engine`]: what a translation engine implements, and its errors.
//! - [`event`]: what a translation streams back to a window.
//! - [`router`]: the hedged engine chain (first engine to produce text wins).
//! - [`request`]: request ids and cancel-previous per window.
//! - [`text`]: normalizing, splitting into sentences/lines and joining back.
//! - [`code`]: programmer mode (translate only strings and comments).
//! - [`prompt`]: LLM prompts, glossary, output clean-up.

pub mod code;
pub mod engine;
pub mod event;
pub mod lang;
pub mod prompt;
pub mod request;
pub mod router;
pub mod text;

pub use engine::{DictEntry, DictGroup, Engine, EngineError, EngineId, Job, Output, Sink};
pub use event::TranslateEvent;
pub use request::{RequestId, Requests, Surface};
pub use router::{Bench, HedgePolicy, Router};
