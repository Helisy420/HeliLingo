//! Installing, cancelling and deleting models, with progress for the
//! Settings page. One background job per model; the page asks [`status`].

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex};

use serde::{Deserialize, Serialize};

use crate::catalog::{self, Component, LlamaBackend, MARKER, RuntimeInfo};
use crate::download::{self, DlError};
use crate::{Model, Runtime};

/// What the Models list shows for a model.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Status {
    /// `partial`: a cancelled download left files to resume from.
    NotInstalled { partial: bool, size: u64 },
    Downloading { done: u64, total: u64 },
    /// Size on disk (without the shared llama runtime).
    Installed { size: u64 },
    /// The last attempt failed: "network", "checksum", "disk", "space",
    /// "source" or "incomplete".
    Failed { error: &'static str },
}

struct Job {
    cancel: AtomicBool,
    done: AtomicU64,
    total: AtomicU64,
    running: AtomicBool,
    error: Mutex<Option<&'static str>>,
}

static JOBS: LazyLock<Mutex<HashMap<Model, Arc<Job>>>> = LazyLock::new(Default::default);

fn job(m: Model) -> Option<Arc<Job>> {
    JOBS.lock().unwrap().get(&m).cloned()
}

pub fn status(models: &Path, m: Model) -> Status {
    if let Some(j) = job(m) {
        if j.running.load(Ordering::Relaxed) {
            return Status::Downloading { done: j.done.load(Ordering::Relaxed), total: j.total.load(Ordering::Relaxed) };
        }
        if let Some(error) = *j.error.lock().unwrap() {
            return Status::Failed { error };
        }
    }
    if catalog::is_installed(models, m) {
        return Status::Installed { size: crate::dir_size(&catalog::model_dir(models, m)) };
    }
    Status::NotInstalled { partial: catalog::model_dir(models, m).exists(), size: catalog::download_size(models, m) }
}

/// A download is running.
pub fn any_running() -> bool {
    JOBS.lock().unwrap().values().any(|j| j.running.load(Ordering::Relaxed))
}

/// Starts installing `m` in the background (no-op while it runs). Call
/// from inside a Tokio runtime.
pub fn install(models: PathBuf, m: Model) {
    let j = Arc::new(Job {
        cancel: AtomicBool::new(false),
        done: AtomicU64::new(0),
        total: AtomicU64::new(catalog::download_size(&models, m)),
        running: AtomicBool::new(true),
        error: Mutex::new(None),
    });
    {
        let mut jobs = JOBS.lock().unwrap();
        if jobs.get(&m).is_some_and(|j| j.running.load(Ordering::Relaxed)) {
            return;
        }
        jobs.insert(m, j.clone());
    }
    tokio::spawn(async move {
        let r = run(&models, m, &j).await;
        match &r {
            Ok(()) | Err(DlError::Cancelled) => {}
            Err(e) => {
                tracing::warn!(model = m.id(), "install failed: {e}");
                *j.error.lock().unwrap() = Some(error_code(e));
            }
        }
        j.running.store(false, Ordering::Relaxed);
        if j.error.lock().unwrap().is_none() {
            JOBS.lock().unwrap().remove(&m);
        }
    });
}

/// Stops a running download; its files stay for a resume.
pub fn cancel(m: Model) {
    if let Some(j) = job(m) {
        j.cancel.store(true, Ordering::Relaxed);
    }
}

/// Clears a failed state (the row goes back to «Скачать»).
pub fn dismiss(m: Model) {
    let mut jobs = JOBS.lock().unwrap();
    if jobs.get(&m).is_some_and(|j| !j.running.load(Ordering::Relaxed)) {
        jobs.remove(&m);
    }
}

/// Removes the model's files (and the llama runtime when no llama model is
/// left). Cancels a running download first.
pub async fn delete(models: &Path, m: Model) -> std::io::Result<()> {
    if let Some(j) = job(m) {
        j.cancel.store(true, Ordering::Relaxed);
        for _ in 0..50 {
            if !j.running.load(Ordering::Relaxed) {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(40)).await;
        }
        JOBS.lock().unwrap().remove(&m);
    }
    crate::unload(m);
    let dir = catalog::model_dir(models, m);
    if dir.exists() {
        tokio::fs::remove_dir_all(&dir).await?;
    }
    let llama_left = Model::ALL
        .into_iter()
        .any(|t| t.runtime() == Runtime::Llama && catalog::model_dir(models, t).join(MARKER).is_file());
    if m.runtime() == Runtime::Llama && !llama_left {
        crate::llama::stop_all();
        let rt = catalog::runtime_dir(models);
        if rt.exists() {
            tokio::fs::remove_dir_all(rt).await?;
        }
    }
    Ok(())
}

fn error_code(e: &DlError) -> &'static str {
    match e {
        DlError::Cancelled => "cancelled",
        DlError::Status(404 | 401 | 403 | 410) => "source",
        DlError::Network | DlError::Status(_) => "network",
        DlError::Checksum => "checksum",
        DlError::Space => "space",
        DlError::Incomplete => "incomplete",
        DlError::Disk(_) => "disk",
    }
}

/// One file to fetch, and what to do with it.
struct Item {
    url: String,
    dest: PathBuf,
    size: Option<u64>,
    sha256: Option<String>,
    /// Unzip into this folder (and delete the zip); strip the top folder.
    unzip: Option<(PathBuf, bool)>,
}

/// The installed runtime is too old for `m` (Granite needs a recent build).
fn runtime_outdated(models: &Path, m: Model) -> bool {
    m == Model::Granite
        && catalog::runtime_info(models)
            .and_then(|i| catalog::build_number(&i.release))
            .is_some_and(|b| b < catalog::LLAMA_MIN_BUILD)
}

async fn run(models: &Path, m: Model, j: &Job) -> Result<(), DlError> {
    let http = download::client();
    let dir = catalog::model_dir(models, m);
    let rt_dir = catalog::runtime_dir(models);
    let mut items: Vec<Item> = Vec::new();
    let mut runtime: Option<RuntimeInfo> = None;

    let outdated = runtime_outdated(models, m);
    if m.runtime() == Runtime::Llama && (!catalog::runtime_installed(models) || outdated) {
        if outdated {
            crate::llama::stop_all();
            let _ = tokio::fs::remove_dir_all(&rt_dir).await;
        }
        let (info, assets) = llama_release(&http).await?;
        for a in assets {
            items.push(Item {
                dest: rt_dir.join("_dl").join(&a.name),
                url: a.browser_download_url,
                size: Some(a.size),
                sha256: a.digest.and_then(|d| d.strip_prefix("sha256:").map(str::to_owned)),
                unzip: Some((rt_dir.clone(), false)),
            });
        }
        runtime = Some(info);
    }
    for c in catalog::components(m) {
        match *c {
            Component::Hf { sub, files } => {
                for f in files {
                    let base = if sub.is_empty() { dir.clone() } else { dir.join(sub) };
                    items.push(Item {
                        url: f.url(),
                        dest: base.join(f.path),
                        size: Some(f.size),
                        sha256: f.sha256.map(str::to_owned),
                        unzip: None,
                    });
                }
            }
            Component::Argos { from, to, .. } => {
                let pair = format!("{from}_{to}");
                if dir.join(&pair).join("model").join("model.bin").is_file() {
                    continue;
                }
                let (url, size) = argos_link(&http, from, to).await;
                items.push(Item {
                    url,
                    dest: dir.join("_dl").join(format!("{pair}.argosmodel")),
                    size,
                    sha256: None,
                    unzip: Some((dir.join(&pair), true)),
                });
            }
        }
    }

    let total: u64 = items.iter().map(|i| i.size.unwrap_or(0)).sum::<u64>().max(1);
    j.total.store(total, Ordering::Relaxed);
    let need: u64 = items.iter().map(|i| i.size.unwrap_or(0) * if i.unzip.is_some() { 2 } else { 1 }).sum();
    if let Some(free) = crate::sys::free_disk(models)
        && free < need + (200 << 20)
    {
        return Err(DlError::Space);
    }

    let mut base = 0u64;
    for item in &items {
        download::fetch(&http, &item.url, &item.dest, item.size, item.sha256.as_deref(), &j.cancel, |n| {
            j.done.store(base + n, Ordering::Relaxed);
        })
        .await?;
        base += item.size.unwrap_or_else(|| std::fs::metadata(&item.dest).map(|m| m.len()).unwrap_or(0));
        if let Some((into, strip)) = item.unzip.clone() {
            let zip = item.dest.clone();
            tokio::task::spawn_blocking(move || download::unzip(&zip, &into, strip))
                .await
                .map_err(|e| DlError::Disk(e.to_string()))??;
            let _ = tokio::fs::remove_file(&item.dest).await;
        }
        if j.cancel.load(Ordering::Relaxed) {
            return Err(DlError::Cancelled);
        }
    }
    let _ = tokio::fs::remove_dir_all(dir.join("_dl")).await;

    if let Some(info) = runtime {
        let _ = tokio::fs::remove_dir_all(rt_dir.join("_dl")).await;
        if !rt_dir.join("llama-server.exe").is_file() {
            return Err(DlError::Incomplete);
        }
        write_json(&rt_dir.join(MARKER), &info)?;
    }
    if !files_present(models, m) {
        return Err(DlError::Incomplete);
    }
    write_json(&dir.join(MARKER), &serde_json::json!({ "model": m.id(), "installed": unix_now() }))?;
    Ok(())
}

/// The files each runtime needs, before the marker is written.
fn files_present(models: &Path, m: Model) -> bool {
    let dir = catalog::model_dir(models, m);
    match m {
        Model::Argos => ["en_ru", "ru_en"]
            .iter()
            .all(|p| dir.join(p).join("model").join("model.bin").is_file() && dir.join(p).join("sentencepiece.model").is_file()),
        _ => catalog::components(m).iter().all(|c| match c {
            Component::Hf { sub, files } => files.iter().all(|f| {
                let base = if sub.is_empty() { dir.clone() } else { dir.join(sub) };
                std::fs::metadata(base.join(f.path)).is_ok_and(|md| md.len() == f.size)
            }),
            Component::Argos { .. } => true,
        }),
    }
}

fn write_json(path: &Path, value: &impl Serialize) -> Result<(), DlError> {
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d).map_err(download::disk)?;
    }
    std::fs::write(path, serde_json::to_string_pretty(value).unwrap_or_default()).map_err(download::disk)
}

fn unix_now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// The package's link from the Argos index (fallback: the known 1.9 URL)
/// and its size from a HEAD request.
async fn argos_link(http: &reqwest::Client, from: &str, to: &str) -> (String, Option<u64>) {
    #[derive(Deserialize)]
    struct Pkg {
        from_code: String,
        to_code: String,
        #[serde(default)]
        links: Vec<String>,
    }
    let index = match http.get(catalog::ARGOS_INDEX).send().await {
        Ok(r) => r.json::<Vec<Pkg>>().await.ok(),
        Err(_) => None,
    };
    let url = index
        .and_then(|pkgs| {
            pkgs.into_iter()
                .find(|p| p.from_code == from && p.to_code == to)
                .and_then(|p| p.links.into_iter().find(|l| l.starts_with("https://")))
        })
        .unwrap_or_else(|| catalog::argos_fallback_url(from, to));
    let size = match http.head(&url).send().await {
        Ok(r) if r.status().is_success() => r.content_length().filter(|&n| n > 0),
        _ => None,
    };
    (url, size)
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    assets: Vec<Asset>,
}

#[derive(Deserialize, Clone)]
struct Asset {
    name: String,
    size: u64,
    browser_download_url: String,
    digest: Option<String>,
}

/// The newest llama.cpp release with the Windows build for this PC (CUDA →
/// Vulkan → CPU when a build is missing).
async fn llama_release(http: &reqwest::Client) -> Result<(RuntimeInfo, Vec<Asset>), DlError> {
    let resp = http
        .get(catalog::LLAMA_RELEASES)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|_| DlError::Network)?;
    if !resp.status().is_success() {
        return Err(DlError::Status(resp.status().as_u16()));
    }
    let releases: Vec<Release> = resp.json().await.map_err(|_| DlError::Status(502))?;
    let preferred = LlamaBackend::detect();
    for backend in [preferred, LlamaBackend::Vulkan, LlamaBackend::Cpu] {
        if backend == LlamaBackend::Vulkan && !crate::sys::has_vulkan() {
            continue;
        }
        for r in &releases {
            let found: Option<Vec<Asset>> = backend
                .assets()
                .iter()
                .map(|&p| r.assets.iter().find(|a| catalog::asset_matches(&a.name, p)).cloned())
                .collect();
            if let Some(assets) = found {
                let info = RuntimeInfo { backend, release: r.tag_name.clone(), assets: assets.iter().map(|a| a.name.clone()).collect() };
                return Ok((info, assets));
            }
        }
    }
    Err(DlError::Status(404))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn statuses_of_an_empty_folder() {
        let models = std::env::temp_dir().join(format!("hl-models-{}", std::process::id()));
        for m in Model::ALL {
            assert!(matches!(status(&models, m), Status::NotInstalled { partial: false, size } if size > 0));
        }
        // A marker alone isn't enough for a llama model: the runtime is missing.
        let d = catalog::model_dir(&models, Model::Gemma4);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join(MARKER), "{}").unwrap();
        assert!(matches!(status(&models, Model::Gemma4), Status::NotInstalled { partial: true, .. }));
        let d = catalog::model_dir(&models, Model::Opus);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join(MARKER), "{}").unwrap();
        assert!(matches!(status(&models, Model::Opus), Status::Installed { .. }));
        delete(&models, Model::Opus).await.unwrap();
        delete(&models, Model::Gemma4).await.unwrap();
        assert!(matches!(status(&models, Model::Opus), Status::NotInstalled { partial: false, .. }));
        let _ = std::fs::remove_dir_all(&models);
    }

    #[test]
    fn status_json() {
        let s = serde_json::to_string(&Status::Downloading { done: 1, total: 2 }).unwrap();
        assert_eq!(s, r#"{"state":"downloading","done":1,"total":2}"#);
        assert_eq!(error_code(&DlError::Status(404)), "source");
        assert_eq!(error_code(&DlError::Status(500)), "network");
    }
}
