//! Wikipedia summaries for the popup's «W» card:
//! `https://{lang}.wikipedia.org/api/rest_v1/page/summary/{title}`.
//!
//! A lookup tries the translation in the target-language wiki first, then
//! the original text in the source-language wiki. The only network access
//! is Wikipedia's REST API; results are kept for the session.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::Duration;

use serde::Serialize;

/// A found article.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Article {
    pub title: String,
    pub extract: String,
    pub url: String,
    pub thumbnail: Option<String>,
}

/// What the card shows.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Outcome {
    Found { article: Article },
    /// No article in any wiki tried.
    NotFound { search_url: String },
    /// Wikipedia couldn't be reached.
    Failed { search_url: String },
}

/// Longest title looked up; longer text isn't an article name.
const MAX_TITLE: usize = 120;

/// Our language code as a Wikipedia subdomain ("zh-CN" → "zh").
pub fn wiki_lang(code: &str) -> Option<String> {
    let base = code.split('-').next().unwrap_or(code).trim().to_ascii_lowercase();
    let mapped = match base.as_str() {
        "" | "auto" => return None,
        "iw" => "he",
        "nb" => "no",
        other => other,
    };
    mapped.chars().all(|c| c.is_ascii_lowercase()).then(|| mapped.to_owned())
}

/// One line, no surrounding punctuation.
fn clean_title(s: &str) -> String {
    let one_line = s.split_whitespace().collect::<Vec<_>>().join(" ");
    let trimmed = one_line.trim_matches(|c: char| !c.is_alphanumeric() && c != ')' && c != '(');
    trimmed.chars().take(MAX_TITLE).collect()
}

/// Which wiki is tried first (Settings → «Язык справки»).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Prefer {
    /// The selected word's language.
    Source,
    /// The translation's language.
    Target,
}

/// `(wiki language, title)` pairs to try, in order: the preferred side
/// first, then the other.
pub fn candidates_by(
    prefer: Prefer,
    source: &str,
    source_lang: Option<&str>,
    translation: Option<&str>,
    target: &str,
) -> Vec<(String, String)> {
    let mut c = candidates(source, source_lang, translation, target);
    if prefer == Prefer::Source {
        c.reverse();
    }
    c
}

/// `(wiki language, title)` pairs to try, in order.
pub fn candidates(source: &str, source_lang: Option<&str>, translation: Option<&str>, target: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    let mut push = |lang: Option<String>, text: &str| {
        let title = clean_title(text);
        if let Some(lang) = lang
            && !title.is_empty()
            && !out.iter().any(|(l, t)| *l == lang && t.to_lowercase() == title.to_lowercase())
        {
            out.push((lang, title));
        }
    };
    if let Some(t) = translation {
        push(wiki_lang(target), t);
    }
    push(Some(source_lang.and_then(wiki_lang).unwrap_or_else(|| "en".into())), source);
    out
}

fn encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'(' | b')' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

pub fn search_url(lang: &str, text: &str) -> String {
    format!("https://{lang}.wikipedia.org/w/index.php?search={}", encode(text))
}

fn summary_url(lang: &str, title: &str) -> String {
    format!("https://{lang}.wikipedia.org/api/rest_v1/page/summary/{}", encode(&title.replace(' ', "_")))
}

/// An article from a summary response; `None` for "not found" and
/// disambiguation pages.
fn parse(json: &str, lang: &str) -> Option<Article> {
    let v: serde_json::Value = serde_json::from_str(json).ok()?;
    let kind = v["type"].as_str().unwrap_or_default();
    let title = v["title"].as_str().unwrap_or_default().to_owned();
    if title.is_empty() || kind.contains("not_found") || kind.contains("errors/") || kind == "disambiguation" {
        return None;
    }
    let url = v["content_urls"]["desktop"]["page"]
        .as_str()
        .filter(|p| p.starts_with("https://"))
        .map(str::to_owned)
        .unwrap_or_else(|| format!("https://{lang}.wikipedia.org/wiki/{}", encode(&title.replace(' ', "_"))));
    Some(Article {
        title,
        extract: v["extract"].as_str().unwrap_or_default().trim().to_owned(),
        url,
        thumbnail: v["thumbnail"]["source"].as_str().filter(|s| s.starts_with("https://")).map(str::to_owned),
    })
}

fn client() -> &'static reqwest::Client {
    static CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| {
        reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(4))
            .timeout(Duration::from_secs(8))
            // Titles are normalized with redirects.
            .redirect(reqwest::redirect::Policy::limited(3))
            .user_agent(concat!("HeliLingo/", env!("CARGO_PKG_VERSION"), " (Windows translator; github.com/Helisy420/HeliLingo)"))
            .build()
            .expect("TLS backend available")
    });
    &CLIENT
}

type Candidates = Vec<(String, String)>;

static CACHE: LazyLock<Mutex<HashMap<(Candidates, bool), Outcome>>> = LazyLock::new(Default::default);

/// The article's whole introduction (Settings → «Объём справки: Подробно»),
/// plain text.
async fn intro(lang: &str, title: &str) -> Option<String> {
    let url = format!("https://{lang}.wikipedia.org/w/api.php");
    let r = client()
        .get(url)
        .query(&[
            ("action", "query"),
            ("prop", "extracts"),
            ("exintro", "1"),
            ("explaintext", "1"),
            ("redirects", "1"),
            ("format", "json"),
            ("formatversion", "2"),
            ("titles", title),
        ])
        .send()
        .await
        .ok()?;
    let v: serde_json::Value = r.json().await.ok()?;
    let text = v["query"]["pages"][0]["extract"].as_str()?.trim().to_owned();
    (!text.is_empty()).then_some(text)
}

/// Looks the candidates up; the first with an article wins. `long`: the
/// whole introduction instead of the summary's first lines. Failures aren't
/// kept, so the next try fetches again.
pub async fn lookup(candidates: Vec<(String, String)>, long: bool) -> Outcome {
    let key = (candidates.clone(), long);
    if let Some(o) = CACHE.lock().unwrap().get(&key) {
        return o.clone();
    }
    let search = candidates.first().map_or_else(|| "https://www.wikipedia.org/".to_owned(), |(l, t)| search_url(l, t));
    let mut failed = false;
    let mut found = None;
    for (lang, title) in &candidates {
        match client().get(summary_url(lang, title)).header("Accept", "application/json").send().await {
            Ok(r) if r.status().is_success() => {
                if let Some(mut a) = r.text().await.ok().and_then(|b| parse(&b, lang)) {
                    if long && let Some(text) = intro(lang, &a.title).await {
                        a.extract = text;
                    }
                    found = Some(a);
                    break;
                }
            }
            Ok(r) if r.status() == reqwest::StatusCode::NOT_FOUND => {}
            _ => failed = true,
        }
    }
    let outcome = match found {
        Some(article) => Outcome::Found { article },
        None if failed => return Outcome::Failed { search_url: search },
        None => Outcome::NotFound { search_url: search },
    };
    CACHE.lock().unwrap().insert(key, outcome.clone());
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidates_and_titles() {
        let c = candidates("the cat.", Some("en"), Some("Кошка"), "ru");
        assert_eq!(c, [("ru".into(), "Кошка".into()), ("en".into(), "the cat".into())]);
        assert_eq!(wiki_lang("zh-CN").as_deref(), Some("zh"));
        let s = candidates_by(Prefer::Source, "the cat.", Some("en"), Some("Кошка"), "ru");
        assert_eq!(s[0].0, "en");
        assert_eq!(wiki_lang("auto"), None);
        assert_eq!(summary_url("ru", "Кот Шрёдингера"), "https://ru.wikipedia.org/api/rest_v1/page/summary/%D0%9A%D0%BE%D1%82_%D0%A8%D1%80%D1%91%D0%B4%D0%B8%D0%BD%D0%B3%D0%B5%D1%80%D0%B0");
    }

    #[test]
    fn summaries() {
        let a = parse(r#"{"type":"standard","title":"Кошка","extract":" Домашняя кошка. ","content_urls":{"desktop":{"page":"https://ru.wikipedia.org/wiki/Кошка"}},"thumbnail":{"source":"https://upload.wikimedia.org/x.jpg"}}"#, "ru").unwrap();
        assert_eq!((a.title.as_str(), a.extract.as_str()), ("Кошка", "Домашняя кошка."));
        assert_eq!(a.thumbnail.as_deref(), Some("https://upload.wikimedia.org/x.jpg"));
        assert!(parse(r#"{"type":"disambiguation","title":"Кот"}"#, "ru").is_none());
        assert!(parse(r#"{"type":"https://mediawiki.org/wiki/HyperSwitch/errors/not_found"}"#, "ru").is_none());
    }

    /// `cargo test -p hl-providers -- --ignored wiki`
    #[tokio::test]
    #[ignore]
    async fn live_wiki() {
        let o = lookup(candidates("cat", Some("en"), Some("Кошка"), "ru"), true).await;
        println!("{o:?}");
        assert!(matches!(o, Outcome::Found { .. }));
    }
}
