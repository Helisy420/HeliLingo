//! Translation history: newest first, up to 1000 entries plus every starred
//! one. Translating the same text to the same language again moves the old
//! entry to the top instead of adding a copy.

use std::sync::Arc;

use redb::{ReadableDatabase, ReadableTable, TableDefinition};
use serde::{Deserialize, Serialize};

use crate::{Store, now};

pub(crate) const TABLE: TableDefinition<u64, &[u8]> = TableDefinition::new("history");
const KEEP: usize = 1000;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub id: u64,
    pub source_text: String,
    pub text: String,
    pub source_lang: Option<String>,
    pub target_lang: String,
    pub engine: String,
    /// Where it was translated: popup, quick, translator, ultra, paste.
    pub surface: String,
    pub starred: bool,
    /// Unix seconds.
    pub created: u64,
}

#[derive(Clone)]
pub struct History {
    store: Arc<Store>,
}

type R<T> = Result<T, String>;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

impl History {
    pub fn new(store: Arc<Store>) -> Self {
        Self { store }
    }

    /// Adds an entry (or moves the same text+target to the top). Blocking.
    pub fn add(&self, mut item: Item) -> R<Item> {
        let tx = self.store.db().begin_write().map_err(err)?;
        {
            let mut t = tx.open_table(TABLE).map_err(err)?;
            let all = items(&t)?;
            if let Some(old) = all.iter().find(|i| i.source_text == item.source_text && i.target_lang == item.target_lang) {
                item.starred = old.starred;
                t.remove(old.id).map_err(err)?;
            }
            item.id = next_id(&all);
            item.created = now();
            t.insert(item.id, serde_json::to_vec(&item).map_err(err)?.as_slice()).map_err(err)?;
            // Trim: unstarred entries beyond the newest KEEP.
            let mut plain: Vec<u64> = all.iter().filter(|i| !i.starred && i.id != item.id).map(|i| i.id).collect();
            plain.sort_unstable_by(|a, b| b.cmp(a));
            for id in plain.into_iter().skip(KEEP.saturating_sub(1)) {
                t.remove(id).map_err(err)?;
            }
        }
        tx.commit().map_err(err)?;
        Ok(item)
    }

    /// Newest first; `query` matches source or translation (any case);
    /// `starred` keeps only starred ones.
    pub fn list(&self, query: &str, starred: bool, limit: usize) -> R<Vec<Item>> {
        let tx = self.store.db().begin_read().map_err(err)?;
        let t = tx.open_table(TABLE).map_err(err)?;
        let q = query.trim().to_lowercase();
        let mut out = Vec::new();
        for row in t.iter().map_err(err)?.rev() {
            let (_, v) = row.map_err(err)?;
            let Ok(item) = serde_json::from_slice::<Item>(v.value()) else { continue };
            if starred && !item.starred {
                continue;
            }
            if !q.is_empty() && !item.source_text.to_lowercase().contains(&q) && !item.text.to_lowercase().contains(&q) {
                continue;
            }
            out.push(item);
            if out.len() >= limit {
                break;
            }
        }
        Ok(out)
    }

    pub fn set_star(&self, id: u64, starred: bool) -> R<()> {
        self.edit(id, |i| i.starred = starred)
    }

    pub fn delete(&self, id: u64) -> R<()> {
        let tx = self.store.db().begin_write().map_err(err)?;
        tx.open_table(TABLE).map_err(err)?.remove(id).map_err(err)?;
        tx.commit().map_err(err)
    }

    /// Deletes everything except starred entries.
    pub fn clear(&self) -> R<()> {
        let tx = self.store.db().begin_write().map_err(err)?;
        {
            let mut t = tx.open_table(TABLE).map_err(err)?;
            for item in items(&t)?.into_iter().filter(|i| !i.starred) {
                t.remove(item.id).map_err(err)?;
            }
        }
        tx.commit().map_err(err)
    }

    fn edit(&self, id: u64, f: impl FnOnce(&mut Item)) -> R<()> {
        let tx = self.store.db().begin_write().map_err(err)?;
        {
            let mut t = tx.open_table(TABLE).map_err(err)?;
            let found = t.get(id).map_err(err)?.and_then(|v| serde_json::from_slice::<Item>(v.value()).ok());
            if let Some(mut item) = found {
                f(&mut item);
                t.insert(id, serde_json::to_vec(&item).map_err(err)?.as_slice()).map_err(err)?;
            }
        }
        tx.commit().map_err(err)
    }
}

fn items(t: &impl ReadableTable<u64, &'static [u8]>) -> R<Vec<Item>> {
    let mut out = Vec::new();
    for row in t.iter().map_err(err)? {
        let (_, v) = row.map_err(err)?;
        if let Ok(item) = serde_json::from_slice(v.value()) {
            out.push(item);
        }
    }
    Ok(out)
}

/// Ids grow with time, so table order is age order.
fn next_id(all: &[Item]) -> u64 {
    let last = all.iter().map(|i| i.id).max().unwrap_or(0);
    let clock = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_micros() as u64);
    clock.max(last + 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::temp_store;

    fn item(src: &str) -> Item {
        Item {
            id: 0,
            source_text: src.into(),
            text: format!("[{src}]"),
            source_lang: Some("en".into()),
            target_lang: "ru".into(),
            engine: "google".into(),
            surface: "quick".into(),
            starred: false,
            created: 0,
        }
    }

    #[test]
    fn newest_first_and_deduplicated() {
        let h = History::new(temp_store("hist"));
        h.add(item("one")).unwrap();
        let two = h.add(item("two")).unwrap();
        h.set_star(two.id, true).unwrap();
        h.add(item("one")).unwrap();
        let all = h.list("", false, 10).unwrap();
        assert_eq!(all.iter().map(|i| i.source_text.as_str()).collect::<Vec<_>>(), ["one", "two"]);
        assert_eq!(h.list("TW", false, 10).unwrap().len(), 1);
        assert_eq!(h.list("", true, 10).unwrap()[0].source_text, "two");
        h.clear().unwrap();
        assert_eq!(h.list("", false, 10).unwrap().len(), 1, "starred survives clear");
    }
}
