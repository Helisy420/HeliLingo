//! OPUS-MT, Argos and NLLB run in worker processes, one per model:
//! `helilingo.exe --offline-worker <spec>`.
//!
//! A worker starts the first time its model translates and keeps the model
//! in memory. Ending the worker is the only way to really free that memory:
//! ct2rs never releases a loaded translator on Windows (doing so deadlocks).
//! Switching a model off, deleting it, or changing the device / threads /
//! precision ends its worker.
//!
//! Protocol: one JSON object per line on stdin (`{"text","src","tgt"}`); on
//! stdout, `{"part": text}` for each finished sentence (or batch), then
//! `{"ok": full}` or `{"err": message}`. The worker exits when its stdin
//! closes; a kill-on-close Job takes it down with the app.
//! Calls here block: the engine runs them on Tokio's blocking pool.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{Receiver, RecvTimeoutError, channel};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::Duration;

use hl_core::EngineError;
use serde::{Deserialize, Serialize};

use crate::sys::KillOnClose;
use crate::{Config, Model};

/// The flag the app's `main` checks before anything else.
pub const FLAG: &str = "--offline-worker";

/// The first request also loads the model.
const FIRST_TIMEOUT: Duration = Duration::from_secs(180);
const TIMEOUT: Duration = Duration::from_secs(60);

/// What a worker is started with (its command-line argument, as JSON).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Spec {
    pub model: Model,
    pub config: Config,
}

#[derive(Serialize, Deserialize)]
struct Request {
    text: String,
    src: String,
    tgt: String,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Reply {
    /// A finished piece, as it should be appended.
    Part(String),
    Ok(String),
    Err(String),
}

struct Worker {
    spec: Spec,
    child: Child,
    stdin: ChildStdin,
    replies: Receiver<Reply>,
    /// Has answered once (the model is loaded).
    warm: bool,
    _job: Option<KillOnClose>,
}

impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

type Slot = Arc<Mutex<Option<Worker>>>;

/// One slot per model, so models translate in parallel.
static WORKERS: LazyLock<Mutex<HashMap<Model, Slot>>> = LazyLock::new(Default::default);

fn slot(m: Model) -> Slot {
    WORKERS.lock().unwrap().entry(m).or_default().clone()
}

pub(crate) fn failed() -> EngineError {
    EngineError::Server("the offline model failed".into())
}

fn spawn(spec: &Spec) -> Result<Worker, EngineError> {
    #[cfg(windows)]
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    let exe = std::env::current_exe().map_err(|_| failed())?;
    let arg = serde_json::to_string(spec).map_err(|_| failed())?;
    // CTranslate2's own messages go to a log next to the model.
    let log = std::fs::File::create(crate::catalog::model_dir(&spec.config.model_dir, spec.model).join("worker.log"))
        .map(Stdio::from)
        .unwrap_or_else(|_| Stdio::null());
    let mut cmd = Command::new(exe);
    cmd.args([FLAG, &arg]).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(log);
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    let mut child = cmd.spawn().map_err(|_| failed())?;
    let job = KillOnClose::new();
    if let Some(j) = &job {
        j.assign(&child);
    }
    let stdin = child.stdin.take().ok_or_else(failed)?;
    let stdout = child.stdout.take().ok_or_else(failed)?;
    let (tx, replies) = channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            let Ok(line) = line else { break };
            if let Ok(reply) = serde_json::from_str::<Reply>(&line)
                && tx.send(reply).is_err()
            {
                break;
            }
        }
    });
    Ok(Worker { spec: spec.clone(), child, stdin, replies, warm: false, _job: job })
}

/// Translates with `m`'s worker, starting it when needed; `part` gets each
/// sentence as it is done. Blocks.
pub fn translate(
    config: &Config,
    m: Model,
    text: &str,
    src: &str,
    tgt: &str,
    part: &mut dyn FnMut(&str),
) -> Result<String, EngineError> {
    let spec = Spec { model: m, config: config.clone() };
    let slot = slot(m);
    let mut guard = slot.lock().unwrap();
    // A different setup (device, threads, precision, folder), or it died.
    if guard.as_mut().is_some_and(|w| w.spec != spec || !matches!(w.child.try_wait(), Ok(None))) {
        *guard = None;
    }
    if guard.is_none() {
        tracing::info!(model = m.id(), "starting offline worker");
        *guard = Some(spawn(&spec)?);
    }
    let w = guard.as_mut().expect("worker just started");
    let line = serde_json::to_string(&Request { text: text.to_owned(), src: src.to_owned(), tgt: tgt.to_owned() })
        .map_err(|_| failed())?;
    if writeln!(w.stdin, "{line}").and_then(|_| w.stdin.flush()).is_err() {
        *guard = None;
        return Err(failed());
    }
    loop {
        let timeout = if w.warm { TIMEOUT } else { FIRST_TIMEOUT };
        match w.replies.recv_timeout(timeout) {
            Ok(Reply::Part(t)) => {
                w.warm = true;
                part(&t);
            }
            Ok(Reply::Ok(t)) => {
                w.warm = true;
                return Ok(t);
            }
            Ok(Reply::Err(e)) => {
                w.warm = true;
                return Err(if e == UNSUPPORTED { EngineError::UnsupportedPair } else { EngineError::Server(e) });
            }
            // Hung or died: end it; the next request starts a fresh one.
            Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => {
                *guard = None;
                return Err(failed());
            }
        }
    }
}

/// Starts `m`'s worker and loads its model without translating. Blocks.
pub fn preload(config: &Config, m: Model) -> Result<(), EngineError> {
    translate(config, m, "", "en", "ru", &mut |_| {}).map(|_| ())
}

/// Ends `m`'s worker (frees its memory).
pub fn stop(m: Model) {
    let slot = WORKERS.lock().unwrap().get(&m).cloned();
    if let Some(slot) = slot {
        *slot.lock().unwrap() = None;
    }
}

pub fn stop_all() {
    let slots: Vec<Slot> = WORKERS.lock().unwrap().values().cloned().collect();
    for slot in slots {
        *slot.lock().unwrap() = None;
    }
}

pub fn running(m: Model) -> bool {
    let slot = WORKERS.lock().unwrap().get(&m).cloned();
    slot.is_some_and(|s| s.lock().unwrap().as_mut().is_some_and(|w| matches!(w.child.try_wait(), Ok(None))))
}

/// The answer a worker gives for a pair its model lacks.
pub(crate) const UNSUPPORTED: &str = "unsupported pair";

/// The worker process: serves requests until stdin closes.
pub fn serve(arg: &str) {
    let Ok(spec) = serde_json::from_str::<Spec>(arg) else { return };
    let stdout = std::io::stdout();
    let send = |reply: &Reply| {
        let Ok(json) = serde_json::to_string(reply) else { return true };
        let mut out = stdout.lock();
        writeln!(out, "{json}").and_then(|_| out.flush()).is_ok()
    };
    for line in std::io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        let Ok(req) = serde_json::from_str::<Request>(&line) else { continue };
        let reply = match run(&spec, &req, &mut |p| {
            send(&Reply::Part(p.to_owned()));
        }) {
            Ok(t) => Reply::Ok(t),
            Err(e) => Reply::Err(e),
        };
        if !send(&reply) {
            break;
        }
    }
}

#[cfg(feature = "ct2")]
fn run(spec: &Spec, req: &Request, part: &mut dyn FnMut(&str)) -> Result<String, String> {
    if req.text.is_empty() {
        crate::ct2::preload(&spec.config, spec.model).map(|_| String::new())
    } else {
        crate::ct2::translate(&spec.config, spec.model, &req.text, &req.src, &req.tgt, part)
    }
}

#[cfg(not(feature = "ct2"))]
fn run(_spec: &Spec, _req: &Request, _part: &mut dyn FnMut(&str)) -> Result<String, String> {
    Err("this build has no CTranslate2".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_and_messages_round_trip() {
        let spec = Spec { model: Model::Nllb600, config: Config::default() };
        let json = serde_json::to_string(&spec).unwrap();
        assert_eq!(serde_json::from_str::<Spec>(&json).unwrap(), spec);
        assert_eq!(serde_json::to_string(&Reply::Ok("Привет".into())).unwrap(), r#"{"ok":"Привет"}"#);
        assert!(matches!(serde_json::from_str::<Reply>(r#"{"err":"x"}"#).unwrap(), Reply::Err(m) if m == "x"));
        assert_eq!(serde_json::to_string(&Reply::Part("Привет. ".into())).unwrap(), r#"{"part":"Привет. "}"#);
    }
}
