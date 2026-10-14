//! Updates from GitHub (`Helisy420/HeliLingo`): `version.json` in the
//! repository says what the latest version is and where its exe is. The
//! exe is downloaded next to the running one, checked against the size and
//! SHA-256 from `version.json`, and swapped in: the running exe is renamed
//! aside (Windows allows that), the new one takes its name and starts, and
//! this one quits. Old copies are cleaned up on the next start.
//!
//! `version.json`:
//!
//! ```json
//! {
//!   "version": "2.5.0", "date": "2026-10-15",
//!   "notes": { "ru": "…", "en": "…" },
//!   "url": "https://github.com/…/releases/download/v2.5.0/helilingo.exe",
//!   "sha256": "…", "size": 67340288,
//!   "page": "https://github.com/…/releases/tag/v2.5.0",
//!   "beta": { …the same fields, optional… }
//! }
//! ```

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter, Manager};
use tokio::io::AsyncWriteExt;

use crate::settings::UpdateChannel;
use crate::state::AppState;

/// Where `version.json` lives (the repository's main branch).
pub const VERSION_URL: &str = "https://raw.githubusercontent.com/Helisy420/HeliLingo/main/version.json";
/// Checks while running, when «Проверять автоматически» is on.
const CHECK_EVERY: Duration = Duration::from_secs(24 * 60 * 60);

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Release {
    pub version: String,
    #[serde(default)]
    pub date: String,
    #[serde(default)]
    pub notes: Notes,
    pub url: String,
    pub sha256: String,
    #[serde(default)]
    pub size: u64,
    #[serde(default)]
    pub page: String,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Notes {
    #[serde(default)]
    pub ru: String,
    #[serde(default)]
    pub en: String,
}

#[derive(Deserialize)]
struct VersionFile {
    #[serde(flatten)]
    stable: Release,
    beta: Option<Release>,
}

/// What the About page shows.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Check {
    pub current: String,
    pub latest: Release,
    pub available: bool,
}

/// "2.5.0" → (2, 5, 0); "v2.6.0-beta.1" → (2, 6, 0) and a pre-release flag.
fn parse(v: &str) -> Option<((u64, u64, u64), bool)> {
    let v = v.trim().trim_start_matches('v');
    let (core, pre) = match v.split_once('-') {
        Some((c, _)) => (c, true),
        None => (v, false),
    };
    let mut it = core.split('.').map(|p| p.parse::<u64>().ok());
    let t = (it.next()??, it.next().flatten().unwrap_or(0), it.next().flatten().unwrap_or(0));
    Some((t, pre))
}

/// `latest` is newer than `current` (a pre-release is older than its release).
pub fn newer(latest: &str, current: &str) -> bool {
    match (parse(latest), parse(current)) {
        (Some((a, ap)), Some((b, bp))) => a > b || (a == b && bp && !ap),
        _ => false,
    }
}

pub async fn check(app: &AppState) -> Result<Check, String> {
    let channel = app.settings.read().unwrap().update_channel;
    let text = hl_providers::client()
        .get(VERSION_URL)
        .timeout(Duration::from_secs(15))
        .send()
        .await
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?
        .text()
        .await
        .map_err(|e| e.to_string())?;
    let file: VersionFile = serde_json::from_str(&text).map_err(|e| format!("version.json: {e}"))?;
    // Beta: the beta when it's newer than the stable one.
    let latest = match (channel, file.beta) {
        (UpdateChannel::Beta, Some(b)) if newer(&b.version, &file.stable.version) => b,
        _ => file.stable,
    };
    let current = env!("CARGO_PKG_VERSION").to_owned();
    let available = newer(&latest.version, &current);
    Ok(Check { current, latest, available })
}

#[tauri::command]
pub async fn update_check(state: tauri::State<'_, Arc<AppState>>) -> Result<Check, String> {
    check(&state).await
}

#[derive(Clone, Serialize)]
struct Progress {
    done: u64,
    total: u64,
}

/// Downloads, checks and swaps in the release, then restarts into it.
#[tauri::command]
pub async fn update_install(app: AppHandle, state: tauri::State<'_, Arc<AppState>>) -> Result<(), String> {
    let c = check(&state).await?;
    if !c.available {
        return Err("up to date".into());
    }
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let dir = exe.parent().ok_or("no folder")?.to_path_buf();
    let new = dir.join("helilingo.update.exe");
    let resp = hl_providers::client()
        .get(&c.latest.url)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?;
    let total = resp.content_length().unwrap_or(c.latest.size);
    let mut file = tokio::fs::File::create(&new).await.map_err(|e| e.to_string())?;
    let mut hash = Sha256::new();
    let mut done = 0u64;
    let mut body = resp.bytes_stream();
    let mut last = std::time::Instant::now();
    while let Some(chunk) = body.next().await {
        let chunk = chunk.map_err(|e| e.to_string())?;
        hash.update(&chunk);
        file.write_all(&chunk).await.map_err(|e| e.to_string())?;
        done += chunk.len() as u64;
        if last.elapsed() > Duration::from_millis(150) {
            last = std::time::Instant::now();
            let _ = app.emit("update:progress", Progress { done, total });
        }
    }
    file.flush().await.map_err(|e| e.to_string())?;
    drop(file);
    let _ = app.emit("update:progress", Progress { done, total });
    let digest: String = hash.finalize().iter().map(|b| format!("{b:02x}")).collect();
    if (c.latest.size > 0 && done != c.latest.size) || !digest.eq_ignore_ascii_case(c.latest.sha256.trim()) {
        let _ = std::fs::remove_file(&new);
        return Err("checksum".into());
    }
    // The running exe steps aside; the new one takes its name.
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let aside = dir.join(format!("helilingo.old{stamp}.exe"));
    std::fs::rename(&exe, &aside).map_err(|e| e.to_string())?;
    if let Err(e) = std::fs::rename(&new, &exe) {
        let _ = std::fs::rename(&aside, &exe);
        return Err(e.to_string());
    }
    tracing::info!(version = %c.latest.version, "updated, restarting");
    crate::api::shutdown();
    hl_offline::shutdown();
    std::process::Command::new(&exe).arg("--updated").spawn().map_err(|e| e.to_string())?;
    app.exit(0);
    Ok(())
}

/// On start: old copies left by an update go; with «Проверять
/// автоматически» on, a check now and once a day — the About page and the
/// tray hear `update:available`.
pub fn start(app: &AppHandle) {
    if let Some(dir) = std::env::current_exe().ok().and_then(|e| e.parent().map(PathBuf::from)) {
        // Renamed by an update, not by hand: "helilingo.old<digits>.exe".
        let stale = |n: &str| {
            n.strip_prefix("helilingo.old")
                .and_then(|r| r.strip_suffix(".exe"))
                .is_some_and(|d| !d.is_empty() && d.bytes().all(|b| b.is_ascii_digit()))
        };
        if let Ok(list) = std::fs::read_dir(&dir) {
            for e in list.flatten() {
                if e.file_name().to_str().is_some_and(stale) {
                    let _ = std::fs::remove_file(e.path());
                }
            }
        }
        let _ = std::fs::remove_file(dir.join("helilingo.update.exe"));
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(20)).await;
        loop {
            let state = app.state::<Arc<AppState>>().inner().clone();
            if state.settings.read().unwrap().auto_update
                && let Ok(c) = check(&state).await
                && c.available
            {
                let _ = app.emit("update:available", &c);
            }
            tokio::time::sleep(CHECK_EVERY).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions() {
        assert!(newer("2.5.1", "2.5.0"));
        assert!(newer("v2.6.0", "2.5.9"));
        assert!(newer("3.0", "2.99.99"));
        assert!(!newer("2.5.0", "2.5.0"));
        assert!(!newer("2.4.9", "2.5.0"));
        assert!(newer("2.6.0", "2.6.0-beta.2"));
        assert!(!newer("2.6.0-beta.1", "2.6.0"));
        assert!(!newer("garbage", "2.5.0"));
    }

    #[test]
    fn version_file_parses() {
        let f: VersionFile = serde_json::from_str(
            r#"{"version":"2.5.0","url":"https://x/helilingo.exe","sha256":"ab","notes":{"ru":"р","en":"e"},
                "beta":{"version":"2.6.0-beta.1","url":"https://x/b.exe","sha256":"cd"}}"#,
        )
        .unwrap();
        assert_eq!(f.stable.version, "2.5.0");
        assert_eq!(f.beta.unwrap().version, "2.6.0-beta.1");
    }
}
