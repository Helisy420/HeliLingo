//! Usage statistics for Settings → Статистика, kept only on this PC: per
//! local day, translations by feature, characters, language pairs, the
//! words translated most and the OpenRouter tokens spent.

use std::collections::BTreeMap;
use std::sync::Arc;

use redb::{ReadableDatabase, ReadableTable, TableDefinition};
use serde::{Deserialize, Serialize};

use crate::Store;

/// Local day number (days since 1970-01-01 in local time) → `Day`.
pub(crate) const TABLE: TableDefinition<u32, &[u8]> = TableDefinition::new("stats");

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Day {
    /// Feature ("popup", "quick", "translator", "images", "ultra") → count.
    pub features: BTreeMap<String, u64>,
    pub chars: u64,
    /// "en>ru" → count.
    pub pairs: BTreeMap<String, u64>,
    /// "word\ttranslation" → count (single words and short phrases).
    pub words: BTreeMap<String, u64>,
    /// OpenRouter tokens: sent (prompt) and received (completion).
    pub tokens_in: u64,
    pub tokens_out: u64,
}

/// One finished translation, as counted.
pub struct Event<'a> {
    pub day: u32,
    pub feature: &'a str,
    pub source_text: &'a str,
    pub text: &'a str,
    pub source_lang: &'a str,
    pub target_lang: &'a str,
}

/// The Statistics page for a period.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub translations: u64,
    /// Same-length period before, for «на прошлой неделе: N».
    pub previous: u64,
    pub chars: u64,
    pub streak: u32,
    pub best_streak: u32,
    pub features: BTreeMap<String, u64>,
    /// Most used first.
    pub pairs: Vec<(String, u64)>,
    /// Most used first, at most 6: (word, translation, count).
    pub words: Vec<(String, String, u64)>,
    /// OpenRouter tokens in the period.
    pub tokens_in: u64,
    pub tokens_out: u64,
}

#[derive(Clone)]
pub struct Stats {
    store: Arc<Store>,
}

type R<T> = Result<T, String>;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

impl Stats {
    pub fn new(store: Arc<Store>) -> Self {
        Self { store }
    }

    /// Counts one translation. Blocking.
    pub fn record(&self, e: &Event) -> R<()> {
        let tx = self.store.db().begin_write().map_err(err)?;
        {
            let mut t = tx.open_table(TABLE).map_err(err)?;
            let mut day: Day =
                t.get(e.day).map_err(err)?.and_then(|v| serde_json::from_slice(v.value()).ok()).unwrap_or_default();
            *day.features.entry(e.feature.to_owned()).or_default() += 1;
            day.chars += e.source_text.chars().count() as u64;
            *day.pairs.entry(format!("{}>{}", e.source_lang, e.target_lang)).or_default() += 1;
            let words = e.source_text.split_whitespace().count();
            if (1..=3).contains(&words) && e.source_text.chars().count() <= 40 {
                let key = format!("{}\t{}", e.source_text.trim().to_lowercase(), e.text.trim());
                *day.words.entry(key).or_default() += 1;
            }
            t.insert(e.day, serde_json::to_vec(&day).map_err(err)?.as_slice()).map_err(err)?;
        }
        tx.commit().map_err(err)
    }

    /// Adds OpenRouter tokens to `day`. Blocking.
    pub fn add_tokens(&self, day: u32, prompt: u64, completion: u64) -> R<()> {
        let tx = self.store.db().begin_write().map_err(err)?;
        {
            let mut t = tx.open_table(TABLE).map_err(err)?;
            let mut d: Day =
                t.get(day).map_err(err)?.and_then(|v| serde_json::from_slice(v.value()).ok()).unwrap_or_default();
            d.tokens_in += prompt;
            d.tokens_out += completion;
            t.insert(day, serde_json::to_vec(&d).map_err(err)?.as_slice()).map_err(err)?;
        }
        tx.commit().map_err(err)
    }

    /// The last `days` days up to `today` (0 = all time).
    pub fn summary(&self, today: u32, days: u32) -> R<Summary> {
        let tx = self.store.db().begin_read().map_err(err)?;
        let t = tx.open_table(TABLE).map_err(err)?;
        let mut all: BTreeMap<u32, Day> = BTreeMap::new();
        for row in t.iter().map_err(err)? {
            let (k, v) = row.map_err(err)?;
            if let Ok(d) = serde_json::from_slice(v.value()) {
                all.insert(k.value(), d);
            }
        }
        let from = if days == 0 { 0 } else { today.saturating_sub(days - 1) };
        let mut s = Summary::default();
        let mut pairs: BTreeMap<String, u64> = BTreeMap::new();
        let mut words: BTreeMap<String, u64> = BTreeMap::new();
        for (_, d) in all.range(from..=today) {
            for (f, n) in &d.features {
                *s.features.entry(f.clone()).or_default() += n;
                s.translations += n;
            }
            s.chars += d.chars;
            s.tokens_in += d.tokens_in;
            s.tokens_out += d.tokens_out;
            for (p, n) in &d.pairs {
                *pairs.entry(p.clone()).or_default() += n;
            }
            for (w, n) in &d.words {
                *words.entry(w.clone()).or_default() += n;
            }
        }
        if days > 0 {
            let prev_from = from.saturating_sub(days);
            s.previous = all.range(prev_from..from).map(|(_, d)| d.features.values().sum::<u64>()).sum();
        }
        s.pairs = sorted(pairs);
        s.words = sorted(words)
            .into_iter()
            .take(6)
            .filter_map(|(k, n)| k.split_once('\t').map(|(a, b)| (a.to_owned(), b.to_owned(), n)))
            .collect();
        (s.streak, s.best_streak) = streaks(&all, today);
        Ok(s)
    }

    /// Deletes all statistics («Сбросить»).
    pub fn reset(&self) -> R<()> {
        let tx = self.store.db().begin_write().map_err(err)?;
        tx.delete_table(TABLE).map_err(err)?;
        tx.open_table(TABLE).map_err(err)?;
        tx.commit().map_err(err)
    }
}

fn sorted(map: BTreeMap<String, u64>) -> Vec<(String, u64)> {
    let mut v: Vec<_> = map.into_iter().collect();
    v.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    v
}

/// Days in a row with translations ending today (or yesterday), and the record.
fn streaks(all: &BTreeMap<u32, Day>, today: u32) -> (u32, u32) {
    let active = |d: u32| all.get(&d).is_some_and(|x| x.features.values().any(|n| *n > 0));
    let mut current = 0;
    let mut d = if active(today) { today } else { today.saturating_sub(1) };
    while active(d) {
        current += 1;
        if d == 0 {
            break;
        }
        d -= 1;
    }
    let (mut best, mut run, mut prev) = (0, 0, None::<u32>);
    for (&day, x) in all {
        if x.features.values().all(|n| *n == 0) {
            continue;
        }
        run = if prev == Some(day.wrapping_sub(1)) { run + 1 } else { 1 };
        best = best.max(run);
        prev = Some(day);
    }
    (current, best.max(current))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::temp_store;

    fn ev(day: u32, feature: &str, text: &str) -> Event<'static> {
        let text: &'static str = Box::leak(text.to_owned().into_boxed_str());
        Event { day, feature: Box::leak(feature.to_owned().into_boxed_str()), source_text: text, text: "x", source_lang: "en", target_lang: "ru" }
    }

    #[test]
    fn week_summary_and_streak() {
        let s = Stats::new(temp_store("stats"));
        for day in [96, 97, 98, 100] {
            s.record(&ev(day, "popup", "hello")).unwrap();
        }
        s.record(&ev(100, "quick", "a longer sentence to translate here")).unwrap();
        s.add_tokens(99, 40, 5).unwrap();
        s.add_tokens(100, 10, 2).unwrap();
        let w = s.summary(100, 7).unwrap();
        assert_eq!((w.tokens_in, w.tokens_out), (50, 7));
        // Token-only days don't count as translation days.
        assert_eq!(w.best_streak, 3);
        assert_eq!(w.translations, 5);
        assert_eq!(w.features["popup"], 4);
        assert_eq!(w.streak, 1);
        assert_eq!(w.best_streak, 3);
        assert_eq!(w.words[0], ("hello".into(), "x".into(), 4));
        assert_eq!(w.pairs[0], ("en>ru".into(), 5));
        s.reset().unwrap();
        assert_eq!(s.summary(100, 0).unwrap().translations, 0);
    }
}
