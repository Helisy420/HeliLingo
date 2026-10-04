//! Google: the public web endpoints (no key; `translate_a/single` returns
//! dictionary data), or Cloud Translation v2 when a key is set.
//!
//! Google rate-limits the keyless endpoints per IP and answers 429 (or
//! redirects to a captcha page) when a PC sends too much. Each endpoint
//! that does so is left alone for a while (doubling up to 30 minutes), and
//! the next one is tried, in the order of the "Google mode" setting. When
//! every endpoint is cooling down the provider reports Busy at once,
//! without sending anything.
//!
//! Endpoints are raced (see [`super::hedge`]): a "busy" answer can take a
//! second to arrive, so the next endpoint starts after a short wait instead
//! of after it. The cooldowns are kept in `%APPDATA%\HeliLingo\cooldown.json`
//! so a restart doesn't learn them again the slow way.

use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use serde::Deserialize;

use super::{DictGroup, Engine, Error, KEY_REJECTED, Translation, hedge, lang, parse, send, status_error};
use crate::settings::{GoogleMode, ProviderKind};

pub const FREE_URL: &str = "https://translate.googleapis.com/translate_a/single";
pub const CLOUD_URL: &str = "https://translation.googleapis.com/language/translate/v2";
/// Same API on the main site (rate-limited separately).
const SITE_URL: &str = "https://translate.google.com/translate_a/single";
/// Chrome dictionary extension endpoint.
const DICT_URL: &str = "https://clients5.google.com/translate_a/t";
/// The plain-HTML mobile page.
const MOBILE_URL: &str = "https://translate.google.com/m";
/// Warming the connection: the host, not an endpoint that counts requests.
pub const WARM_URL: &str = "https://translate.googleapis.com/";

/// The keyless endpoints, in the default order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Endpoint {
    Api,
    Site,
    Dict,
    Mobile,
}

impl Endpoint {
    const ALL: [Endpoint; 4] = [Endpoint::Api, Endpoint::Site, Endpoint::Dict, Endpoint::Mobile];

    fn order(mode: GoogleMode) -> &'static [Endpoint] {
        match mode {
            GoogleMode::Auto => &Endpoint::ALL,
            GoogleMode::Api => &[Endpoint::Api],
            GoogleMode::Web => &[Endpoint::Mobile, Endpoint::Dict, Endpoint::Site, Endpoint::Api],
        }
    }

    fn index(self) -> usize {
        Endpoint::ALL.iter().position(|e| *e == self).unwrap_or(0)
    }
}

const BACKOFF_START: Duration = Duration::from_secs(60);
const BACKOFF_MAX: Duration = Duration::from_secs(30 * 60);
/// Racing the endpoints: the next one starts after this wait.
const ENDPOINT_HEDGE: Duration = Duration::from_millis(450);

/// Per endpoint: busy until, and the wait that was used.
#[derive(Default, Debug)]
pub struct Backoff([Option<(Instant, Duration)>; 4]);

impl Backoff {
    fn cooling(&self, e: Endpoint, now: Instant) -> bool {
        self.0[e.index()].is_some_and(|(until, _)| until > now)
    }

    /// Has a cooldown recorded (current or past), i.e. `ok` would change it.
    fn cooling_or_set(&self, e: Endpoint) -> bool {
        self.0[e.index()].is_some()
    }

    fn busy(&mut self, e: Endpoint, now: Instant) {
        let next = match self.0[e.index()] {
            Some((_, wait)) => (wait * 2).min(BACKOFF_MAX),
            None => BACKOFF_START,
        };
        self.0[e.index()] = Some((now + next, next));
    }

    fn ok(&mut self, e: Endpoint) {
        self.0[e.index()] = None;
    }

    fn file() -> Option<std::path::PathBuf> {
        Some(crate::settings::data_dir()?.join("cooldown.json"))
    }

    /// The cooldowns saved by the last session (wall-clock times on disk).
    pub fn load() -> Backoff {
        let saved: Option<[Option<(u64, u64)>; 4]> = Self::file()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|json| serde_json::from_str(&json).ok());
        let Some(saved) = saved else { return Backoff::default() };
        let (now, wall) = (Instant::now(), epoch_secs(SystemTime::now()));
        Backoff(saved.map(|slot| {
            slot.map(|(until, wait)| {
                let until = if until >= wall {
                    now + Duration::from_secs(until - wall)
                } else {
                    now.checked_sub(Duration::from_secs(wall - until)).unwrap_or(now)
                };
                (until, Duration::from_secs(wait).min(BACKOFF_MAX))
            })
        }))
    }

    fn save(&self) {
        let Some(path) = Self::file() else { return };
        if crate::settings::read_only() {
            return;
        }
        let (now, wall) = (Instant::now(), epoch_secs(SystemTime::now()));
        let saved = self.0.map(|slot| {
            slot.map(|(until, wait)| {
                let until = match until.checked_duration_since(now) {
                    Some(left) => wall + left.as_secs(),
                    None => wall.saturating_sub(now.duration_since(until).as_secs()),
                };
                (until, wait.as_secs())
            })
        });
        if let Ok(json) = serde_json::to_string(&saved) {
            let _ = std::fs::write(path, json);
        }
    }
}

fn epoch_secs(t: SystemTime) -> u64 {
    t.duration_since(SystemTime::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

impl Engine {
    /// Keyless Google: the endpoints not cooling down, in `mode` order,
    /// raced.
    pub(super) fn google_keyless(&self, mode: GoogleMode, text: &str, from: &str, to: &str) -> Result<Translation, Error> {
        let now = Instant::now();
        let endpoints: Vec<Endpoint> = {
            let backoff = self.google_backoff.lock().unwrap();
            Endpoint::order(mode).iter().copied().filter(|e| !backoff.cooling(*e, now)).collect()
        };
        if endpoints.is_empty() {
            return Err(Error::Busy);
        }
        let this = self.me.upgrade().expect("the engine lives in an Arc");
        let (text, from, to) = (text.to_owned(), from.to_owned(), to.to_owned());
        let list = endpoints.clone();
        let raced = hedge::race(endpoints.len(), |_| ENDPOINT_HEDGE, Arc::new(move |i| {
            let e = list[i];
            crate::timing(&format!("google: try {e:?}"));
            let result = match e {
                Endpoint::Api => this.google_single(FREE_URL, &text, &from, &to),
                Endpoint::Site => this.google_single(SITE_URL, &text, &from, &to),
                Endpoint::Dict => this.google_dict(&text, &from, &to),
                Endpoint::Mobile => this.google_mobile(&text, &from, &to),
            };
            let mut backoff = this.google_backoff.lock().unwrap();
            match &result {
                Ok(_) if backoff.cooling_or_set(e) => {
                    backoff.ok(e);
                    backoff.save();
                }
                Err(Error::Busy) => {
                    crate::timing(&format!("google: {e:?} busy"));
                    backoff.busy(e, Instant::now());
                    backoff.save();
                }
                _ => {}
            }
            result
        }));
        raced.map_err(|errors| {
            let errors: Vec<Error> = errors.into_iter().flatten().collect();
            // A real error says more than "busy"; all offline = offline.
            errors
                .iter()
                .find(|e| !matches!(e, Error::Busy | Error::Offline))
                .cloned()
                .unwrap_or(if errors.iter().all(|e| matches!(e, Error::Offline)) { Error::Offline } else { Error::Busy })
        })
    }

    fn google_single(&self, url: &str, text: &str, from: &str, to: &str) -> Result<Translation, Error> {
        let req = self
            .http
            .post(url)
            .query(&[
                ("client", "gtx"),
                ("sl", from),
                ("tl", to),
                ("hl", "en"),
                ("dt", "t"),
                ("dt", "bd"),
                ("dt", "at"),
                ("dj", "1"),
                ("ie", "UTF-8"),
                ("oe", "UTF-8"),
            ])
            .form(&[("q", text)]);
        let (status, body) = send(req)?;
        check(status, &body)?;
        parse_free(&body, text, to)
    }

    /// `clients5.google.com/translate_a/t?client=dict-chrome-ex`.
    fn google_dict(&self, text: &str, from: &str, to: &str) -> Result<Translation, Error> {
        let req = self
            .http
            .post(DICT_URL)
            .query(&[("client", "dict-chrome-ex"), ("sl", from), ("tl", to)])
            .form(&[("q", text)]);
        let (status, body) = send(req)?;
        check(status, &body)?;
        parse_dict(&body, text, to)
    }

    /// The mobile page, `translate.google.com/m?sl=..&tl=..&q=..` (HTML).
    fn google_mobile(&self, text: &str, from: &str, to: &str) -> Result<Translation, Error> {
        let req = self
            .http
            .get(MOBILE_URL)
            .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64)")
            .query(&[("sl", from), ("tl", to), ("hl", "en"), ("q", text)]);
        let (status, body) = send(req)?;
        check(status, &body)?;
        parse_mobile(&body, text, to)
    }

    pub(super) fn google_cloud(&self, key: &str, text: &str, from: &str, to: &str) -> Result<Translation, Error> {
        let mut json = serde_json::json!({ "q": text, "target": to, "format": "text" });
        if from != "auto" {
            json["source"] = from.into();
        }
        // The key goes in a header rather than `?key=`, so it can't end up
        // in a URL that an error message might show.
        let req = self.http.post(CLOUD_URL).header("X-goog-api-key", key).json(&json);
        let (status, body) = send(req)?;
        if !status.is_success() {
            // An invalid key is a 400 with reason API_KEY_INVALID.
            if body.contains("API_KEY_INVALID") || body.contains("API key not valid") {
                return Err(Error::Other(KEY_REJECTED.into()));
            }
            return Err(status_error(status, true));
        }
        parse_cloud(&body, text, to)
    }
}

/// A keyless answer that isn't a translation: an error status, or the
/// "unusual traffic" page Google redirects to (with status 200 after the
/// redirect) — both mean "busy" for rate limiting.
fn check(status: reqwest::StatusCode, body: &str) -> Result<(), Error> {
    if is_captcha(body) {
        return Err(Error::Busy);
    }
    if !status.is_success() {
        return Err(status_error(status, false));
    }
    Ok(())
}

fn is_captcha(body: &str) -> bool {
    body.contains("/sorry/") || body.contains("unusual traffic")
}

/// `translate_a/t` answers `[["перевод","en"]]` with `sl=auto`, or
/// `["перевод"]` with a fixed source language.
fn parse_dict(body: &str, text: &str, to: &str) -> Result<Translation, Error> {
    let v: serde_json::Value = parse(body)?;
    let first = v.get(0).ok_or_else(super::empty)?;
    let (translated, src) = match first {
        serde_json::Value::String(t) => (t.clone(), None),
        serde_json::Value::Array(a) => (
            a.first().and_then(|x| x.as_str()).unwrap_or_default().to_owned(),
            a.get(1).and_then(|x| x.as_str()).map(lang::ours),
        ),
        _ => return Err(super::empty()),
    };
    Ok(Translation {
        source: text.to_owned(),
        text: translated,
        src_lang: src,
        tgt_lang: to.to_owned(),
        is_word: false,
        dict: Vec::new(),
        provider: ProviderKind::Google,
    })
}

/// The text of `<div class="result-container">…</div>` on the mobile page.
fn parse_mobile(body: &str, text: &str, to: &str) -> Result<Translation, Error> {
    let start = body.find(r#"class="result-container">"#).ok_or_else(super::empty)?;
    let rest = &body[start..];
    let rest = &rest[rest.find('>').map_or(0, |i| i + 1)..];
    let end = rest.find("</div>").ok_or_else(super::empty)?;
    Ok(Translation {
        source: text.to_owned(),
        text: html_unescape(&rest[..end]),
        src_lang: None,
        tgt_lang: to.to_owned(),
        is_word: false,
        dict: Vec::new(),
        provider: ProviderKind::Google,
    })
}

fn html_unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let Some(end) = rest.find(';').filter(|&e| e <= 10) else {
            out.push('&');
            rest = &rest[1..];
            continue;
        };
        let entity = &rest[1..end];
        let c = match entity {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            e if e.starts_with("#x") => u32::from_str_radix(&e[2..], 16).ok().and_then(char::from_u32),
            e if e.starts_with('#') => e[1..].parse().ok().and_then(char::from_u32),
            _ => None,
        };
        match c {
            Some(c) => out.push(c),
            None => out.push_str(&rest[..=end]),
        }
        rest = &rest[end + 1..];
    }
    out.push_str(rest);
    out
}

/// Parses a `translate_a/single` response (`dj=1`).
fn parse_free(body: &str, text: &str, to: &str) -> Result<Translation, Error> {
    #[derive(Deserialize)]
    struct Resp {
        #[serde(default)]
        sentences: Vec<Sentence>,
        #[serde(default)]
        dict: Vec<Dict>,
        #[serde(default)]
        alternative_translations: Vec<Alt>,
        src: Option<String>,
    }
    #[derive(Deserialize)]
    struct Alt {
        #[serde(default)]
        alternative: Vec<AltWord>,
    }
    #[derive(Deserialize)]
    struct AltWord {
        word_postproc: Option<String>,
    }
    #[derive(Deserialize)]
    struct Sentence {
        trans: Option<String>,
    }
    #[derive(Deserialize)]
    struct Dict {
        pos: String,
        #[serde(default)]
        terms: Vec<String>,
    }

    let r: Resp = parse(body)?;
    let translated: String = r.sentences.iter().filter_map(|s| s.trans.as_deref()).collect();
    let mut dict: Vec<DictGroup> = r
        .dict
        .into_iter()
        .filter(|d| !d.terms.is_empty())
        .map(|d| DictGroup { pos: d.pos, terms: d.terms })
        .collect();
    // For a single segment, the alternative translations are good
    // synonyms too; fold them into the first dictionary group.
    if let [alt] = r.alternative_translations.as_slice() {
        let words = alt.alternative.iter().filter_map(|a| a.word_postproc.clone());
        if dict.is_empty() {
            dict.push(DictGroup { pos: String::new(), terms: Vec::new() });
        }
        let first = &mut dict[0];
        for w in words {
            if !first.terms.iter().any(|t| t.to_lowercase() == w.to_lowercase()) {
                first.terms.push(w);
            }
        }
        dict.retain(|g| !g.terms.is_empty());
    }
    Ok(Translation {
        source: text.to_owned(),
        text: translated,
        src_lang: r.src.as_deref().map(lang::ours),
        tgt_lang: to.to_owned(),
        is_word: false,
        dict,
        provider: ProviderKind::Google,
    })
}

/// Parses a Cloud Translation v2 response.
fn parse_cloud(body: &str, text: &str, to: &str) -> Result<Translation, Error> {
    #[derive(Deserialize)]
    struct Resp {
        data: Data,
    }
    #[derive(Deserialize)]
    struct Data {
        translations: Vec<Item>,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Item {
        translated_text: String,
        detected_source_language: Option<String>,
    }

    let r: Resp = parse(body)?;
    let item = r.data.translations.into_iter().next().ok_or_else(super::empty)?;
    Ok(Translation {
        source: text.to_owned(),
        text: item.translated_text,
        src_lang: item.detected_source_language.as_deref().map(lang::ours),
        tgt_lang: to.to_owned(),
        is_word: false,
        dict: Vec::new(),
        provider: ProviderKind::Google,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn free_response() {
        let body = r#"{"sentences":[{"trans":"Эффективно","orig":"effectively"}],
            "dict":[{"pos":"adverb","terms":["эффективно","фактически"]}],
            "alternative_translations":[{"alternative":[{"word_postproc":"Эффективно"},{"word_postproc":"действенно"}]}],
            "src":"en"}"#;
        let t = parse_free(body, "effectively", "ru").unwrap();
        assert_eq!(t.text, "Эффективно");
        assert_eq!(t.src_lang.as_deref(), Some("en"));
        assert_eq!(t.pos(), Some("adverb"));
        assert_eq!(t.dict[0].terms, ["эффективно", "фактически", "действенно"]);
        assert_eq!(t.provider, ProviderKind::Google);
    }

    #[test]
    fn free_response_sentences_join() {
        let body = r#"{"sentences":[{"trans":"Привет. "},{"trans":"Как дела?"},{"translit":"x"}],"src":"zh-CN"}"#;
        let t = parse_free(body, "Hi. How are you?", "ru").unwrap();
        assert_eq!(t.text, "Привет. Как дела?");
        assert_eq!(t.src_lang.as_deref(), Some("zh-CN"));
        assert!(t.dict.is_empty());
    }

    #[test]
    fn dict_and_mobile_responses() {
        let t = parse_dict(r#"[["привет","en"]]"#, "hello", "ru").unwrap();
        assert_eq!((t.text.as_str(), t.src_lang.as_deref()), ("привет", Some("en")));
        let t = parse_dict(r#"["привет"]"#, "hello", "ru").unwrap();
        assert_eq!((t.text.as_str(), t.src_lang), ("привет", None));
        let html = r#"<div class="other">x</div><div class="result-container">Tom &amp; Jerry&#39;s &quot;show&quot;</div>"#;
        assert_eq!(parse_mobile(html, "x", "en").unwrap().text, r#"Tom & Jerry's "show""#);
        assert!(parse_mobile("<html>no result</html>", "x", "en").is_err());
        assert!(is_captcha(r#"<a href="https://www.google.com/sorry/index?continue=">"#));
        assert!(matches!(check(reqwest::StatusCode::OK, "Our systems have detected unusual traffic"), Err(Error::Busy)));
    }

    #[test]
    fn backoff_doubles_and_resets() {
        let mut b = Backoff::default();
        let now = Instant::now();
        assert!(!b.cooling(Endpoint::Api, now));
        b.busy(Endpoint::Api, now);
        assert!(b.cooling(Endpoint::Api, now + Duration::from_secs(59)));
        assert!(!b.cooling(Endpoint::Api, now + Duration::from_secs(61)));
        assert!(!b.cooling(Endpoint::Dict, now));
        b.busy(Endpoint::Api, now);
        assert!(b.cooling(Endpoint::Api, now + Duration::from_secs(119)));
        b.ok(Endpoint::Api);
        assert!(!b.cooling(Endpoint::Api, now));
        assert_eq!(Endpoint::order(GoogleMode::Web)[0], Endpoint::Mobile);
    }

    #[test]
    fn cloud_response() {
        let body = r#"{"data":{"translations":[{"translatedText":"Hallo","detectedSourceLanguage":"en"}]}}"#;
        let t = parse_cloud(body, "Hello", "de").unwrap();
        assert_eq!(t.text, "Hallo");
        assert_eq!(t.src_lang.as_deref(), Some("en"));
        assert!(parse_cloud(r#"{"data":{"translations":[]}}"#, "Hello", "de").is_err());
        assert!(parse_cloud("<html>", "Hello", "de").is_err());
    }
}
