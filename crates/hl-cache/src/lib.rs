//! HeliLingo's local data, in one redb file (`%APPDATA%\HeliLingo\helilingo.redb`):
//!
//! - [`Cache`]: finished translations. A moka cache in RAM answers repeats
//!   in microseconds; redb keeps them across restarts. Disk writes run on a
//!   blocking thread after the answer is shown.
//! - [`history`]: what was translated, newest first, with stars.
//! - [`stats`]: counters for the Statistics page.
//!
//! Keys are a stable FNV-1a hash of everything that changes the answer
//! (text, languages, engine choice, prompt version); the stored entry keeps
//! the full text, so a hash collision can't return a wrong translation.

pub mod history;
pub mod stats;

use std::path::Path;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use redb::{Database, ReadableDatabase, ReadableTable, ReadableTableMetadata, TableDefinition};
use serde::{Deserialize, Serialize};

const TRANSLATIONS: TableDefinition<u64, &[u8]> = TableDefinition::new("translations");
/// Disk cache size; the oldest entries go first.
const DISK_ENTRIES: u64 = 20_000;
const RAM_ENTRIES: u64 = 2_000;

/// The open database, shared by the cache, history and stats.
pub struct Store {
    db: Database,
}

impl Store {
    pub fn open(path: &Path) -> Result<Arc<Store>, String> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let db = Database::create(path).map_err(|e| e.to_string())?;
        // Create every table once, so reads never meet a missing one.
        let tx = db.begin_write().map_err(|e| e.to_string())?;
        tx.open_table(TRANSLATIONS).map_err(|e| e.to_string())?;
        tx.open_table(history::TABLE).map_err(|e| e.to_string())?;
        tx.open_table(stats::TABLE).map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())?;
        Ok(Arc::new(Store { db }))
    }

    pub(crate) fn db(&self) -> &Database {
        &self.db
    }
}

/// What the cache key is made of.
#[derive(Clone, Copy, Debug)]
pub struct Key<'a> {
    pub text: &'a str,
    pub source: &'a str,
    pub target: &'a str,
    /// Engine picked in the window, or "" for the normal chain.
    pub engine: &'a str,
    /// Bumped when prompts or post-processing change.
    pub version: u32,
}

impl Key<'_> {
    pub fn hash(&self) -> u64 {
        let mut h = Fnv::new();
        for part in [self.text, self.source, self.target, self.engine] {
            h.write(part.as_bytes());
            h.write(&[0xff]);
        }
        h.write(&self.version.to_le_bytes());
        h.0
    }
}

/// A finished translation.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    /// The source text (checked on read).
    pub source_text: String,
    pub text: String,
    pub engine: String,
    pub source_lang: Option<String>,
    pub alternatives: Vec<String>,
    /// Dictionary groups (absent in entries written before they existed).
    #[serde(default)]
    pub dict: Vec<hl_core::DictGroup>,
    #[serde(default)]
    pub transcription: Option<String>,
    #[serde(default)]
    pub examples: Vec<String>,
    /// Unix seconds.
    pub created: u64,
}

#[derive(Clone)]
pub struct Cache {
    ram: moka::future::Cache<u64, Arc<Entry>>,
    store: Arc<Store>,
}

impl Cache {
    pub fn new(store: Arc<Store>) -> Self {
        Self { ram: moka::future::Cache::new(RAM_ENTRIES), store }
    }

    /// RAM first, then disk (a disk hit is copied into RAM).
    pub async fn get(&self, key: &Key<'_>) -> Option<Arc<Entry>> {
        let h = key.hash();
        if let Some(e) = self.ram.get(&h).await {
            return (e.source_text == key.text).then_some(e);
        }
        let store = self.store.clone();
        let entry = tokio::task::spawn_blocking(move || read(&store, h)).await.ok().flatten()?;
        if entry.source_text != key.text {
            return None;
        }
        let entry = Arc::new(entry);
        self.ram.insert(h, entry.clone()).await;
        Some(entry)
    }

    /// Into RAM now, onto disk in the background.
    pub async fn put(&self, key: &Key<'_>, entry: Entry) {
        let h = key.hash();
        let entry = Arc::new(entry);
        self.ram.insert(h, entry.clone()).await;
        let store = self.store.clone();
        tokio::task::spawn_blocking(move || {
            if let Err(e) = write(&store, h, &entry) {
                tracing::warn!("cache write failed: {e}");
            }
        });
    }

    /// Empties both levels («Очистить кэш»).
    pub async fn clear(&self) -> Result<(), String> {
        self.ram.invalidate_all();
        let store = self.store.clone();
        tokio::task::spawn_blocking(move || {
            let tx = store.db().begin_write().map_err(|e| e.to_string())?;
            tx.delete_table(TRANSLATIONS).map_err(|e| e.to_string())?;
            tx.open_table(TRANSLATIONS).map_err(|e| e.to_string())?;
            tx.commit().map_err(|e| e.to_string())
        })
        .await
        .map_err(|e| e.to_string())?
    }
}

fn read(store: &Store, h: u64) -> Option<Entry> {
    let tx = store.db().begin_read().ok()?;
    let table = tx.open_table(TRANSLATIONS).ok()?;
    let bytes = table.get(h).ok()??;
    serde_json::from_slice(bytes.value()).ok()
}

fn write(store: &Store, h: u64, entry: &Entry) -> Result<(), String> {
    let bytes = serde_json::to_vec(entry).map_err(|e| e.to_string())?;
    let tx = store.db().begin_write().map_err(|e| e.to_string())?;
    {
        let mut table = tx.open_table(TRANSLATIONS).map_err(|e| e.to_string())?;
        table.insert(h, bytes.as_slice()).map_err(|e| e.to_string())?;
        // Over the limit: drop the oldest tenth.
        if table.len().map_err(|e| e.to_string())? > DISK_ENTRIES {
            let mut ages: Vec<(u64, u64)> = table
                .iter()
                .map_err(|e| e.to_string())?
                .filter_map(|r| r.ok())
                .filter_map(|(k, v)| Some((k.value(), serde_json::from_slice::<Entry>(v.value()).ok()?.created)))
                .collect();
            ages.sort_by_key(|(_, t)| *t);
            for (k, _) in ages.into_iter().take((DISK_ENTRIES / 10) as usize) {
                table.remove(k).map_err(|e| e.to_string())?;
            }
        }
    }
    tx.commit().map_err(|e| e.to_string())
}

pub fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// FNV-1a, 64 bit: stable across Rust versions (unlike `DefaultHasher`),
/// which matters for keys stored on disk.
struct Fnv(u64);

impl Fnv {
    fn new() -> Self {
        Self(0xcbf2_9ce4_8422_2325)
    }
    fn write(&mut self, bytes: &[u8]) {
        for b in bytes {
            self.0 ^= u64::from(*b);
            self.0 = self.0.wrapping_mul(0x0100_0000_01b3);
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn temp_store(name: &str) -> Arc<Store> {
        let path = std::env::temp_dir().join(format!("hl-cache-test-{name}-{}.redb", std::process::id()));
        let _ = std::fs::remove_file(&path);
        Store::open(&path).unwrap()
    }

    fn key(text: &str) -> Key<'_> {
        Key { text, source: "auto", target: "ru", engine: "", version: 1 }
    }

    fn entry(src: &str, text: &str) -> Entry {
        Entry {
            source_text: src.into(),
            text: text.into(),
            engine: "google".into(),
            source_lang: Some("en".into()),
            alternatives: vec![],
            dict: vec![],
            transcription: None,
            examples: vec![],
            created: now(),
        }
    }

    #[test]
    fn keys_differ_by_every_part() {
        let a = key("hi").hash();
        assert_ne!(a, Key { target: "de", ..key("hi") }.hash());
        assert_ne!(a, Key { engine: "deepl", ..key("hi") }.hash());
        assert_ne!(a, Key { version: 2, ..key("hi") }.hash());
        assert_eq!(a, key("hi").hash());
    }

    #[tokio::test]
    async fn ram_then_disk() {
        let store = temp_store("rd");
        let cache = Cache::new(store.clone());
        cache.put(&key("hello"), entry("hello", "привет")).await;
        assert_eq!(cache.get(&key("hello")).await.unwrap().text, "привет");
        // A fresh cache over the same file finds it on disk.
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        let cold = Cache::new(store);
        assert_eq!(cold.get(&key("hello")).await.unwrap().text, "привет");
        assert!(cold.get(&key("bye")).await.is_none());
        cold.clear().await.unwrap();
        let empty = Cache::new(cold.store.clone());
        assert!(empty.get(&key("hello")).await.is_none());
    }
}
