//! Offline models as `hl_core::Engine`s. They need no network, so the
//! router still uses them when the PC is offline.

use std::sync::Arc;

use futures_util::FutureExt;
use futures_util::future::BoxFuture;
use hl_core::lang::AUTO;
use hl_core::prompt::Glossary;
use hl_core::{Engine, EngineError, EngineId, Job, Output, Sink};

use crate::{Config, Model, Runtime, base_code, same};

pub struct OfflineEngine {
    model: Model,
    config: Config,
    glossary: Option<Arc<Glossary>>,
}

impl OfflineEngine {
    pub fn new(model: Model, config: Config) -> Self {
        Self { model, config, glossary: None }
    }

    pub fn with_glossary(mut self, g: Arc<Glossary>) -> Self {
        self.glossary = Some(g);
        self
    }

    async fn run(&self, job: &Job, sink: &Sink) -> Result<Output, EngineError> {
        if !self.config.installed(self.model) {
            return Err(EngineError::Other("model not installed".into()));
        }
        // Local models need a source language: the given one, the local
        // guess, or the script's.
        let src = if job.source != AUTO {
            base_code(&job.source).to_owned()
        } else {
            job.hint.clone().or_else(|| hl_detect::detect(&job.text).map(str::to_owned)).unwrap_or_else(|| "en".into())
        };
        let src = base_code(&src).to_owned();
        let tgt = base_code(&job.target).to_owned();
        if same(&src, &tgt) {
            return Ok(Output { text: job.text.clone(), source_lang: Some(src), ..Output::default() });
        }
        if self.model.only_en_ru() && !matches!((src.as_str(), tgt.as_str()), ("en", "ru") | ("ru", "en")) {
            return Err(EngineError::UnsupportedPair);
        }
        let text = match self.model.runtime() {
            Runtime::Ct2 => {
                let (cfg, m, t, s, g) = (self.config.clone(), self.model, job.text.clone(), src.clone(), tgt.clone());
                let sink = sink.clone();
                // Sentences stream into the window as the worker finishes them.
                tokio::task::spawn_blocking(move || crate::worker::translate(&cfg, m, &t, &s, &g, &mut |p| sink.send(p)))
                    .await
                    .map_err(|_| crate::worker::failed())??
            }
            Runtime::Llama => {
                let terms = self.glossary.as_ref().map(|g| g.terms()).unwrap_or_default();
                crate::llama::translate(&self.config, self.model, &job.text, &src, &tgt, &job.context, &terms, sink).await?
            }
        };
        Ok(Output { text, source_lang: Some(src), ..Output::default() })
    }
}

impl Engine for OfflineEngine {
    fn id(&self) -> EngineId {
        EngineId::new(format!("offline.{}", self.model.id()))
    }

    fn needs_network(&self) -> bool {
        false
    }

    fn translate(self: Arc<Self>, job: Arc<Job>, sink: Sink) -> BoxFuture<'static, Result<Output, EngineError>> {
        async move { self.run(&job, &sink).await }.boxed()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn not_installed_and_pairs() {
        let cfg = Config { model_dir: std::env::temp_dir().join("hl-no-models-engine"), ..Config::default() };
        let e = Arc::new(OfflineEngine::new(Model::Opus, cfg));
        assert_eq!(e.id().as_str(), "offline.opus");
        assert!(!e.needs_network());
        let job = Arc::new(Job { text: "Hallo".into(), source: "de".into(), target: "ru".into(), ..Job::default() });
        assert!(matches!(e.translate(job, Sink::discard()).await, Err(EngineError::Other(_))));
    }
}
