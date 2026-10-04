//! Wikipedia summaries for the "W" button of the popups and translator
//! windows: `https://{lang}.wikipedia.org/api/rest_v1/page/summary/{title}`.
//!
//! A lookup tries the target-language wiki with the translation first,
//! then the source-language wiki with the original text. Results are
//! fetched on a worker thread and cached in memory for the session; the
//! only network access is Wikipedia's REST API.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use serde::Deserialize;

use crate::web::percent_encode;

static ENABLED: AtomicBool = AtomicBool::new(true);

/// Settings → `show_wiki`: whether the W buttons are shown.
pub fn set_enabled(on: bool) {
    ENABLED.store(on, Ordering::Relaxed);
}

pub fn enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

/// A found article.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Article {
    pub title: String,
    pub extract: String,
    /// `content_urls.desktop.page`.
    pub url: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    Found(Article),
    /// No article in any of the wikis tried.
    NotFound { search_url: String },
    /// Wikipedia couldn't be reached.
    Failed { search_url: String },
}

/// What to look up: `(wiki language, title)` pairs, tried in order.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Query {
    pub candidates: Vec<(String, String)>,
}

/// Longest title looked up (characters); longer text isn't an article name.
const MAX_TITLE: usize = 120;

/// Our language code as a Wikipedia subdomain ("zh-CN" → "zh", "jw" → "jv").
pub fn wiki_lang(code: &str) -> Option<String> {
    let base = code.split('-').next().unwrap_or(code).trim().to_ascii_lowercase();
    let mapped = match base.as_str() {
        "" | "auto" => return None,
        "iw" => "he",
        "jw" => "jv",
        "in" => "id",
        "nb" => "no",
        other => other,
    };
    mapped.chars().all(|c| c.is_ascii_lowercase()).then(|| mapped.to_owned())
}

/// The text as an article title: one line, no surrounding punctuation.
fn clean_title(s: &str) -> String {
    let one_line = s.split_whitespace().collect::<Vec<_>>().join(" ");
    let trimmed = one_line.trim_matches(|c: char| !c.is_alphanumeric() && c != ')' && c != '(');
    trimmed.chars().take(MAX_TITLE).collect()
}

impl Query {
    /// The translation in the target-language wiki, then the source text
    /// in the source-language wiki (English when it wasn't detected).
    pub fn new(source: &str, src_lang: Option<&str>, translation: Option<&str>, target: &str) -> Self {
        let mut candidates: Vec<(String, String)> = Vec::new();
        let mut push = |lang: Option<String>, text: &str| {
            let title = clean_title(text);
            if let Some(lang) = lang
                && !title.is_empty()
                && !candidates.iter().any(|(l, t)| *l == lang && t.to_lowercase() == title.to_lowercase())
            {
                candidates.push((lang, title));
            }
        };
        if let Some(t) = translation {
            push(wiki_lang(target), t);
        }
        let src = src_lang.and_then(wiki_lang).unwrap_or_else(|| "en".to_owned());
        push(Some(src), source);
        Self { candidates }
    }

    /// "Search Wikipedia" link for the first candidate.
    pub fn search_url(&self) -> String {
        match self.candidates.first() {
            Some((lang, title)) => search_url(lang, title),
            None => "https://www.wikipedia.org/".to_owned(),
        }
    }
}

pub fn summary_url(lang: &str, title: &str) -> String {
    format!(
        "https://{lang}.wikipedia.org/api/rest_v1/page/summary/{}",
        percent_encode(&title.replace(' ', "_"))
    )
}

pub fn search_url(lang: &str, text: &str) -> String {
    format!("https://{lang}.wikipedia.org/w/index.php?search={}", percent_encode(text))
}

#[derive(Deserialize)]
struct Summary {
    #[serde(default, rename = "type")]
    kind: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    extract: String,
    #[serde(default)]
    content_urls: Option<ContentUrls>,
}

#[derive(Deserialize)]
struct ContentUrls {
    desktop: Option<PageUrl>,
}

#[derive(Deserialize)]
struct PageUrl {
    #[serde(default)]
    page: String,
}

/// An article from a summary response; `None` for "not found" bodies.
pub fn parse_summary(json: &str, lang: &str) -> Option<Article> {
    let s: Summary = serde_json::from_str(json).ok()?;
    if s.title.is_empty() || s.kind.contains("not_found") || s.kind.contains("errors/") {
        return None;
    }
    let url = s
        .content_urls
        .and_then(|c| c.desktop)
        .map(|d| d.page)
        .filter(|p| p.starts_with("https://"))
        .unwrap_or_else(|| format!("https://{lang}.wikipedia.org/wiki/{}", percent_encode(&s.title.replace(' ', "_"))));
    Some(Article { title: s.title, extract: s.extract.trim().to_owned(), url })
}

fn client() -> &'static reqwest::blocking::Client {
    static CLIENT: OnceLock<reqwest::blocking::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::blocking::Client::builder()
            .connect_timeout(Duration::from_secs(4))
            .timeout(Duration::from_secs(8))
            .user_agent(concat!("HeliLingo/", env!("CARGO_PKG_VERSION"), " (Windows translator)"))
            .build()
            .expect("HTTP client")
    })
}

enum Fetched {
    Article(Article),
    Missing,
    Error,
}

fn fetch_one(lang: &str, title: &str) -> Fetched {
    let resp = match client().get(summary_url(lang, title)).header("Accept", "application/json").send() {
        Ok(r) => r,
        Err(_) => return Fetched::Error,
    };
    let status = resp.status();
    if status == reqwest::StatusCode::NOT_FOUND {
        return Fetched::Missing;
    }
    if !status.is_success() {
        return Fetched::Error;
    }
    match resp.text().ok().and_then(|body| parse_summary(&body, lang)) {
        Some(a) => Fetched::Article(a),
        None => Fetched::Missing,
    }
}

/// Runs the lookup (blocking): the first candidate with an article wins.
pub fn fetch(q: &Query) -> Outcome {
    let mut failed = false;
    for (lang, title) in &q.candidates {
        match fetch_one(lang, title) {
            Fetched::Article(a) => return Outcome::Found(a),
            Fetched::Missing => {}
            Fetched::Error => failed = true,
        }
    }
    let search_url = q.search_url();
    if failed { Outcome::Failed { search_url } } else { Outcome::NotFound { search_url } }
}

enum Slot {
    Loading,
    Done(Arc<Outcome>),
}

fn cache() -> &'static Mutex<HashMap<Query, Slot>> {
    static CACHE: OnceLock<Mutex<HashMap<Query, Slot>>> = OnceLock::new();
    CACHE.get_or_init(Mutex::default)
}

/// The cached result of `q`, or `None` while it loads. The first call
/// starts the fetch on a worker thread; `notify` runs when it finishes
/// (e.g. a repaint of the window showing the card). Failed lookups are
/// not kept, so closing and reopening the card tries again.
pub fn lookup(q: &Query, notify: impl FnOnce() + Send + 'static) -> Option<Arc<Outcome>> {
    let mut map = cache().lock().unwrap();
    match map.get(q) {
        Some(Slot::Done(o)) => return Some(o.clone()),
        Some(Slot::Loading) => return None,
        None => {}
    }
    map.insert(q.clone(), Slot::Loading);
    drop(map);
    let q = q.clone();
    std::thread::spawn(move || {
        let outcome = Arc::new(fetch(&q));
        cache().lock().unwrap().insert(q, Slot::Done(outcome));
        notify();
    });
    None
}

/// Drops a failed result so the next [`lookup`] fetches again.
pub fn forget_failure(q: &Query) {
    let mut map = cache().lock().unwrap();
    if matches!(map.get(q), Some(Slot::Done(o)) if matches!(**o, Outcome::Failed { .. })) {
        map.remove(q);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CAR: &str = r#"{
        "type": "standard",
        "title": "Car",
        "displaytitle": "<span class=\"mw-page-title-main\">Car</span>",
        "extract": "A car, or an automobile, is a motor vehicle with wheels. Most definitions of cars state that they run primarily on roads. ",
        "lang": "en",
        "content_urls": {
            "desktop": {"page": "https://en.wikipedia.org/wiki/Car", "revisions": "https://en.wikipedia.org/wiki/Car?action=history"},
            "mobile": {"page": "https://en.m.wikipedia.org/wiki/Car"}
        }
    }"#;

    const MISSING: &str = r#"{
        "type": "https://mediawiki.org/wiki/HyperSwitch/errors/not_found",
        "title": "Not found.",
        "method": "get",
        "detail": "Page or revision not found.",
        "uri": "/en.wikipedia.org/v1/page/summary/Qwzxv"
    }"#;

    #[test]
    fn parses_an_article() {
        let a = parse_summary(CAR, "en").unwrap();
        assert_eq!(a.title, "Car");
        assert!(a.extract.starts_with("A car, or an automobile"));
        assert!(!a.extract.ends_with(' '));
        assert_eq!(a.url, "https://en.wikipedia.org/wiki/Car");
    }

    #[test]
    fn not_found_and_junk_are_none() {
        assert_eq!(parse_summary(MISSING, "en"), None);
        assert_eq!(parse_summary("<html>", "en"), None);
        assert_eq!(parse_summary("{}", "en"), None);
    }

    #[test]
    fn missing_desktop_url_is_built() {
        let a = parse_summary(r#"{"type":"standard","title":"Чай","extract":"Напиток."}"#, "ru").unwrap();
        assert_eq!(a.url, "https://ru.wikipedia.org/wiki/%D0%A7%D0%B0%D0%B9");
    }

    #[test]
    fn urls() {
        assert_eq!(summary_url("en", "New York"), "https://en.wikipedia.org/api/rest_v1/page/summary/New_York");
        assert_eq!(summary_url("en", "AC/DC"), "https://en.wikipedia.org/api/rest_v1/page/summary/AC%2FDC");
        assert_eq!(search_url("ru", "ещё раз"), "https://ru.wikipedia.org/w/index.php?search=%D0%B5%D1%89%D1%91%20%D1%80%D0%B0%D0%B7");
    }

    #[test]
    fn languages() {
        assert_eq!(wiki_lang("zh-CN").as_deref(), Some("zh"));
        assert_eq!(wiki_lang("iw").as_deref(), Some("he"));
        assert_eq!(wiki_lang("auto"), None);
        assert_eq!(wiki_lang("ru").as_deref(), Some("ru"));
    }

    #[test]
    fn query_order() {
        let q = Query::new("car.", Some("en"), Some("автомобиль"), "ru");
        assert_eq!(
            q.candidates,
            [("ru".to_owned(), "автомобиль".to_owned()), ("en".to_owned(), "car".to_owned())]
        );
        assert_eq!(q.search_url(), "https://ru.wikipedia.org/w/index.php?search=%D0%B0%D0%B2%D1%82%D0%BE%D0%BC%D0%BE%D0%B1%D0%B8%D0%BB%D1%8C");
        // Unknown source language: the English wiki; duplicates are dropped.
        let q = Query::new("Paris", None, Some("Paris"), "en");
        assert_eq!(q.candidates, [("en".to_owned(), "Paris".to_owned())]);
    }
}
