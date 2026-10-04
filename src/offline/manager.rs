//! Installing, cancelling and deleting tiers, with progress for the
//! Settings page. One background job per tier; the page polls [`status`].

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex};

use serde::Deserialize;

use super::catalog::{self, Component, LlamaBackend, MARKER, RuntimeInfo};
use super::download::{self, DlError};
use super::{Runtime, Tier};

/// What the Models list shows for a tier.
#[derive(Clone, Debug, PartialEq)]
pub enum Status {
    /// `partial`: a cancelled download left files to resume from.
    NotInstalled { partial: bool },
    Downloading { done: u64, total: u64 },
    /// Size on disk (without the shared llama runtime).
    Installed { size: u64 },
    /// The last attempt failed (English key, shown through `tr_text`).
    Failed(String),
}

// User-visible errors (Russian in i18n's OFFLINE table).
pub const ERR_NETWORK: &str = "Download failed: no connection";
pub const ERR_CHECKSUM: &str = "The file is damaged (checksum mismatch); try again";
pub const ERR_DISK: &str = "Can’t write to the model folder";
pub const ERR_SPACE: &str = "Not enough free disk space";
pub const ERR_SOURCE: &str = "The download source is unavailable";
pub const ERR_FILES: &str = "The downloaded model is incomplete";

struct Job {
    cancel: AtomicBool,
    done: AtomicU64,
    total: AtomicU64,
    running: AtomicBool,
    error: Mutex<Option<String>>,
}

static JOBS: LazyLock<Mutex<HashMap<Tier, Arc<Job>>>> = LazyLock::new(Default::default);

fn job(tier: Tier) -> Option<Arc<Job>> {
    JOBS.lock().unwrap().get(&tier).cloned()
}

pub fn status(models: &Path, tier: Tier) -> Status {
    if let Some(j) = job(tier) {
        if j.running.load(Ordering::Relaxed) {
            return Status::Downloading { done: j.done.load(Ordering::Relaxed), total: j.total.load(Ordering::Relaxed) };
        }
        if let Some(e) = j.error.lock().unwrap().clone() {
            return Status::Failed(e);
        }
    }
    if catalog::is_installed(models, tier) {
        return Status::Installed { size: super::dir_size(&catalog::tier_dir(models, tier)) };
    }
    Status::NotInstalled { partial: catalog::tier_dir(models, tier).exists() }
}

/// A download is running (the page keeps repainting).
pub fn any_running() -> bool {
    JOBS.lock().unwrap().values().any(|j| j.running.load(Ordering::Relaxed))
}

/// Starts installing `tier` in the background (no-op while it runs).
pub fn install(models: PathBuf, tier: Tier) {
    let j = Arc::new(Job {
        cancel: AtomicBool::new(false),
        done: AtomicU64::new(0),
        total: AtomicU64::new(catalog::download_size(&models, tier)),
        running: AtomicBool::new(true),
        error: Mutex::new(None),
    });
    {
        let mut jobs = JOBS.lock().unwrap();
        if jobs.get(&tier).is_some_and(|j| j.running.load(Ordering::Relaxed)) {
            return;
        }
        jobs.insert(tier, j.clone());
    }
    std::thread::spawn(move || {
        let r = run(&models, tier, &j, |_| {});
        match r {
            Ok(()) | Err(DlError::Cancelled) => {}
            Err(e) => *j.error.lock().unwrap() = Some(error_text(&e).to_owned()),
        }
        j.running.store(false, Ordering::Relaxed);
        if j.error.lock().unwrap().is_none() {
            JOBS.lock().unwrap().remove(&tier);
        }
    });
}

/// Stops a running download; its files stay for a resume.
pub fn cancel(tier: Tier) {
    if let Some(j) = job(tier) {
        j.cancel.store(true, Ordering::Relaxed);
    }
}

/// Clears a failed state (the row goes back to "Download").
pub fn dismiss(tier: Tier) {
    let mut jobs = JOBS.lock().unwrap();
    if jobs.get(&tier).is_some_and(|j| !j.running.load(Ordering::Relaxed)) {
        jobs.remove(&tier);
    }
}

/// Removes the tier's files (and the llama runtime when no llama tier is
/// left). Cancels a running download first.
pub fn delete(models: &Path, tier: Tier) -> std::io::Result<()> {
    if let Some(j) = job(tier) {
        j.cancel.store(true, Ordering::Relaxed);
        // The job notices within one chunk; give it a moment to let go of files.
        for _ in 0..50 {
            if !j.running.load(Ordering::Relaxed) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(40));
        }
        JOBS.lock().unwrap().remove(&tier);
    }
    super::unload(tier);
    let dir = catalog::tier_dir(models, tier);
    if dir.exists() {
        std::fs::remove_dir_all(&dir)?;
    }
    let llama_left = Tier::ALL
        .into_iter()
        .any(|t| t.runtime() == Runtime::Llama && catalog::tier_dir(models, t).join(MARKER).is_file());
    if tier.runtime() == Runtime::Llama && !llama_left {
        super::llama::stop();
        let rt = catalog::runtime_dir(models);
        if rt.exists() {
            std::fs::remove_dir_all(rt)?;
        }
    }
    Ok(())
}

/// Installs `tier` on this thread (the CLI): prints progress via `report`.
pub fn install_blocking(models: &Path, tier: Tier, report: impl FnMut(u64, u64)) -> Result<(), String> {
    let j = Job {
        cancel: AtomicBool::new(false),
        done: AtomicU64::new(0),
        total: AtomicU64::new(catalog::download_size(models, tier)),
        running: AtomicBool::new(true),
        error: Mutex::new(None),
    };
    let mut report = report;
    run(models, tier, &j, |j| report(j.done.load(Ordering::Relaxed), j.total.load(Ordering::Relaxed)))
        .map_err(|e| format!("{} ({e})", error_text(&e)))
}

pub fn error_text(e: &DlError) -> &'static str {
    match e {
        DlError::Cancelled => "cancelled",
        DlError::Network => ERR_NETWORK,
        DlError::Status(404 | 401 | 403 | 410) => ERR_SOURCE,
        DlError::Status(_) => ERR_NETWORK,
        DlError::Checksum => ERR_CHECKSUM,
        DlError::Disk(m) if m == "space" => ERR_SPACE,
        DlError::Disk(m) if m == "files" => ERR_FILES,
        DlError::Disk(_) => ERR_DISK,
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

fn run(models: &Path, tier: Tier, j: &Job, mut tick: impl FnMut(&Job)) -> Result<(), DlError> {
    let http = download::client();
    let dir = catalog::tier_dir(models, tier);
    let mut items: Vec<Item> = Vec::new();
    let mut runtime: Option<RuntimeInfo> = None;
    let rt_dir = catalog::runtime_dir(models);

    if tier.runtime() == Runtime::Llama && !catalog::runtime_installed(models) {
        let (info, assets) = llama_release(&http)?;
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
    for c in catalog::components(tier) {
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
                let (url, size) = argos_link(&http, from, to);
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

    // Totals from what the sources say now (Argos and llama sizes).
    let total: u64 = items.iter().map(|i| i.size.unwrap_or(0)).sum::<u64>().max(1);
    j.total.store(total, Ordering::Relaxed);
    let need = items
        .iter()
        .map(|i| i.size.unwrap_or(0) * if i.unzip.is_some() { 2 } else { 1 })
        .sum::<u64>();
    if let Some(free) = super::sys::free_disk(models)
        && free < need + (200 << 20)
    {
        return Err(DlError::Disk("space".into()));
    }

    let mut base = 0u64;
    for item in &items {
        download::fetch(&http, &item.url, &item.dest, item.size, item.sha256.as_deref(), &j.cancel, |n| {
            j.done.store(base + n, Ordering::Relaxed);
            tick(j);
        })?;
        base += item.size.unwrap_or_else(|| std::fs::metadata(&item.dest).map(|m| m.len()).unwrap_or(0));
        if let Some((into, strip)) = &item.unzip {
            download::unzip(&item.dest, into, *strip)?;
            let _ = std::fs::remove_file(&item.dest);
        }
        if j.cancel.load(Ordering::Relaxed) {
            return Err(DlError::Cancelled);
        }
    }
    let _ = std::fs::remove_dir_all(dir.join("_dl"));

    if let Some(info) = runtime {
        let _ = std::fs::remove_dir_all(rt_dir.join("_dl"));
        if !rt_dir.join("llama-server.exe").is_file() {
            return Err(DlError::Disk("files".into()));
        }
        write_json(&rt_dir.join(MARKER), &info)?;
    }
    if !files_present(models, tier) {
        return Err(DlError::Disk("files".into()));
    }
    write_json(
        &dir.join(MARKER),
        &serde_json::json!({ "tier": tier.id(), "installed": unix_now() }),
    )?;
    Ok(())
}

/// The files each runtime needs, before the marker is written.
fn files_present(models: &Path, tier: Tier) -> bool {
    let dir = catalog::tier_dir(models, tier);
    match tier {
        Tier::SuperFast => ["en_ru", "ru_en"]
            .iter()
            .all(|p| dir.join(p).join("model").join("model.bin").is_file() && dir.join(p).join("sentencepiece.model").is_file()),
        _ => catalog::components(tier).iter().all(|c| match c {
            Component::Hf { sub, files } => files.iter().all(|f| {
                let base = if sub.is_empty() { dir.clone() } else { dir.join(sub) };
                std::fs::metadata(base.join(f.path)).is_ok_and(|m| m.len() == f.size)
            }),
            Component::Argos { .. } => true,
        }),
    }
}

fn write_json(path: &Path, value: &impl serde::Serialize) -> Result<(), DlError> {
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d).map_err(|e| DlError::Disk(e.to_string()))?;
    }
    let json = serde_json::to_string_pretty(value).unwrap_or_default();
    std::fs::write(path, json).map_err(|e| DlError::Disk(e.to_string()))
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// The package's link from the Argos index (fallback: the known v1.9 URL),
/// and its size from a HEAD request.
fn argos_link(http: &reqwest::blocking::Client, from: &str, to: &str) -> (String, Option<u64>) {
    #[derive(Deserialize)]
    struct Pkg {
        from_code: String,
        to_code: String,
        #[serde(default)]
        links: Vec<String>,
    }
    let url = http
        .get(catalog::ARGOS_INDEX)
        .send()
        .ok()
        .and_then(|r| r.json::<Vec<Pkg>>().ok())
        .and_then(|pkgs| {
            pkgs.into_iter()
                .find(|p| p.from_code == from && p.to_code == to)
                .and_then(|p| p.links.into_iter().find(|l| l.starts_with("https://")))
        })
        .unwrap_or_else(|| catalog::argos_fallback_url(from, to));
    let size = http
        .head(&url)
        .send()
        .ok()
        .filter(|r| r.status().is_success())
        .and_then(|r| r.content_length())
        .filter(|&n| n > 0);
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
fn llama_release(http: &reqwest::blocking::Client) -> Result<(RuntimeInfo, Vec<Asset>), DlError> {
    let resp = http
        .get(catalog::LLAMA_RELEASES)
        .header("Accept", "application/vnd.github+json")
        .send()
        .map_err(|_| DlError::Network)?;
    if !resp.status().is_success() {
        return Err(DlError::Status(resp.status().as_u16()));
    }
    let releases: Vec<Release> = resp.json().map_err(|_| DlError::Status(502))?;
    let preferred = LlamaBackend::detect();
    let order = [preferred, LlamaBackend::Vulkan, LlamaBackend::Cpu];
    for backend in order {
        if backend == LlamaBackend::Vulkan && !super::sys::has_vulkan() {
            continue;
        }
        for r in &releases {
            let found: Option<Vec<Asset>> = backend
                .assets()
                .iter()
                .map(|&p| r.assets.iter().find(|a| catalog::asset_matches(&a.name, p)).cloned())
                .collect();
            if let Some(assets) = found {
                let info = RuntimeInfo {
                    backend,
                    release: r.tag_name.clone(),
                    assets: assets.iter().map(|a| a.name.clone()).collect(),
                };
                return Ok((info, assets));
            }
        }
    }
    Err(DlError::Status(404))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statuses_of_an_empty_folder() {
        let models = std::env::temp_dir().join(format!("hl-models-{}", std::process::id()));
        for t in Tier::ALL {
            assert_eq!(status(&models, t), Status::NotInstalled { partial: false });
        }
        // A marker alone isn't enough for a llama tier: the runtime is missing.
        let d = catalog::tier_dir(&models, Tier::Medium);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join(MARKER), "{}").unwrap();
        assert_eq!(status(&models, Tier::Medium), Status::NotInstalled { partial: true });
        let d = catalog::tier_dir(&models, Tier::SuperMegaFast);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join(MARKER), "{}").unwrap();
        assert!(matches!(status(&models, Tier::SuperMegaFast), Status::Installed { .. }));
        delete(&models, Tier::SuperMegaFast).unwrap();
        delete(&models, Tier::Medium).unwrap();
        assert_eq!(status(&models, Tier::SuperMegaFast), Status::NotInstalled { partial: false });
        let _ = std::fs::remove_dir_all(&models);
    }

    #[test]
    fn errors_have_russian() {
        crate::i18n::set_lang("ru");
        for e in [ERR_NETWORK, ERR_CHECKSUM, ERR_DISK, ERR_SPACE, ERR_SOURCE, ERR_FILES] {
            assert_ne!(crate::i18n::tr_text(e), e);
        }
    }
}
