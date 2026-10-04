//! Translation history, kept locally in %APPDATA%\HeliLingo\history.json:
//! newest first, at most [`MAX_ENTRIES`]. Recording can be turned off in
//! Settings → General ("Save translation history"); clearing deletes the file.
//!
//! One process-wide store: every surface (popup, Quick, main window, Ultra,
//! image) calls [`add`]; the History panel reads it with [`list`]. Writes go
//! to disk on a background thread, coalesced, so the UI never waits on I/O.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Sender, channel};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::settings::ProviderKind;

pub const MAX_ENTRIES: usize = 1000;

/// Where a translation was made.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Mode {
    Popup,
    Quick,
    Main,
    Ultra,
    Image,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Entry {
    pub id: u64,
    pub source: String,
    pub translation: String,
    /// Source language code (detected or chosen), "auto" if unknown.
    pub from: String,
    pub to: String,
    pub provider: ProviderKind,
    pub mode: Mode,
    /// Unix time, seconds.
    pub time: u64,
    #[serde(default)]
    pub starred: bool,
}

struct Store {
    entries: Mutex<Vec<Entry>>,
    next_id: AtomicU64,
    enabled: AtomicBool,
    save_tx: Sender<()>,
}

fn path() -> Option<PathBuf> {
    Some(crate::settings::data_dir()?.join("history.json"))
}

fn store() -> &'static Store {
    static STORE: OnceLock<Store> = OnceLock::new();
    STORE.get_or_init(|| {
        let entries: Vec<Entry> = path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|json| serde_json::from_str(&json).ok())
            .unwrap_or_default();
        let next_id = entries.iter().map(|e| e.id).max().unwrap_or(0) + 1;
        let (save_tx, save_rx) = channel::<()>();
        std::thread::Builder::new()
            .name("history".into())
            .spawn(move || {
                while save_rx.recv().is_ok() {
                    // Coalesce bursts (e.g. several Ultra fragments) into one write.
                    std::thread::sleep(Duration::from_millis(500));
                    while save_rx.try_recv().is_ok() {}
                    write_file();
                }
            })
            .expect("spawn history thread");
        Store {
            entries: Mutex::new(entries),
            next_id: AtomicU64::new(next_id),
            enabled: AtomicBool::new(true),
            save_tx,
        }
    })
}

fn write_file() {
    let Some(p) = path() else { return };
    let json = {
        let entries = store().entries.lock().unwrap();
        if entries.is_empty() {
            let _ = std::fs::remove_file(&p);
            return;
        }
        serde_json::to_string(&*entries)
    };
    if let Ok(json) = json {
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        // Write-then-rename so a crash never leaves a truncated file.
        let tmp = p.with_extension("json.tmp");
        if std::fs::write(&tmp, json).is_ok() {
            let _ = std::fs::rename(&tmp, &p);
        }
    }
}

fn changed() {
    let _ = store().save_tx.send(());
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// Turns recording on or off (Settings → General). Existing entries stay.
pub fn set_enabled(on: bool) {
    store().enabled.store(on, Ordering::Relaxed);
}

pub fn is_enabled() -> bool {
    store().enabled.load(Ordering::Relaxed)
}

/// Records a translation (and counts it for Settings → Statistics). Repeating the newest entry (same text and
/// languages) just moves its time forward instead of adding a duplicate.
pub fn add(source: &str, translation: &str, from: &str, to: &str, provider: ProviderKind, mode: Mode) {
    let (source, translation) = (source.trim(), translation.trim());
    // Statistics count every translation, whether history is kept or not.
    let feature = match mode {
        Mode::Popup => crate::stats::Feature::DoubleCtrl,
        Mode::Quick => crate::stats::Feature::Quick,
        Mode::Main => crate::stats::Feature::Main,
        Mode::Image => crate::stats::Feature::Image,
        Mode::Ultra => crate::stats::Feature::Ultra,
    };
    let is_word = source.split_whitespace().count() == 1;
    crate::stats::record(feature, source, translation, from, to, is_word);
    if !is_enabled() || source.is_empty() || translation.is_empty() {
        return;
    }
    let s = store();
    {
        let mut entries = s.entries.lock().unwrap();
        if let Some(first) = entries.first_mut()
            && first.source == source
            && first.to == to
        {
            first.translation = translation.to_owned();
            first.provider = provider;
            first.time = now();
        } else {
            entries.insert(
                0,
                Entry {
                    id: s.next_id.fetch_add(1, Ordering::Relaxed),
                    source: source.to_owned(),
                    translation: translation.to_owned(),
                    from: from.to_owned(),
                    to: to.to_owned(),
                    provider,
                    mode,
                    time: now(),
                    starred: false,
                },
            );
            // Starred entries survive the cap; the oldest unstarred go first.
            while entries.len() > MAX_ENTRIES {
                match entries.iter().rposition(|e| !e.starred) {
                    Some(i) => {
                        entries.remove(i);
                    }
                    None => break,
                }
            }
        }
    }
    changed();
}

/// Entries matching `query` (case-insensitive, in source or translation),
/// newest first; `starred_only` keeps favourites only.
pub fn list(query: &str, starred_only: bool) -> Vec<Entry> {
    let q = query.trim().to_lowercase();
    store()
        .entries
        .lock()
        .unwrap()
        .iter()
        .filter(|e| !starred_only || e.starred)
        .filter(|e| {
            q.is_empty() || e.source.to_lowercase().contains(&q) || e.translation.to_lowercase().contains(&q)
        })
        .cloned()
        .collect()
}

pub fn len() -> usize {
    store().entries.lock().unwrap().len()
}

/// Stars or unstars an entry (the star in the main window and History).
pub fn set_starred(id: u64, starred: bool) {
    if let Some(e) = store().entries.lock().unwrap().iter_mut().find(|e| e.id == id) {
        e.starred = starred;
    }
    changed();
}

/// Stars the entry for this exact translation, adding it first if needed
/// (the star next to a result in the main window). Returns its id.
pub fn star_translation(source: &str, translation: &str, from: &str, to: &str, provider: ProviderKind, mode: Mode, starred: bool) -> Option<u64> {
    let was_enabled = is_enabled();
    // Starring is an explicit request to keep it, even with recording off.
    set_enabled(true);
    add(source, translation, from, to, provider, mode);
    set_enabled(was_enabled);
    let id = store()
        .entries
        .lock()
        .unwrap()
        .iter()
        .find(|e| e.source == source.trim() && e.to == to)
        .map(|e| e.id)?;
    set_starred(id, starred);
    Some(id)
}

/// Is this translation starred? (to show a filled star for a result).
pub fn is_starred(source: &str, to: &str) -> bool {
    store()
        .entries
        .lock()
        .unwrap()
        .iter()
        .any(|e| e.starred && e.source == source.trim() && e.to == to)
}

pub fn remove(id: u64) {
    store().entries.lock().unwrap().retain(|e| e.id != id);
    changed();
}

/// Deletes all entries, starred ones included, and the file.
pub fn clear() {
    store().entries.lock().unwrap().clear();
    changed();
}
