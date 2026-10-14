//! The hedged engine chain.
//!
//! Engines are tried in the user's order. Engine 0 starts at once; the next
//! one starts when every running engine has failed, or when none of them
//! has produced text within the hedge delay (doubling for each later
//! engine). The first engine to produce text wins and the others are
//! aborted. If the winner fails mid-stream, the chain goes on and the next
//! engine's text replaces what was shown.
//!
//! Engines that hit a rate limit (or a rejected key / no credits) are
//! benched: skipped for 1 minute, doubling up to 30, unless every engine is
//! benched.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::mpsc;
use tokio::task::{AbortHandle, JoinSet};
use tokio::time::Instant;

use crate::engine::{Engine, EngineError, EngineId, Job, Output, Sink};
use crate::event::TranslateEvent;
use crate::request::RequestId;

#[derive(Clone, Copy, Debug)]
pub struct HedgePolicy {
    /// «Переключаться при ошибке». Off: engine 0 only, no fallback.
    pub fallback: bool,
    /// How long engine 0 runs alone before engine 1 joins.
    pub first_delay: Duration,
    /// «Время ожидания ответа»: an engine with no text by then counts as failed.
    pub timeout: Duration,
}

impl Default for HedgePolicy {
    fn default() -> Self {
        Self { fallback: true, first_delay: Duration::from_millis(600), timeout: Duration::from_secs(5) }
    }
}

const BENCH_FIRST: Duration = Duration::from_secs(60);
const BENCH_MAX: Duration = Duration::from_secs(30 * 60);

/// Engines that are resting after a rate limit, shared by all requests.
#[derive(Default)]
pub struct Bench(Mutex<HashMap<EngineId, (Instant, Duration)>>);

impl Bench {
    pub fn is_benched(&self, id: &EngineId) -> bool {
        self.0.lock().unwrap().get(id).is_some_and(|(until, _)| Instant::now() < *until)
    }
    fn punish(&self, id: &EngineId) {
        let mut map = self.0.lock().unwrap();
        let next = match map.get(id) {
            Some((_, last)) => (*last * 2).min(BENCH_MAX),
            None => BENCH_FIRST,
        };
        map.insert(id.clone(), (Instant::now() + next, next));
    }
    fn forgive(&self, id: &EngineId) {
        self.0.lock().unwrap().remove(id);
    }
}

pub struct Router {
    pub engines: Vec<Arc<dyn Engine>>,
    pub policy: HedgePolicy,
    pub bench: Arc<Bench>,
}

enum Msg {
    Delta(usize, String),
    Finished(usize, Result<Output, EngineError>),
}

/// The result of a whole request.
#[derive(Clone, Debug, PartialEq)]
pub struct Finished {
    pub engine: EngineId,
    pub output: Output,
}

impl Router {
    /// Runs `job` through the chain, reporting through `emit`. Dropping the
    /// returned future aborts every engine still running.
    pub async fn run(
        &self,
        id: RequestId,
        job: Arc<Job>,
        online: bool,
        mut emit: impl FnMut(TranslateEvent) + Send,
    ) -> Result<Finished, EngineError> {
        let result = self.chain(job, online, &mut |e| emit(e.with_id(id))).await;
        match &result {
            Ok(f) => emit(TranslateEvent::Done {
                id,
                engine: f.engine.clone(),
                text: f.output.text.clone(),
                source_lang: f.output.source_lang.clone(),
                alternatives: f.output.alternatives.clone(),
                dict: f.output.dict.clone(),
                transcription: f.output.transcription.clone(),
                examples: f.output.examples.clone(),
                cached: false,
            }),
            Err(kind) => emit(TranslateEvent::Error { id, kind: kind.clone() }),
        }
        result
    }

    fn candidates(&self, online: bool) -> Vec<usize> {
        let usable: Vec<usize> =
            (0..self.engines.len()).filter(|&i| online || !self.engines[i].needs_network()).collect();
        let rested: Vec<usize> =
            usable.iter().copied().filter(|&i| !self.bench.is_benched(&self.engines[i].id())).collect();
        let mut list = if rested.is_empty() { usable } else { rested };
        if !self.policy.fallback {
            list.truncate(1);
        }
        list
    }

    async fn chain(
        &self,
        job: Arc<Job>,
        online: bool,
        emit: &mut (dyn FnMut(TranslateEvent) + Send),
    ) -> Result<Finished, EngineError> {
        let order = self.candidates(online);
        if order.is_empty() {
            return Err(if online { EngineError::NoEngines } else { EngineError::Network("offline".into()) });
        }

        let (tx, mut rx) = mpsc::unbounded_channel::<Msg>();
        let mut tasks = JoinSet::new();
        let mut running: HashMap<usize, (AbortHandle, Instant)> = HashMap::new();
        let mut next = 0usize; // position in `order` of the next engine to start
        let mut hedge_at: Option<Instant> = None;
        let mut winner: Option<usize> = None;
        let mut shown = String::new(); // text the window shows now
        let mut replaced = false; // a previous winner's text is on screen
        let mut errors: Vec<(usize, EngineError)> = Vec::new();

        let mut start = |next: &mut usize, running: &mut HashMap<usize, (AbortHandle, Instant)>| {
            let idx = order[*next];
            *next += 1;
            let engine = self.engines[idx].clone();
            let tx2 = tx.clone();
            let sink = Sink::new(move |t| {
                let _ = tx2.send(Msg::Delta(idx, t));
            });
            let tx3 = tx.clone();
            let fut = engine.translate(job.clone(), sink);
            let h = tasks.spawn(async move {
                let r = fut.await;
                let _ = tx3.send(Msg::Finished(idx, r));
            });
            running.insert(idx, (h, Instant::now()));
            tracing::debug!(engine = %self.engines[idx].id(), "engine started");
        };

        start(&mut next, &mut running);
        if next < order.len() {
            hedge_at = Some(Instant::now() + self.policy.first_delay);
        }
        let mut hedge_delay = self.policy.first_delay;

        loop {
            // Earliest timeout among engines that haven't produced text yet.
            let timeout_at = running
                .iter()
                .filter(|(i, _)| Some(**i) != winner)
                .map(|(_, (_, t))| *t + self.policy.timeout)
                .min();

            tokio::select! {
                msg = rx.recv() => {
                    let Some(msg) = msg else { unreachable!("the router holds a sender") };
                    match msg {
                        Msg::Delta(idx, text) => {
                            if winner.is_none() && running.contains_key(&idx) {
                                winner = Some(idx);
                                hedge_at = None;
                                abort_others(&mut running, idx);
                                announce(emit, &self.engines[idx].id(), &mut replaced, &mut shown);
                            }
                            if winner == Some(idx) {
                                shown.push_str(&text);
                                emit(TranslateEvent::Delta { id: RequestId(0), text });
                            }
                        }
                        Msg::Finished(idx, Ok(mut out)) => {
                            if running.remove(&idx).is_none() || winner.is_some_and(|w| w != idx) {
                                continue; // late answer from an aborted engine
                            }
                            let eid = self.engines[idx].id();
                            self.bench.forgive(&eid);
                            if winner.is_none() {
                                // A non-streaming engine answered in one go.
                                abort_others(&mut running, idx);
                                announce(emit, &eid, &mut replaced, &mut shown);
                                if !out.text.is_empty() {
                                    emit(TranslateEvent::Delta { id: RequestId(0), text: out.text.clone() });
                                }
                            } else if out.text.is_empty() {
                                out.text = std::mem::take(&mut shown);
                            }
                            return Ok(Finished { engine: eid, output: out });
                        }
                        Msg::Finished(idx, Err(e)) => {
                            if running.remove(&idx).is_none() {
                                continue;
                            }
                            if winner.is_some_and(|w| w != idx) {
                                continue;
                            }
                            tracing::debug!(engine = %self.engines[idx].id(), error = %e, "engine failed");
                            if e.benches() {
                                self.bench.punish(&self.engines[idx].id());
                            }
                            if winner == Some(idx) {
                                winner = None;
                                replaced = true;
                            }
                            errors.push((idx, e));
                        }
                    }
                }
                _ = sleep_until(hedge_at), if winner.is_none() && hedge_at.is_some() => {
                    start(&mut next, &mut running);
                    hedge_delay *= 2;
                    hedge_at = (next < order.len()).then(|| Instant::now() + hedge_delay);
                    continue;
                }
                _ = sleep_until(timeout_at), if timeout_at.is_some() => {
                    let now = Instant::now();
                    let late: Vec<usize> = running
                        .iter()
                        .filter(|(i, (_, t))| Some(**i) != winner && *t + self.policy.timeout <= now)
                        .map(|(i, _)| *i)
                        .collect();
                    for i in late {
                        if let Some((h, _)) = running.remove(&i) {
                            h.abort();
                        }
                        errors.push((i, EngineError::Timeout));
                    }
                }
            }

            // Nothing running and no winner: go on down the chain, or give up.
            if winner.is_none() && running.is_empty() {
                if next < order.len() {
                    start(&mut next, &mut running);
                    hedge_at = (next < order.len()).then(|| Instant::now() + hedge_delay);
                } else {
                    return Err(pick_error(&order, errors));
                }
            }
        }
    }
}

impl TranslateEvent {
    /// The chain builds events with a placeholder id; `run` stamps the real one.
    fn with_id(mut self, real: RequestId) -> Self {
        match &mut self {
            Self::Planned { id, .. }
            | Self::Started { id, .. }
            | Self::Delta { id, .. }
            | Self::Replace { id, .. }
            | Self::Done { id, .. }
            | Self::Error { id, .. } => *id = real,
        }
        self
    }
}

/// First text from a new winner: `Started`, or `Replace` (empty) when an
/// earlier winner's text is on screen and must go.
fn announce(emit: &mut (dyn FnMut(TranslateEvent) + Send), engine: &EngineId, replaced: &mut bool, shown: &mut String) {
    if *replaced {
        emit(TranslateEvent::Replace { id: RequestId(0), engine: engine.clone(), text: String::new() });
        *replaced = false;
    } else {
        emit(TranslateEvent::Started { id: RequestId(0), engine: engine.clone() });
    }
    shown.clear();
}

fn abort_others(running: &mut HashMap<usize, (AbortHandle, Instant)>, keep: usize) {
    running.retain(|i, (h, _)| {
        if *i == keep {
            true
        } else {
            h.abort();
            false
        }
    });
}

async fn sleep_until(at: Option<Instant>) {
    match at {
        Some(t) => tokio::time::sleep_until(t).await,
        None => std::future::pending().await,
    }
}

/// The error to show: the one from the user's highest engine, but a
/// timeout or network error only if nothing more specific happened.
fn pick_error(order: &[usize], mut errors: Vec<(usize, EngineError)>) -> EngineError {
    let rank = |i: usize| order.iter().position(|&o| o == i).unwrap_or(usize::MAX);
    errors.sort_by_key(|(i, e)| (matches!(e, EngineError::Timeout | EngineError::Network(_)), rank(*i)));
    errors.into_iter().next().map(|(_, e)| e).unwrap_or(EngineError::NoEngines)
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::FutureExt;

    /// A scripted engine: waits, streams pieces, then succeeds or fails.
    struct Fake {
        id: &'static str,
        wait: Duration,
        pieces: Vec<&'static str>,
        gap: Duration,
        fail: Option<EngineError>,
        network: bool,
    }

    impl Fake {
        fn new(id: &'static str, wait_ms: u64, pieces: &[&'static str]) -> Self {
            Self { id, wait: Duration::from_millis(wait_ms), pieces: pieces.to_vec(), gap: Duration::from_millis(10), fail: None, network: true }
        }
        fn failing(mut self, e: EngineError) -> Self {
            self.fail = Some(e);
            self
        }
    }

    impl Engine for Fake {
        fn id(&self) -> EngineId {
            EngineId::new(self.id)
        }
        fn needs_network(&self) -> bool {
            self.network
        }
        fn translate(self: Arc<Self>, _job: Arc<Job>, sink: Sink) -> futures_util::future::BoxFuture<'static, Result<Output, EngineError>> {
            async move {
                tokio::time::sleep(self.wait).await;
                for p in &self.pieces {
                    sink.send(*p);
                    tokio::time::sleep(self.gap).await;
                }
                match &self.fail {
                    Some(e) => Err(e.clone()),
                    None => Ok(Output::default()),
                }
            }
            .boxed()
        }
    }

    fn router(engines: Vec<Fake>) -> Router {
        Router {
            engines: engines.into_iter().map(|e| Arc::new(e) as Arc<dyn Engine>).collect(),
            policy: HedgePolicy::default(),
            bench: Arc::default(),
        }
    }

    fn job() -> Arc<Job> {
        Arc::new(Job { text: "hi".into(), source: "en".into(), target: "ru".into(), ..Job::default() })
    }

    async fn run(r: &Router) -> (Result<Finished, EngineError>, Vec<TranslateEvent>) {
        let mut events = Vec::new();
        let res = r.run(RequestId(7), job(), true, |e| events.push(e)).await;
        (res, events)
    }

    #[tokio::test(start_paused = true)]
    async fn fast_first_engine_wins_alone() {
        let r = router(vec![Fake::new("a", 100, &["При", "вет"]), Fake::new("b", 50, &["X"])]);
        let (res, ev) = run(&r).await;
        let f = res.unwrap();
        assert_eq!(f.engine.as_str(), "a");
        assert_eq!(f.output.text, "Привет");
        assert!(matches!(&ev[0], TranslateEvent::Started { engine, id } if engine.as_str() == "a" && id.0 == 7));
        assert!(matches!(ev.last(), Some(TranslateEvent::Done { text, .. }) if text == "Привет"));
    }

    #[tokio::test(start_paused = true)]
    async fn slow_first_engine_is_hedged() {
        // a needs 2 s, b starts at 600 ms and answers at 650 ms.
        let r = router(vec![Fake::new("a", 2000, &["A"]), Fake::new("b", 50, &["B"])]);
        let t0 = Instant::now();
        let (res, _) = run(&r).await;
        assert_eq!(res.unwrap().engine.as_str(), "b");
        assert!(t0.elapsed() < Duration::from_millis(800));
    }

    #[tokio::test(start_paused = true)]
    async fn failure_moves_on_at_once() {
        let r = router(vec![Fake::new("a", 10, &[]).failing(EngineError::RateLimited), Fake::new("b", 10, &["B"])]);
        let t0 = Instant::now();
        let (res, _) = run(&r).await;
        assert_eq!(res.unwrap().engine.as_str(), "b");
        assert!(t0.elapsed() < Duration::from_millis(100));
        assert!(r.bench.is_benched(&EngineId::new("a")));
    }

    #[tokio::test(start_paused = true)]
    async fn mid_stream_failure_replaces_text() {
        let r = router(vec![
            Fake::new("a", 10, &["half"]).failing(EngineError::Server("500".into())),
            Fake::new("b", 10, &["whole"]),
        ]);
        let (res, ev) = run(&r).await;
        assert_eq!(res.unwrap().output.text, "whole");
        assert!(ev.iter().any(|e| matches!(e, TranslateEvent::Replace { engine, .. } if engine.as_str() == "b")));
    }

    #[tokio::test(start_paused = true)]
    async fn all_failing_reports_the_top_engines_error() {
        let r = router(vec![
            Fake::new("a", 10, &[]).failing(EngineError::NoKey),
            Fake::new("b", 10, &[]).failing(EngineError::Network("dns".into())),
        ]);
        let (res, ev) = run(&r).await;
        assert_eq!(res.unwrap_err(), EngineError::NoKey);
        assert!(matches!(ev.last(), Some(TranslateEvent::Error { kind: EngineError::NoKey, .. })));
    }

    #[tokio::test(start_paused = true)]
    async fn silent_engine_times_out() {
        let mut r = router(vec![Fake::new("a", 60_000, &["A"])]);
        r.policy.timeout = Duration::from_secs(5);
        let (res, _) = run(&r).await;
        assert_eq!(res.unwrap_err(), EngineError::Timeout);
    }

    #[tokio::test(start_paused = true)]
    async fn no_fallback_uses_engine_zero_only() {
        let mut r = router(vec![Fake::new("a", 10, &[]).failing(EngineError::NoKey), Fake::new("b", 10, &["B"])]);
        r.policy.fallback = false;
        let (res, _) = run(&r).await;
        assert_eq!(res.unwrap_err(), EngineError::NoKey);
    }

    #[tokio::test(start_paused = true)]
    async fn offline_skips_cloud_engines() {
        let mut local = Fake::new("offline.opus", 10, &["L"]);
        local.network = false;
        let r = router(vec![Fake::new("google", 10, &["G"]), local]);
        let res = r.run(RequestId(1), job(), false, |_| {}).await;
        assert_eq!(res.unwrap().engine.as_str(), "offline.opus");
    }
}
