//! TranslateGemma and Granite Vision through llama.cpp's prebuilt
//! `llama-server`.
//!
//! A server starts on first use (127.0.0.1, a free port, no window, its
//! output in `runtime/llama/<model>.log`) and stops after 15 idle minutes,
//! when its model is switched off, or with the app (a kill-on-close Job
//! takes it down if the app dies). The two TranslateGemma sizes never run
//! together; Granite (screen reading) may run next to one of them.
//!
//! Translation goes to `/completion` as a raw prompt in TranslateGemma's own
//! template, streamed (SSE), so text appears as it is generated.
//!
//! GPU offload: all layers when the model, its KV cache and some working
//! memory fit in free VRAM; otherwise as many layers as fit.

use std::collections::HashMap;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use eventsource_stream::Eventsource;
use futures_util::StreamExt;
use hl_core::prompt::{self, ContextPair};
use hl_core::{EngineError, Sink};

use crate::catalog::{self, LlamaBackend};
use crate::sys::KillOnClose;
use crate::{Config, Device, Model};

/// Context window (prompt + answer).
const CTX: u32 = 4096;
/// Granite reads a whole screenshot: image tokens need room.
const VISION_CTX: u32 = 8192;
/// CUDA/Vulkan context, compute buffers and the output layer.
const VRAM_OVERHEAD: u64 = 900 << 20;
/// A server nobody used for this long is stopped (frees its memory).
const IDLE: Duration = Duration::from_secs(15 * 60);

struct Server {
    child: Child,
    port: u16,
    /// GPU layers and threads it was started with.
    setup: (u32, usize),
    last_used: Instant,
    _job: Option<KillOnClose>,
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

static SERVERS: LazyLock<Mutex<HashMap<Model, Server>>> = LazyLock::new(Default::default);
/// One start at a time (two requests must not start two servers).
static STARTING: LazyLock<tokio::sync::Mutex<()>> = LazyLock::new(Default::default);

fn http() -> &'static reqwest::Client {
    static HTTP: LazyLock<reqwest::Client> = LazyLock::new(|| {
        reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(3))
            .no_proxy()
            .build()
            .expect("HTTP client")
    });
    &HTTP
}

pub(crate) fn runtime_failed() -> EngineError {
    EngineError::Server("the local model server didn't start".into())
}

/// (layers, KV cache bytes per layer at `CTX`).
fn shape(m: Model) -> (u32, u64) {
    // n_kv_heads × head_dim × 2 (K and V) × 2 bytes × CTX.
    match m {
        Model::Gemma12 => (48, 8 * 256 * 2 * 2 * CTX as u64),
        Model::Granite => (40, 8 * 128 * 2 * 2 * VISION_CTX as u64),
        _ => (34, 4 * 256 * 2 * 2 * CTX as u64),
    }
}

/// How many layers fit in `free` bytes of VRAM; 999 = all (llama.cpp
/// clamps it), 0 = none.
pub fn fit_layers(free: u64, model: u64, layers: u32, kv_per_layer: u64) -> u32 {
    let avail = free.saturating_sub(VRAM_OVERHEAD);
    let per_layer = model / layers as u64 + kv_per_layer;
    if avail >= per_layer * layers as u64 { 999 } else { (avail / per_layer).min(layers as u64) as u32 }
}

fn gpu_layers(cfg: &Config, m: Model) -> u32 {
    let backend = catalog::runtime_info(&cfg.model_dir).map_or(LlamaBackend::Cpu, |i| i.backend);
    if cfg.device == Device::Cpu || !backend.uses_gpu() {
        return 0;
    }
    let Some(gpu) = crate::sys::gpu() else { return 0 };
    let free = crate::sys::free_vram().unwrap_or(gpu.vram * 9 / 10);
    let size = catalog::gguf(m).map_or(0, |f| f.size) + catalog::mmproj(m).map_or(0, |f| f.size);
    let (layers, kv) = shape(m);
    fit_layers(free, size, layers, kv)
}

/// Starts (or reuses) the server for `m`; returns its port.
async fn ensure(cfg: &Config, m: Model) -> Result<u16, EngineError> {
    let _one = STARTING.lock().await;
    let threads = cfg.thread_count();
    {
        let mut servers = SERVERS.lock().unwrap();
        if let Some(s) = servers.get_mut(&m) {
            let alive = matches!(s.child.try_wait(), Ok(None));
            if alive && s.setup.1 == threads && (s.setup.0 == 0) == (cfg.device == Device::Cpu) {
                s.last_used = Instant::now();
                return Ok(s.port);
            }
            servers.remove(&m);
        }
        // The TranslateGemma sizes don't share memory.
        if m != Model::Granite {
            servers.retain(|k, _| *k == Model::Granite);
        }
    }
    let (child, port, ngl, job) = spawn(cfg, m, threads)?;
    let mut server = Server { child, port, setup: (ngl, threads), last_used: Instant::now(), _job: job };

    // Loading a multi-gigabyte model from disk takes a while.
    let deadline = Instant::now() + Duration::from_secs(300);
    loop {
        if !matches!(server.child.try_wait(), Ok(None)) {
            return Err(runtime_failed());
        }
        let health = http().get(format!("http://127.0.0.1:{port}/health")).timeout(Duration::from_secs(2)).send().await;
        if health.is_ok_and(|r| r.status().is_success()) {
            break;
        }
        if Instant::now() > deadline {
            return Err(runtime_failed());
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    server.last_used = Instant::now();
    SERVERS.lock().unwrap().insert(m, server);
    reaper();
    Ok(port)
}

fn spawn(cfg: &Config, m: Model, threads: usize) -> Result<(Child, u16, u32, Option<KillOnClose>), EngineError> {
    #[cfg(windows)]
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    let models: &Path = &cfg.model_dir;
    let rt = catalog::runtime_dir(models);
    let gguf = catalog::gguf(m).ok_or_else(runtime_failed)?;
    let dir = catalog::model_dir(models, m);
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .and_then(|l| l.local_addr())
        .map(|a| a.port())
        .map_err(|_| runtime_failed())?;
    let ngl = gpu_layers(cfg, m);
    let log = std::fs::File::create(rt.join(format!("{}.log", m.id()))).ok();
    let (out, err) = match log.as_ref().and_then(|f| Some((f.try_clone().ok()?, f.try_clone().ok()?))) {
        Some((a, b)) => (Stdio::from(a), Stdio::from(b)),
        None => (Stdio::null(), Stdio::null()),
    };
    let mut cmd = Command::new(rt.join("llama-server.exe"));
    cmd.arg("-m").arg(dir.join(gguf.path));
    cmd.args(["--host", "127.0.0.1", "--port", &port.to_string(), "-np", "1"]);
    cmd.args(["-ngl", &ngl.to_string(), "-t", &threads.to_string()]);
    match catalog::mmproj(m) {
        // Vision: the model's own chat template handles the image.
        Some(proj) => {
            cmd.arg("--mmproj").arg(dir.join(proj.path));
            cmd.args(["-c", &VISION_CTX.to_string(), "--jinja"]);
        }
        // TranslateGemma: raw prompts (its bundled template rejects chat).
        None => {
            cmd.args(["-c", &CTX.to_string(), "--no-jinja"]);
        }
    }
    cmd.current_dir(&rt).stdin(Stdio::null()).stdout(out).stderr(err);
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    let child = cmd.spawn().map_err(|_| runtime_failed())?;
    let job = KillOnClose::new();
    if let Some(j) = &job {
        j.assign(&child);
    }
    tracing::info!(model = m.id(), ngl, threads, "llama-server started");
    Ok((child, port, ngl, job))
}

/// Stops servers nobody used for [`IDLE`] (one background task).
fn reaper() {
    static STARTED: std::sync::Once = std::sync::Once::new();
    STARTED.call_once(|| {
        tokio::spawn(async {
            loop {
                tokio::time::sleep(Duration::from_secs(60)).await;
                SERVERS.lock().unwrap().retain(|m, s| {
                    let keep = s.last_used.elapsed() < IDLE;
                    if !keep {
                        tracing::info!(model = m.id(), "llama-server idle, stopping");
                    }
                    keep
                });
            }
        });
    });
}

fn touch(m: Model) {
    if let Some(s) = SERVERS.lock().unwrap().get_mut(&m) {
        s.last_used = Instant::now();
    }
}

/// Translates with TranslateGemma, streaming text into `sink`.
#[allow(clippy::too_many_arguments)]
pub async fn translate(
    cfg: &Config,
    m: Model,
    text: &str,
    src: &str,
    tgt: &str,
    context: &[ContextPair],
    glossary: &[(String, String)],
    sink: &Sink,
) -> Result<String, EngineError> {
    let port = ensure(cfg, m).await?;
    let prompt = prompt::translategemma(src, tgt, text, context, glossary);
    let n_predict = (text.chars().count() * 2 + 64).clamp(128, 2048);
    let body = serde_json::json!({
        "prompt": prompt,
        "n_predict": n_predict,
        "temperature": 0.0,
        "top_k": 1,
        "cache_prompt": true,
        "stream": true,
        "stop": ["<end_of_turn>"],
    });
    let resp = http()
        .post(format!("http://127.0.0.1:{port}/completion"))
        .json(&body)
        .send()
        .await
        .map_err(|_| runtime_failed())?;
    if !resp.status().is_success() {
        return Err(crate::worker::failed());
    }
    let mut events = resp.bytes_stream().eventsource();
    let mut full = String::new();
    while let Some(ev) = events.next().await {
        let Ok(ev) = ev else { return Err(crate::worker::failed()) };
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&ev.data) else { continue };
        if let Some(t) = v["content"].as_str().filter(|t| !t.is_empty()) {
            // The first piece often starts with a newline.
            let t = if full.is_empty() { t.trim_start() } else { t };
            if !t.is_empty() {
                sink.send(t.to_owned());
                full.push_str(t);
            }
        }
        if v["stop"].as_bool() == Some(true) {
            break;
        }
    }
    touch(m);
    let out = prompt::clean_output(text, &full);
    if out.is_empty() {
        return Err(EngineError::Server("empty answer".into()));
    }
    Ok(out)
}

/// Asks a vision model about an image (PNG/JPEG bytes). Blocks until done.
pub async fn ask_image(cfg: &Config, m: Model, image: &[u8], mime: &str, question: &str) -> Result<String, EngineError> {
    use base64::Engine as _;
    let port = ensure(cfg, m).await?;
    let data = base64::engine::general_purpose::STANDARD.encode(image);
    let body = serde_json::json!({
        "messages": [{
            "role": "user",
            "content": [
                { "type": "image_url", "image_url": { "url": format!("data:{mime};base64,{data}") } },
                { "type": "text", "text": question },
            ],
        }],
        "temperature": 0.0,
        "max_tokens": 2048,
    });
    let resp = http()
        .post(format!("http://127.0.0.1:{port}/v1/chat/completions"))
        .timeout(Duration::from_secs(300))
        .json(&body)
        .send()
        .await
        .map_err(|_| runtime_failed())?;
    if !resp.status().is_success() {
        return Err(crate::worker::failed());
    }
    let v: serde_json::Value = resp.json().await.map_err(|_| crate::worker::failed())?;
    touch(m);
    Ok(v["choices"][0]["message"]["content"].as_str().unwrap_or_default().trim().to_owned())
}

/// Starts `m`'s server without asking anything.
pub async fn preload(cfg: &Config, m: Model) -> Result<(), EngineError> {
    ensure(cfg, m).await.map(|_| ())
}

pub fn running(m: Model) -> bool {
    SERVERS.lock().unwrap().get_mut(&m).is_some_and(|s| matches!(s.child.try_wait(), Ok(None)))
}

/// GPU layers of `m`'s running server (Settings shows "на видеокарте").
pub fn gpu_layers_of(m: Model) -> Option<u32> {
    SERVERS.lock().unwrap().get(&m).map(|s| s.setup.0)
}

pub fn stop(m: Model) {
    SERVERS.lock().unwrap().remove(&m);
}

pub fn stop_all() {
    SERVERS.lock().unwrap().clear();
}

#[cfg(test)]
mod tests {
    use super::*;

    const GB: u64 = 1 << 30;

    #[test]
    fn layer_fit() {
        let (l12, kv12) = shape(Model::Gemma12);
        let size12 = catalog::GEMMA12_FILE.size;
        // RTX 3060 12 GB, ~11 GB free: the whole 12B model fits.
        assert_eq!(fit_layers(11 * GB, size12, l12, kv12), 999);
        // RTX 4060 8 GB, ~7 GB free: part of it.
        let n = fit_layers(7 * GB, size12, l12, kv12);
        assert!((20..48).contains(&n), "{n}");
        let (l4, kv4) = shape(Model::Gemma4);
        assert_eq!(fit_layers(7 * GB, catalog::GEMMA4_FILE.size, l4, kv4), 999);
        assert_eq!(fit_layers(GB / 2, size12, l12, kv12), 0);
    }
}
