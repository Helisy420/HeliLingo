//! One translation from start to finish, without any UI:
//!
//! 1. normalize the text;
//! 2. "auto" source: detect the language locally (no network) to choose the
//!    target — text already in the target language goes to the second
//!    language. Engines still receive "auto" and detect on their own;
//! 3. answer from the cache (RAM, then disk) when possible;
//! 4. programmer mode: only strings and comments go to the engines;
//! 5. otherwise the hedged engine chain, streaming;
//! 6. store the answer in the cache.
//!
//! Each stage logs its time at `debug` (`HELILINGO_LOG=hl_pipeline=debug`).

use std::sync::Arc;
use std::time::Instant;

use hl_cache::{Cache, Entry, Key};
use hl_core::code;
use hl_core::lang::AUTO;
use hl_core::prompt::VERSION;
use hl_core::router::Finished;
use hl_core::{EngineError, EngineId, Job, Output, RequestId, Router, TranslateEvent, text};

/// What a window asks for.
#[derive(Clone, Debug)]
pub struct Request {
    pub text: String,
    pub source: String,
    pub target: String,
    /// Used when the text is already in `target` (Settings: «Если текст уже на …»).
    pub second_lang: String,
    /// Engine picked in the window ("" = the normal chain); part of the cache key.
    pub engine: String,
    pub programmer_mode: bool,
    /// Earlier fragments of this window («Переводить с контекстом»).
    pub context: Vec<hl_core::prompt::ContextPair>,
}

/// The finished translation, for history and statistics.
#[derive(Clone, Debug)]
pub struct Done {
    pub source_text: String,
    pub text: String,
    pub source_lang: Option<String>,
    pub target: String,
    pub engine: String,
    pub cached: bool,
}

pub struct Pipeline {
    pub cache: Option<Cache>,
}

impl Pipeline {
    pub async fn run(
        &self,
        id: RequestId,
        req: Request,
        router: Router,
        online: bool,
        mut emit: impl FnMut(TranslateEvent) + Send,
    ) -> Result<Done, EngineError> {
        let t0 = Instant::now();
        let source_text = text::normalize(&req.text);
        if source_text.is_empty() {
            return Err(EngineError::Other("empty".into()));
        }

        // Language plan.
        let detected = if req.source != AUTO {
            None
        } else if hl_detect::ready() {
            hl_detect::detect(&source_text)
        } else {
            let t = source_text.clone();
            tokio::task::spawn_blocking(move || hl_detect::detect(&t)).await.ok().flatten()
        };
        let source_lang = detected.map(str::to_owned).or_else(|| (req.source != AUTO).then(|| req.source.clone()));
        let target = match &source_lang {
            Some(s) if same_lang(s, &req.target) && !same_lang(&req.second_lang, &req.target) => req.second_lang.clone(),
            _ => req.target.clone(),
        };
        // Local detection only chooses the target. Engines still get "auto"
        // (it covers 10 languages; a near guess must not mislead them).
        let source = req.source.clone();
        emit(TranslateEvent::Planned { id, source_lang: source_lang.clone(), target: target.clone() });
        tracing::debug!(us = t0.elapsed().as_micros(), ?source_lang, %target, "planned");

        // Cache.
        let key = Key { text: &source_text, source: &req.source, target: &target, engine: &req.engine, version: VERSION };
        // With context the same text may translate differently: no cache read.
        if let Some(cache) = self.cache.as_ref().filter(|_| req.context.is_empty())
            && let Some(hit) = cache.get(&key).await
        {
            tracing::debug!(us = t0.elapsed().as_micros(), "cache hit");
            let engine = EngineId::new(hit.engine.clone());
            emit(TranslateEvent::Started { id, engine: engine.clone() });
            emit(TranslateEvent::Delta { id, text: hit.text.clone() });
            emit(TranslateEvent::Done {
                id,
                engine,
                text: hit.text.clone(),
                source_lang: hit.source_lang.clone().or(source_lang.clone()),
                alternatives: hit.alternatives.clone(),
                dict: hit.dict.clone(),
                transcription: hit.transcription.clone(),
                examples: hit.examples.clone(),
                cached: true,
            });
            return Ok(Done {
                source_text,
                text: hit.text.clone(),
                source_lang: hit.source_lang.clone().or(source_lang),
                target,
                engine: hit.engine.clone(),
                cached: true,
            });
        }

        let job = Arc::new(Job { text: source_text.clone(), source, target: target.clone(), context: req.context.clone(), hint: source_lang.clone() });
        let result = if req.programmer_mode && code::is_code(&source_text) {
            self.code(id, job, &router, online, &mut emit).await
        } else {
            router.run(id, job, online, &mut emit).await
        };
        let Finished { engine, output } = result?;
        tracing::debug!(us = t0.elapsed().as_micros(), %engine, "translated");

        let source_lang = output.source_lang.clone().or(source_lang);
        if let Some(cache) = &self.cache {
            let entry = Entry {
                source_text: source_text.clone(),
                text: output.text.clone(),
                engine: engine.to_string(),
                source_lang: source_lang.clone(),
                alternatives: output.alternatives.clone(),
                dict: output.dict.clone(),
                transcription: output.transcription.clone(),
                examples: output.examples.clone(),
                created: hl_cache::now(),
            };
            cache.put(&key, entry).await;
        }
        Ok(Done { source_text, text: output.text, source_lang, target, engine: engine.to_string(), cached: false })
    }

    /// Programmer mode: each string/comment through the chain (quietly),
    /// then the rebuilt code as one answer.
    async fn code(
        &self,
        id: RequestId,
        job: Arc<Job>,
        router: &Router,
        online: bool,
        emit: &mut (impl FnMut(TranslateEvent) + Send),
    ) -> Result<Finished, EngineError> {
        let pieces = code::split(&job.text);
        let mut out = Vec::new();
        let mut engine = None;
        for t in code::texts(&pieces) {
            let part = Arc::new(Job { text: t, context: Vec::new(), ..(*job).clone() });
            let f = router.run(id, part, online, |_| {}).await?;
            engine = Some(f.engine.clone());
            out.push(f.output.text);
        }
        let engine = engine.unwrap_or_else(|| EngineId::new("code"));
        let text = code::join(&pieces, &out);
        emit(TranslateEvent::Started { id, engine: engine.clone() });
        emit(TranslateEvent::Delta { id, text: text.clone() });
        emit(TranslateEvent::Done {
            id,
            engine: engine.clone(),
            text: text.clone(),
            source_lang: None,
            alternatives: Vec::new(),
            dict: Vec::new(),
            transcription: None,
            examples: Vec::new(),
            cached: false,
        });
        Ok(Finished { engine, output: Output { text, ..Output::default() } })
    }
}

fn same_lang(a: &str, b: &str) -> bool {
    a.split('-').next().unwrap_or(a).eq_ignore_ascii_case(b.split('-').next().unwrap_or(b))
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::FutureExt;
    use futures_util::future::BoxFuture;
    use hl_core::{Engine, HedgePolicy, Sink};

    /// Echoes "[target] text" and counts calls.
    struct Echo(std::sync::atomic::AtomicUsize);

    impl Engine for Echo {
        fn id(&self) -> EngineId {
            EngineId::new("echo")
        }
        fn translate(self: Arc<Self>, job: Arc<Job>, _sink: Sink) -> BoxFuture<'static, Result<Output, EngineError>> {
            self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            async move { Ok(Output { text: format!("[{}] {}", job.target, job.text), ..Output::default() }) }.boxed()
        }
    }

    fn setup() -> (Pipeline, Router, Arc<Echo>) {
        let path = std::env::temp_dir().join(format!("hl-pipeline-{}-{:?}.redb", std::process::id(), std::thread::current().id()));
        let _ = std::fs::remove_file(&path);
        let store = hl_cache::Store::open(&path).unwrap();
        let echo = Arc::new(Echo(0.into()));
        let router = Router { engines: vec![echo.clone()], policy: HedgePolicy::default(), bench: Arc::default() };
        (Pipeline { cache: Some(Cache::new(store)) }, router, echo)
    }

    fn req(text: &str) -> Request {
        Request {
            text: text.into(),
            source: AUTO.into(),
            target: "ru".into(),
            second_lang: "en".into(),
            engine: String::new(),
            programmer_mode: true,
            context: Vec::new(),
        }
    }

    fn router_clone(r: &Router) -> Router {
        Router { engines: r.engines.clone(), policy: r.policy, bench: r.bench.clone() }
    }

    #[tokio::test]
    async fn caches_and_flips_target() {
        let (p, r, echo) = setup();
        let mut planned = None;
        let d = p
            .run(RequestId(1), req("  Где  ближайшая аптека? "), router_clone(&r), true, |e| {
                if let TranslateEvent::Planned { target, .. } = e {
                    planned = Some(target);
                }
            })
            .await
            .unwrap();
        assert_eq!(planned.as_deref(), Some("en"), "Russian text with target ru goes to en");
        assert_eq!(d.text, "[en] Где ближайшая аптека?");
        let again = p.run(RequestId(2), req("Где ближайшая аптека?"), router_clone(&r), true, |_| {}).await.unwrap();
        assert!(again.cached);
        assert_eq!(echo.0.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn programmer_mode_keeps_code() {
        let (p, r, _) = setup();
        let src = "fn main() {\n    let key = \"user_id\";\n    println!(\"Hello there\");\n}\n";
        let d = p.run(RequestId(1), req(src), r, true, |_| {}).await.unwrap();
        assert!(d.text.contains("\"user_id\""));
        assert!(d.text.contains("println!(\"[ru] Hello there\")"));
    }
}
