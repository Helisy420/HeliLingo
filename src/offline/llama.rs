//! Tiers 5–6: TranslateGemma through llama.cpp's prebuilt `llama-server`.
//!
//! The server starts on first use (127.0.0.1, a free port, no window, its
//! output in `runtime/llama/server.log`), stays warm for the session and
//! is stopped on app exit; a kill-on-close Job object takes it down if
//! the app dies. Prompts go to `/completion` as raw text (see `prompts`).
//!
//! GPU offload: all layers when the model, its KV cache and some working
//! memory fit in free VRAM; otherwise as many layers as fit.

use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use reqwest::blocking::Client;
use serde::Deserialize;

use super::catalog::{self, LlamaBackend};
use super::sys::KillOnClose;
use super::{ContextPair, MODEL_FAILED, OfflineConfig, RUNTIME_FAILED, Tier, prompts};
use crate::settings::OfflineDevice;
use crate::translate::Error;

/// Context window passed to the server (prompt + answer).
const CTX: u32 = 4096;
/// CUDA/Vulkan context, compute buffers and the output layer.
const VRAM_OVERHEAD: u64 = 900 << 20;

struct Server {
    child: Child,
    port: u16,
    tier: Tier,
    /// GPU layers and threads it was started with.
    setup: (u32, usize),
    _job: Option<KillOnClose>,
}

static SERVER: LazyLock<Mutex<Option<Server>>> = LazyLock::new(Default::default);

fn http() -> &'static Client {
    static HTTP: LazyLock<Client> = LazyLock::new(|| {
        Client::builder()
            .connect_timeout(Duration::from_secs(3))
            .timeout(Duration::from_secs(300))
            .no_proxy()
            .build()
            .expect("HTTP client")
    });
    &HTTP
}

/// (layers, KV cache bytes per layer at `CTX`) of the Gemma 3 models.
fn shape(tier: Tier) -> (u32, u64) {
    // n_kv_heads × head_dim × 2 (K and V) × 2 bytes × CTX.
    match tier {
        Tier::Heavy => (48, 8 * 256 * 2 * 2 * CTX as u64),
        _ => (34, 4 * 256 * 2 * 2 * CTX as u64),
    }
}

/// How many layers fit in `free` bytes of VRAM; `999` = all of them
/// (llama.cpp clamps it), `0` = none.
pub fn fit_layers(free: u64, model: u64, layers: u32, kv_per_layer: u64) -> u32 {
    let avail = free.saturating_sub(VRAM_OVERHEAD);
    let per_layer = model / layers as u64 + kv_per_layer;
    if avail >= per_layer * layers as u64 {
        999
    } else {
        (avail / per_layer).min(layers as u64) as u32
    }
}

fn gpu_layers(cfg: &OfflineConfig, tier: Tier, models: &Path) -> u32 {
    let backend = catalog::runtime_info(models).map_or(LlamaBackend::Cpu, |i| i.backend);
    if cfg.device == OfflineDevice::Cpu || !backend.uses_gpu() {
        return 0;
    }
    let Some(gpu) = super::sys::gpu() else { return 0 };
    let free = super::sys::free_vram().unwrap_or(gpu.vram * 9 / 10);
    let size = catalog::gguf(tier).map_or(0, |f| f.size);
    let (layers, kv) = shape(tier);
    fit_layers(free, size, layers, kv)
}

/// Starts (or reuses) the server for `tier`; returns its port.
fn ensure(cfg: &OfflineConfig, tier: Tier) -> Result<u16, Error> {
    let mut guard = SERVER.lock().unwrap();
    let threads = cfg.thread_count();
    if let Some(s) = guard.as_mut() {
        let alive = matches!(s.child.try_wait(), Ok(None));
        // Same tier: keep it (a new layer count would need a restart that
        // costs more than it saves; it's recomputed on the next start).
        if alive && s.tier == tier && s.setup.1 == threads && (s.setup.0 == 0) == (cfg.device == OfflineDevice::Cpu) {
            return Ok(s.port);
        }
    }
    if let Some(mut old) = guard.take() {
        let _ = old.child.kill();
        let _ = old.child.wait();
    }
    let s = start(cfg, tier, threads)?;
    let port = s.port;
    *guard = Some(s);
    Ok(port)
}

fn start(cfg: &OfflineConfig, tier: Tier, threads: usize) -> Result<Server, Error> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    let models = &cfg.model_dir;
    let rt = catalog::runtime_dir(models);
    let exe = rt.join("llama-server.exe");
    let gguf = catalog::gguf(tier).ok_or_else(|| Error::Other(MODEL_FAILED.into()))?;
    let model = catalog::tier_dir(models, tier).join(gguf.path);
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .and_then(|l| l.local_addr())
        .map(|a| a.port())
        .map_err(|_| Error::Other(RUNTIME_FAILED.into()))?;
    let ngl = gpu_layers(cfg, tier, models);
    let log = std::fs::File::create(rt.join("server.log")).ok();
    let (out, err) = match log.as_ref().and_then(|f| Some((f.try_clone().ok()?, f.try_clone().ok()?))) {
        Some((a, b)) => (Stdio::from(a), Stdio::from(b)),
        None => (Stdio::null(), Stdio::null()),
    };
    let child = Command::new(&exe)
        .arg("-m")
        .arg(&model)
        .args(["--host", "127.0.0.1", "--port", &port.to_string()])
        .args(["-c", &CTX.to_string(), "-np", "1", "--no-jinja", "-ngl", &ngl.to_string(), "-t", &threads.to_string()])
        .current_dir(&rt)
        .stdin(Stdio::null())
        .stdout(out)
        .stderr(err)
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(|_| Error::Other(RUNTIME_FAILED.into()))?;
    let job = KillOnClose::new();
    if let Some(j) = &job {
        j.assign(&child);
    }
    let mut server = Server { child, port, tier, setup: (ngl, threads), _job: job };

    // Loading a multi-gigabyte model from disk takes a while.
    let deadline = Instant::now() + Duration::from_secs(300);
    loop {
        if !matches!(server.child.try_wait(), Ok(None)) {
            return Err(Error::Other(RUNTIME_FAILED.into()));
        }
        if let Ok(r) = http().get(format!("http://127.0.0.1:{port}/health")).timeout(Duration::from_secs(2)).send()
            && r.status().is_success()
        {
            return Ok(server);
        }
        if Instant::now() > deadline {
            let _ = server.child.kill();
            return Err(Error::Other(RUNTIME_FAILED.into()));
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

pub fn translate(
    cfg: &OfflineConfig,
    tier: Tier,
    text: &str,
    src: &str,
    tgt: &str,
    context: &[ContextPair],
    glossary: &[(String, String)],
) -> Result<String, Error> {
    let port = ensure(cfg, tier)?;
    let prompt = prompts::translategemma(src, tgt, text, context, glossary);
    let n_predict = (text.chars().count() * 2 + 64).clamp(128, 2048);
    let body = serde_json::json!({
        "prompt": prompt,
        "n_predict": n_predict,
        "temperature": 0.0,
        "top_k": 1,
        "cache_prompt": true,
        "stop": ["<end_of_turn>"],
    });
    #[derive(Deserialize)]
    struct Resp {
        content: String,
    }
    let resp = http()
        .post(format!("http://127.0.0.1:{port}/completion"))
        .json(&body)
        .send()
        .map_err(|_| Error::Other(RUNTIME_FAILED.into()))?;
    if !resp.status().is_success() {
        return Err(Error::Other(MODEL_FAILED.into()));
    }
    let r: Resp = resp.json().map_err(|_| Error::Other(MODEL_FAILED.into()))?;
    let out = prompts::clean_output(&r.content);
    if out.is_empty() {
        return Err(Error::Other(crate::translate::EMPTY.into()));
    }
    Ok(out)
}

pub fn preload(cfg: &OfflineConfig, tier: Tier) -> Result<(), Error> {
    ensure(cfg, tier).map(|_| ())
}

/// The running server's tier and GPU layers (Settings page, CLI).
pub fn running() -> Option<(Tier, u32)> {
    SERVER.lock().unwrap().as_ref().map(|s| (s.tier, s.setup.0))
}

pub fn stop() {
    if let Some(mut s) = SERVER.lock().unwrap().take() {
        let _ = s.child.kill();
        let _ = s.child.wait();
    }
}

pub fn stop_if(tier: Tier) {
    if running().is_some_and(|(t, _)| t == tier) {
        stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GB: u64 = 1 << 30;

    #[test]
    fn layer_fit() {
        let (l12, kv12) = shape(Tier::Heavy);
        let size12 = catalog::GEMMA12_FILE.size;
        // RTX 3060 12 GB, ~11 GB free: the whole 12B model fits.
        assert_eq!(fit_layers(11 * GB, size12, l12, kv12), 999);
        // RTX 4060 8 GB, ~7 GB free: part of it.
        let n = fit_layers(7 * GB, size12, l12, kv12);
        assert!((20..48).contains(&n), "{n}");
        // 4B fits in 8 GB.
        let (l4, kv4) = shape(Tier::Medium);
        assert_eq!(fit_layers(7 * GB, catalog::GEMMA4_FILE.size, l4, kv4), 999);
        assert_eq!(fit_layers(GB / 2, size12, l12, kv12), 0);
    }
}
