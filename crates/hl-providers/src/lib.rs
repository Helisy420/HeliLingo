//! Translation engines that talk HTTP.
//!
//! - [`http`]: the shared client and status → [`EngineError`] mapping.
//! - [`google`]: keyless web endpoints, or Cloud Translation with a key.
//! - [`yandex`]: keyless mobile-app endpoint + dictionary, or Yandex Cloud.
//! - [`bing`]: keyless bing.com session, or Azure Translator.
//! - [`deepl`]: DeepL API (key), with context.
//! - [`openrouter`]: any chat model by key, streamed; the model list.
//! - [`wiki`]: Wikipedia summaries for the popup card.

pub mod bing;
pub mod deepl;
pub mod google;
pub mod http;
pub mod openrouter;
pub mod wiki;
pub mod yandex;

pub use bing::Bing;
pub use deepl::DeepL;
pub use google::{Google, GoogleMode};
pub use http::client;
pub use openrouter::OpenRouter;
pub use yandex::Yandex;

use hl_core::EngineError;
