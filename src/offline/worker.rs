//! Tiers 1–4 (CTranslate2) run in worker processes, one per tier:
//! `helilingo.exe --offline-worker <spec>`.
//!
//! A worker starts the first time its tier translates (models load only
//! when used) and keeps its model in memory for the session. Ending the
//! worker is the only way to really free that memory: ct2rs never releases
//! a loaded translator on Windows (doing so deadlocks), so an in-process
//! model could never be unloaded. Switching a tier off, deleting it, or
//! changing the device / threads / precision ends its worker.
//!
//! Protocol: one JSON object per line on stdin (`{"text","src","tgt"}`),
//! one per line on stdout (`{"ok": text}` or `{"err": message}`). The worker
//! exits when its stdin closes; a kill-on-close Job takes it down with the
//! app.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{Receiver, RecvTimeoutError, channel};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::sys::KillOnClose;
use super::{MODEL_FAILED, OfflineConfig, RUNTIME_FAILED, Tier};
use crate::settings::{OfflineDevice, OfflinePrecision};
use crate::translate::Error;

/// The first request also loads the model.
const FIRST_TIMEOUT: Duration = Duration::from_secs(180);
const TIMEOUT: Duration = Duration::from_secs(60);

/// What a worker is started with (its command-line argument, as JSON).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Spec {
    pub tier: String,
    pub model_dir: PathBuf,
    pub device: OfflineDevice,
    pub threads: u32,
    pub precision: OfflinePrecision,
}

impl Spec {
    fn new(cfg: &OfflineConfig, tier: Tier) -> Self {
        Spec {
            tier: tier.id().to_owned(),
            model_dir: cfg.model_dir.clone(),
            device: cfg.device,
            threads: cfg.threads,
            precision: cfg.precision,
        }
    }

    fn config(&self) -> OfflineConfig {
        OfflineConfig {
            model_dir: self.model_dir.clone(),
            device: self.device,
            threads: self.threads,
            precision: self.precision,
            ..Default::default()
        }
    }
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

/// One slot per tier, so tiers translate in parallel.
static WORKERS: LazyLock<Mutex<HashMap<Tier, Slot>>> = LazyLock::new(Default::default);

fn slot(tier: Tier) -> Slot {
    WORKERS.lock().unwrap().entry(tier).or_default().clone()
}

fn spawn(spec: &Spec) -> Result<Worker, Error> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    let exe = std::env::current_exe().map_err(|_| Error::Other(RUNTIME_FAILED.into()))?;
    let arg = serde_json::to_string(spec).map_err(|_| Error::Other(RUNTIME_FAILED.into()))?;
    // CTranslate2's own messages go to a log next to the model.
    let log = std::fs::File::create(spec.model_dir.join(&spec.tier).join("worker.log"))
        .map(Stdio::from)
        .unwrap_or_else(|_| Stdio::null());
    let mut child = Command::new(exe)
        .args(["--offline-worker", &arg])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(log)
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(|_| Error::Other(RUNTIME_FAILED.into()))?;
    let job = KillOnClose::new();
    if let Some(j) = &job {
        j.assign(&child);
    }
    let stdin = child.stdin.take().ok_or_else(|| Error::Other(RUNTIME_FAILED.into()))?;
    let stdout = child.stdout.take().ok_or_else(|| Error::Other(RUNTIME_FAILED.into()))?;
    let (tx, replies) = channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            let Ok(line) = line else { break };
            if let Ok(reply) = serde_json::from_str::<Reply>(&line) {
                if tx.send(reply).is_err() {
                    break;
                }
            }
        }
    });
    Ok(Worker { spec: spec.clone(), child, stdin, replies, warm: false, _job: job })
}

/// Translates with `tier`'s worker, starting it when needed.
pub fn translate(cfg: &OfflineConfig, tier: Tier, text: &str, src: &str, tgt: &str) -> Result<String, Error> {
    let spec = Spec::new(cfg, tier);
    let slot = slot(tier);
    let mut guard = slot.lock().unwrap();
    // A different setup (device, threads, precision, folder): start over.
    if guard.as_mut().is_some_and(|w| w.spec != spec || !matches!(w.child.try_wait(), Ok(None))) {
        *guard = None;
    }
    if guard.is_none() {
        crate::timing(&format!("offline: start worker {}", tier.id()));
        *guard = Some(spawn(&spec)?);
    }
    let w = guard.as_mut().expect("worker just started");
    let line = serde_json::to_string(&Request { text: text.to_owned(), src: src.to_owned(), tgt: tgt.to_owned() })
        .map_err(|_| Error::Other(MODEL_FAILED.into()))?;
    let sent = writeln!(w.stdin, "{line}").and_then(|_| w.stdin.flush());
    if sent.is_err() {
        *guard = None;
        return Err(Error::Other(RUNTIME_FAILED.into()));
    }
    let timeout = if w.warm { TIMEOUT } else { FIRST_TIMEOUT };
    match w.replies.recv_timeout(timeout) {
        Ok(Reply::Ok(t)) => {
            w.warm = true;
            Ok(t)
        }
        Ok(Reply::Err(e)) => {
            w.warm = true;
            Err(Error::Other(e))
        }
        // Hung or died: end it; the next request starts a fresh one.
        Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => {
            *guard = None;
            Err(Error::Other(MODEL_FAILED.into()))
        }
    }
}

/// Starts `tier`'s worker and loads its model, without translating.
pub fn preload(cfg: &OfflineConfig, tier: Tier) -> Result<(), Error> {
    // An empty text loads the default pair and answers at once.
    translate(cfg, tier, "", "en", "ru").map(|_| ())
}

/// Ends `tier`'s worker (frees its memory).
pub fn stop(tier: Tier) {
    let slot = WORKERS.lock().unwrap().get(&tier).cloned();
    if let Some(slot) = slot {
        *slot.lock().unwrap() = None;
    }
}

/// Ends every worker.
pub fn stop_all() {
    let slots: Vec<Slot> = WORKERS.lock().unwrap().values().cloned().collect();
    for slot in slots {
        *slot.lock().unwrap() = None;
    }
}

/// Is `tier`'s worker running (its model loaded)?
pub fn running(tier: Tier) -> bool {
    let slot = WORKERS.lock().unwrap().get(&tier).cloned();
    slot.is_some_and(|s| s.lock().unwrap().as_mut().is_some_and(|w| matches!(w.child.try_wait(), Ok(None))))
}

/// The worker process: serves requests until stdin closes.
pub fn serve(arg: &str) {
    let Ok(spec) = serde_json::from_str::<Spec>(arg) else { return };
    let Some(tier) = Tier::from_id(&spec.tier) else { return };
    let cfg = spec.config();
    let stdout = std::io::stdout();
    for line in std::io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        let Ok(req) = serde_json::from_str::<Request>(&line) else { continue };
        let reply = if req.text.is_empty() {
            match super::ct2::preload(&cfg, tier) {
                Ok(()) => Reply::Ok(String::new()),
                Err(e) => Reply::Err(error_text(e)),
            }
        } else {
            match super::ct2::translate(&cfg, tier, &req.text, &req.src, &req.tgt) {
                Ok(t) => Reply::Ok(t),
                Err(e) => Reply::Err(error_text(e)),
            }
        };
        let Ok(json) = serde_json::to_string(&reply) else { continue };
        let mut out = stdout.lock();
        if writeln!(out, "{json}").and_then(|_| out.flush()).is_err() {
            break;
        }
    }
}

fn error_text(e: Error) -> String {
    match e {
        Error::Other(m) => m,
        _ => MODEL_FAILED.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_and_messages_round_trip() {
        let spec = Spec::new(&OfflineConfig::default(), Tier::Fast);
        let json = serde_json::to_string(&spec).unwrap();
        assert_eq!(serde_json::from_str::<Spec>(&json).unwrap(), spec);
        assert_eq!(spec.config().model_dir, spec.model_dir);
        let reply = serde_json::to_string(&Reply::Ok("Привет".into())).unwrap();
        assert_eq!(reply, r#"{"ok":"Привет"}"#);
        assert!(matches!(serde_json::from_str::<Reply>(r#"{"err":"x"}"#).unwrap(), Reply::Err(m) if m == "x"));
    }
}
