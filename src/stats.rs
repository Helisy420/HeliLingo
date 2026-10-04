//! Usage statistics for Settings → Statistics, kept only on this PC in
//! %APPDATA%\HeliLingo\stats.json: translations per feature and per day,
//! characters, language pairs and the most translated words. "Keep
//! statistics" (on by default) turns recording off; Reset deletes the file.

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Sender, channel};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use serde::{Deserialize, Serialize};

/// What a translation was made with (the "Functions" list).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Feature {
    /// The popup from the double-tap shortcut.
    DoubleCtrl,
    Quick,
    Main,
    Image,
    Ultra,
}

impl Feature {
    pub const ALL: [Feature; 5] = [Feature::DoubleCtrl, Feature::Quick, Feature::Main, Feature::Image, Feature::Ultra];

    fn index(self) -> usize {
        Feature::ALL.iter().position(|f| *f == self).unwrap_or(0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Period {
    Week,
    Month,
    All,
}

impl Period {
    fn days(self) -> Option<u32> {
        match self {
            Period::Week => Some(7),
            Period::Month => Some(30),
            Period::All => None,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct Day {
    features: [u64; 5],
    chars: u64,
    /// "en>ru" → count.
    pairs: BTreeMap<String, u64>,
    /// Lower-cased source word → count (single-word translations only).
    words: BTreeMap<String, u64>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Data {
    /// Local day number (days since 1970-01-01) → counts.
    days: BTreeMap<u32, Day>,
    /// Latest translation of each counted word.
    word_translations: HashMap<String, String>,
}

/// Everything the Statistics page shows for one period.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Summary {
    pub translations: u64,
    /// The same count for the period before (for "↑ 12% vs last week");
    /// `None` for all time.
    pub previous: Option<u64>,
    pub chars: u64,
    /// Days in a row up to today with at least one translation.
    pub streak: u32,
    /// Longest such run ever.
    pub best_streak: u32,
    /// Uses per feature, in `Feature::ALL` order.
    pub features: [u64; 5],
    /// (from, to, count), most used first.
    pub pairs: Vec<(String, String, u64)>,
    /// (word, its translation, count), most translated first.
    pub words: Vec<(String, String, u64)>,
}

impl Summary {
    /// Source language translated from most often.
    pub fn top_source(&self) -> Option<&str> {
        let mut by_lang: BTreeMap<&str, u64> = BTreeMap::new();
        for (from, _, n) in &self.pairs {
            *by_lang.entry(from.as_str()).or_default() += n;
        }
        by_lang.into_iter().max_by_key(|(_, n)| *n).map(|(l, _)| l)
    }
}

struct Store {
    data: Mutex<Data>,
    enabled: AtomicBool,
    save_tx: Sender<()>,
}

fn path() -> Option<PathBuf> {
    Some(crate::settings::data_dir()?.join("stats.json"))
}

fn store() -> &'static Store {
    static STORE: OnceLock<Store> = OnceLock::new();
    STORE.get_or_init(|| {
        let data: Data = path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|json| serde_json::from_str(&json).ok())
            .unwrap_or_default();
        let (save_tx, save_rx) = channel::<()>();
        std::thread::Builder::new()
            .name("stats".into())
            .spawn(move || {
                while save_rx.recv().is_ok() {
                    std::thread::sleep(Duration::from_secs(2));
                    while save_rx.try_recv().is_ok() {}
                    write_file();
                }
            })
            .expect("spawn stats thread");
        Store { data: Mutex::new(data), enabled: AtomicBool::new(true), save_tx }
    })
}

fn write_file() {
    let Some(p) = path() else { return };
    let json = {
        let data = store().data.lock().unwrap();
        if data.days.is_empty() {
            let _ = std::fs::remove_file(&p);
            return;
        }
        serde_json::to_string(&*data)
    };
    if let Ok(json) = json {
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let tmp = p.with_extension("json.tmp");
        if std::fs::write(&tmp, json).is_ok() {
            let _ = std::fs::rename(&tmp, &p);
        }
    }
}

/// Today's local day number.
pub fn today() -> u32 {
    let t = unsafe { windows::Win32::System::SystemInformation::GetLocalTime() };
    days_from_civil(t.wYear as i64, t.wMonth as i64, t.wDay as i64) as u32
}

/// Days since 1970-01-01 for a calendar date (Howard Hinnant's algorithm).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

pub fn set_enabled(on: bool) {
    store().enabled.store(on, Ordering::Relaxed);
}

/// Counts one translation. `from` is the detected or chosen source code.
/// Single words (and short dictionary phrases) also count towards the
/// "most translated words" list.
pub fn record(feature: Feature, source: &str, translation: &str, from: &str, to: &str, is_word: bool) {
    let s = store();
    if !s.enabled.load(Ordering::Relaxed) || source.trim().is_empty() {
        return;
    }
    let mut data = s.data.lock().unwrap();
    add(&mut data, today(), feature, source, translation, from, to, is_word);
    drop(data);
    let _ = s.save_tx.send(());
}

#[allow(clippy::too_many_arguments)]
fn add(data: &mut Data, day: u32, feature: Feature, source: &str, translation: &str, from: &str, to: &str, is_word: bool) {
    let d = data.days.entry(day).or_default();
    d.features[feature.index()] += 1;
    d.chars += source.chars().count() as u64;
    let base = |l: &str| l.split('-').next().unwrap_or(l).to_lowercase();
    if from != "auto" && !from.is_empty() {
        *d.pairs.entry(format!("{}>{}", base(from), base(to))).or_default() += 1;
    }
    if is_word {
        let word = source.trim().to_lowercase();
        *d.words.entry(word.clone()).or_default() += 1;
        data.word_translations.insert(word, translation.trim().to_owned());
    }
}

pub fn summary(period: Period) -> Summary {
    summarize(&store().data.lock().unwrap(), today(), period)
}

fn summarize(data: &Data, today: u32, period: Period) -> Summary {
    let in_range = |day: u32, from: u32, to: u32| day >= from && day <= to;
    let start = period.days().map_or(0, |n| today.saturating_sub(n - 1));
    let mut s = Summary::default();
    let mut pairs: BTreeMap<&str, u64> = BTreeMap::new();
    let mut words: BTreeMap<&str, u64> = BTreeMap::new();
    for (&day, d) in data.days.range(start..=today) {
        let _ = day;
        s.translations += d.features.iter().sum::<u64>();
        s.chars += d.chars;
        for (i, n) in d.features.iter().enumerate() {
            s.features[i] += n;
        }
        for (p, n) in &d.pairs {
            *pairs.entry(p).or_default() += n;
        }
        for (w, n) in &d.words {
            *words.entry(w).or_default() += n;
        }
    }
    if let Some(n) = period.days() {
        let (from, to) = (start.saturating_sub(n), start.saturating_sub(1));
        s.previous = Some(
            data.days
                .iter()
                .filter(|(day, _)| in_range(**day, from, to) && start > 0)
                .map(|(_, d)| d.features.iter().sum::<u64>())
                .sum(),
        );
    }
    let mut pairs: Vec<(String, String, u64)> = pairs
        .into_iter()
        .filter_map(|(p, n)| p.split_once('>').map(|(a, b)| (a.to_owned(), b.to_owned(), n)))
        .collect();
    pairs.sort_by_key(|p| std::cmp::Reverse(p.2));
    s.pairs = pairs;
    let mut words: Vec<(String, String, u64)> = words
        .into_iter()
        .map(|(w, n)| (w.to_owned(), data.word_translations.get(w).cloned().unwrap_or_default(), n))
        .collect();
    words.sort_by(|a, b| b.2.cmp(&a.2).then_with(|| a.0.cmp(&b.0)));
    s.words = words;

    // Streaks over active days.
    let active = |day: u32| data.days.get(&day).is_some_and(|d| d.features.iter().sum::<u64>() > 0);
    let mut day = today;
    if !active(day) {
        day = day.saturating_sub(1);
    }
    while active(day) && day > 0 {
        s.streak += 1;
        day -= 1;
    }
    let (mut run, mut prev) = (0u32, None::<u32>);
    for (&d, _) in data.days.iter().filter(|(_, d)| d.features.iter().sum::<u64>() > 0) {
        run = if prev == Some(d.wrapping_sub(1)) { run + 1 } else { 1 };
        s.best_streak = s.best_streak.max(run);
        prev = Some(d);
    }
    s
}

/// Deletes all statistics and the file.
pub fn reset() {
    let s = store();
    *s.data.lock().unwrap() = Data::default();
    let _ = s.save_tx.send(());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn civil_days() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(days_from_civil(2026, 10, 4), 20_730);
    }

    #[test]
    fn summary_by_period_and_streaks() {
        let mut data = Data::default();
        let today = 1000;
        for (day, n) in [(990, 1), (995, 1), (998, 2), (999, 1), (1000, 3)] {
            for _ in 0..n {
                add(&mut data, day, Feature::DoubleCtrl, "however", "однако", "en", "ru", true);
            }
        }
        add(&mut data, 1000, Feature::Quick, "Guten Tag", "Добрый день", "de", "ru", false);
        let week = summarize(&data, today, Period::Week);
        assert_eq!(week.translations, 8);
        assert_eq!(week.previous, Some(1));
        assert_eq!(week.features[0], 7);
        assert_eq!(week.features[1], 1);
        assert_eq!(week.pairs[0], ("en".into(), "ru".into(), 7));
        assert_eq!(week.words[0], ("however".into(), "однако".into(), 7));
        assert_eq!(week.top_source(), Some("en"));
        assert_eq!(week.streak, 3);
        assert_eq!(summarize(&data, today, Period::All).translations, 9);
        assert_eq!(summarize(&data, today + 1, Period::All).streak, 3);
        assert_eq!(week.best_streak, 3);
    }
}
