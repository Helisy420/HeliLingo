//! Google: the keyless web endpoints (dictionary data included), or Cloud
//! Translation v2 when the user has a key.
//!
//! The keyless endpoints are rate-limited per IP: a 429, or a redirect to
//! the "unusual traffic" page, means busy. Four endpoints are tried in
//! order — the API host, the main site's copy of it, the Chrome dictionary
//! endpoint and the mobile HTML page — each starting 450 ms after the
//! previous one, or at once when everything running has failed. The first
//! answer wins. A busy endpoint is skipped for 1 minute, doubling up to 30;
//! the cool-downs are saved to a file, so a restart doesn't knock on a
//! blocked endpoint first again.
//!
//! [`GoogleMode`] (Settings → Провайдеры): Auto = the key when there is one,
//! falling back to keyless when the key fails; Web = keyless only; Api =
//! the key only.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use futures_util::FutureExt;
use futures_util::future::BoxFuture;
use futures_util::stream::{FuturesUnordered, StreamExt};
use hl_core::lang::{self, AUTO};
use hl_core::{DictEntry, DictGroup, Engine, EngineError, EngineId, Job, Output, Sink};
use reqwest::{Client, RequestBuilder};
use serde::Deserialize;

use crate::http;

const API_URL: &str = "https://translate.googleapis.com/translate_a/single";
const SITE_URL: &str = "https://translate.google.com/translate_a/single";
const DICT_URL: &str = "https://clients5.google.com/translate_a/t";
const MOBILE_URL: &str = "https://translate.google.com/m";
const CLOUD_URL: &str = "https://translation.googleapis.com/language/translate/v2";
/// Head start of each endpoint over the next.
const STAGGER: Duration = Duration::from_millis(450);
const COOL_FIRST: Duration = Duration::from_secs(60);
const COOL_MAX: Duration = Duration::from_secs(30 * 60);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Endpoint {
    Api,
    Site,
    Dict,
    Mobile,
}

const ENDPOINTS: [Endpoint; 4] = [Endpoint::Api, Endpoint::Site, Endpoint::Dict, Endpoint::Mobile];

/// Which Google the engine uses.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GoogleMode {
    #[default]
    Auto,
    Web,
    Api,
}

type Cooling = [Option<(Instant, Duration)>; 4];

pub struct Google {
    http: Client,
    /// Google Cloud Translation key.
    key: Option<String>,
    mode: GoogleMode,
    /// Per endpoint: skipped until, and the last wait.
    cooling: Mutex<Cooling>,
    /// Where the cool-downs are kept between runs.
    cool_file: Option<PathBuf>,
}

impl Google {
    pub fn new(http: Client, key: Option<String>) -> Self {
        Self {
            http,
            key: key.filter(|k| !k.trim().is_empty()),
            mode: GoogleMode::Auto,
            cooling: Mutex::default(),
            cool_file: None,
        }
    }

    pub fn with_mode(mut self, mode: GoogleMode) -> Self {
        self.mode = mode;
        self
    }

    /// Loads the saved cool-downs and keeps saving them to `path`.
    pub fn with_cool_file(mut self, path: PathBuf) -> Self {
        if let Ok(text) = std::fs::read_to_string(&path) {
            *self.cooling.get_mut().unwrap() = cooling_from_json(&text, Instant::now(), SystemTime::now());
        }
        self.cool_file = Some(path);
        self
    }

    fn save_cooling(&self, c: &Cooling) {
        if let Some(path) = &self.cool_file {
            let _ = std::fs::write(path, cooling_to_json(c, Instant::now(), SystemTime::now()));
        }
    }

    async fn keyless(&self, job: &Job) -> Result<Output, EngineError> {
        let now = Instant::now();
        let order: Vec<usize> = {
            let c = self.cooling.lock().unwrap();
            (0..ENDPOINTS.len()).filter(|&i| !c[i].is_some_and(|(until, _)| until > now)).collect()
        };
        let Some(&first) = order.first() else {
            return Err(EngineError::RateLimited);
        };

        let mut running = FuturesUnordered::new();
        running.push(self.try_endpoint(first, job));
        let mut next = 1;
        let mut errors = Vec::new();
        loop {
            let more = next < order.len();
            tokio::select! {
                Some((i, r)) = running.next() => match r {
                    Ok(o) => {
                        let mut c = self.cooling.lock().unwrap();
                        if c[i].take().is_some() {
                            self.save_cooling(&c);
                        }
                        return Ok(o);
                    }
                    Err(e) => {
                        tracing::debug!(endpoint = ?ENDPOINTS[i], error = %e, "google endpoint failed");
                        if e == EngineError::RateLimited {
                            let mut c = self.cooling.lock().unwrap();
                            let wait = c[i].map_or(COOL_FIRST, |(_, w)| (w * 2).min(COOL_MAX));
                            c[i] = Some((Instant::now() + wait, wait));
                            self.save_cooling(&c);
                        }
                        errors.push(e);
                        if running.is_empty() {
                            if !more {
                                break;
                            }
                            running.push(self.try_endpoint(order[next], job));
                            next += 1;
                        }
                    }
                },
                _ = tokio::time::sleep(STAGGER), if more => {
                    running.push(self.try_endpoint(order[next], job));
                    next += 1;
                }
            }
        }
        // A specific error says more than "busy" or "no connection".
        Err(errors
            .iter()
            .find(|e| !matches!(e, EngineError::RateLimited | EngineError::Network(_)))
            .or_else(|| errors.iter().find(|e| **e == EngineError::RateLimited))
            .or(errors.first())
            .cloned()
            .unwrap_or(EngineError::RateLimited))
    }

    fn try_endpoint<'a>(&'a self, i: usize, job: &'a Job) -> BoxFuture<'a, (usize, Result<Output, EngineError>)> {
        async move {
            let r = match ENDPOINTS[i] {
                Endpoint::Api => self.single(API_URL, job).await,
                Endpoint::Site => self.single(SITE_URL, job).await,
                Endpoint::Dict => self.dict(job).await,
                Endpoint::Mobile => self.mobile(job).await,
            };
            (i, r)
        }
        .boxed()
    }

    /// Sends and turns throttling (a redirect or the captcha page) into
    /// `RateLimited`.
    async fn fetch(&self, req: RequestBuilder) -> Result<String, EngineError> {
        let resp = req.send().await.map_err(http::network)?;
        let status = resp.status();
        let body = resp.text().await.map_err(http::network)?;
        if status.is_redirection() || is_captcha(&body) {
            return Err(EngineError::RateLimited);
        }
        if !status.is_success() {
            return Err(http::status(status, false));
        }
        Ok(body)
    }

    /// `translate_a/single?client=gtx&dj=1`, with dictionary data.
    async fn single(&self, url: &str, job: &Job) -> Result<Output, EngineError> {
        let req = self
            .http
            .post(url)
            .query(&[
                ("client", "gtx"),
                ("sl", job.source.as_str()),
                ("tl", job.target.as_str()),
                ("hl", "en"),
                ("dt", "t"),
                ("dt", "bd"),
                ("dt", "at"),
                ("dt", "ex"),
                ("dt", "rm"),
                ("dj", "1"),
                ("ie", "UTF-8"),
                ("oe", "UTF-8"),
            ])
            .form(&[("q", job.text.as_str())]);
        parse_single(&self.fetch(req).await?)
    }

    /// `clients5…/translate_a/t?client=dict-chrome-ex`.
    async fn dict(&self, job: &Job) -> Result<Output, EngineError> {
        let req = self
            .http
            .post(DICT_URL)
            .query(&[("client", "dict-chrome-ex"), ("sl", job.source.as_str()), ("tl", job.target.as_str())])
            .form(&[("q", job.text.as_str())]);
        parse_dict(&self.fetch(req).await?)
    }

    /// The plain-HTML mobile page.
    async fn mobile(&self, job: &Job) -> Result<Output, EngineError> {
        let req = self
            .http
            .get(MOBILE_URL)
            .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64)")
            .query(&[("sl", job.source.as_str()), ("tl", job.target.as_str()), ("hl", "en"), ("q", job.text.as_str())]);
        parse_mobile(&self.fetch(req).await?)
    }

    async fn cloud(&self, key: &str, job: &Job) -> Result<Output, EngineError> {
        let mut body = serde_json::json!({ "q": job.text, "target": job.target, "format": "text" });
        if job.source != AUTO {
            body["source"] = job.source.clone().into();
        }
        // The key goes in a header, never in the URL.
        let resp = self
            .http
            .post(CLOUD_URL)
            .header("X-goog-api-key", key)
            .json(&body)
            .send()
            .await
            .map_err(http::network)?;
        let status = resp.status();
        let text = resp.text().await.map_err(http::network)?;
        if !status.is_success() {
            if text.contains("API_KEY_INVALID") || text.contains("API key not valid") {
                return Err(EngineError::Unauthorized);
            }
            return Err(http::status(status, true));
        }
        parse_cloud(&text)
    }
}

impl Engine for Google {
    fn id(&self) -> EngineId {
        EngineId::new("google")
    }

    fn translate(self: Arc<Self>, job: Arc<Job>, _sink: Sink) -> BoxFuture<'static, Result<Output, EngineError>> {
        async move {
            match (self.mode, self.key.clone()) {
                (GoogleMode::Web, _) | (GoogleMode::Auto, None) => self.keyless(&job).await,
                (GoogleMode::Api, None) => Err(EngineError::NoKey),
                (GoogleMode::Api, Some(key)) => self.cloud(&key, &job).await,
                (GoogleMode::Auto, Some(key)) => match self.cloud(&key, &job).await {
                    // A dead or exhausted key: the free endpoints still work.
                    Err(EngineError::Unauthorized | EngineError::PaymentRequired | EngineError::RateLimited) => {
                        self.keyless(&job).await
                    }
                    r => r,
                },
            }
        }
        .boxed()
    }
}

fn is_captcha(body: &str) -> bool {
    body.contains("/sorry/") || body.contains("unusual traffic")
}

fn unexpected() -> EngineError {
    EngineError::Server("unexpected answer".into())
}

/// `dj=1` answer: `sentences[].trans`, `src`, `dict[].terms` (words) and
/// `alternative_translations` (other wordings of a one-segment text).
fn parse_single(body: &str) -> Result<Output, EngineError> {
    #[derive(Deserialize)]
    struct Resp {
        #[serde(default)]
        sentences: Vec<Sentence>,
        #[serde(default)]
        dict: Vec<Dict>,
        #[serde(default)]
        alternative_translations: Vec<Alt>,
        src: Option<String>,
        #[serde(default)]
        examples: Examples,
    }
    #[derive(Deserialize)]
    struct Sentence {
        trans: Option<String>,
        src_translit: Option<String>,
    }
    #[derive(Deserialize)]
    struct Dict {
        #[serde(default)]
        pos: String,
        #[serde(default)]
        terms: Vec<String>,
        #[serde(default)]
        entry: Vec<Entry>,
    }
    #[derive(Deserialize)]
    struct Entry {
        word: String,
        #[serde(default)]
        reverse_translation: Vec<String>,
    }
    #[derive(Deserialize, Default)]
    struct Examples {
        #[serde(default)]
        example: Vec<Example>,
    }
    #[derive(Deserialize)]
    struct Example {
        text: String,
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

    let r: Resp = serde_json::from_str(body).map_err(|_| unexpected())?;
    let text: String = r.sentences.iter().filter_map(|s| s.trans.as_deref()).collect();
    if text.trim().is_empty() {
        return Err(unexpected());
    }
    let mut alternatives: Vec<String> = Vec::new();
    let mut add = |w: String| {
        let w = w.trim().to_owned();
        let lower = w.to_lowercase();
        if !w.is_empty() && lower != text.trim().to_lowercase() && !alternatives.iter().any(|a| a.to_lowercase() == lower) {
            alternatives.push(w);
        }
    };
    if let [alt] = r.alternative_translations.as_slice() {
        alt.alternative.iter().filter_map(|a| a.word_postproc.clone()).for_each(&mut add);
    }
    r.dict.iter().flat_map(|d| d.terms.iter().cloned()).for_each(&mut add);
    let dict = r
        .dict
        .into_iter()
        .map(|d| DictGroup {
            pos: d.pos,
            terms: d.terms.into_iter().take(8).collect(),
            entries: d
                .entry
                .into_iter()
                .take(5)
                .map(|e| DictEntry { word: e.word, gloss: e.reverse_translation.into_iter().take(3).collect() })
                .collect(),
        })
        .filter(|g| !g.terms.is_empty())
        .collect();
    let transcription = r.sentences.iter().find_map(|s| s.src_translit.clone()).filter(|t| !t.trim().is_empty());
    let examples = r.examples.example.into_iter().take(3).map(|e| strip_tags(&e.text)).collect();
    Ok(Output {
        text,
        source_lang: r.src.as_deref().map(lang::normalize),
        alternatives,
        dict,
        transcription,
        examples,
    })
}

/// `[["перевод","en"]]` with `sl=auto`, `["перевод"]` with a fixed source.
fn parse_dict(body: &str) -> Result<Output, EngineError> {
    let v: serde_json::Value = serde_json::from_str(body).map_err(|_| unexpected())?;
    let (text, source_lang) = match &v[0] {
        serde_json::Value::String(t) => (t.clone(), None),
        serde_json::Value::Array(a) => (
            a.first().and_then(|x| x.as_str()).unwrap_or_default().to_owned(),
            a.get(1).and_then(|x| x.as_str()).map(lang::normalize),
        ),
        _ => return Err(unexpected()),
    };
    if text.trim().is_empty() {
        return Err(unexpected());
    }
    Ok(Output { text, source_lang, ..Output::default() })
}

/// The text of `<div class="result-container">…</div>`.
fn parse_mobile(body: &str) -> Result<Output, EngineError> {
    let start = body.find(r#"class="result-container">"#).ok_or_else(unexpected)?;
    let rest = &body[start..];
    let rest = &rest[rest.find('>').map_or(0, |i| i + 1)..];
    let end = rest.find("</div>").ok_or_else(unexpected)?;
    Ok(Output { text: html_unescape(&rest[..end]), ..Output::default() })
}

/// "we're going by <b>car</b>" → "we're going by car".
fn strip_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut tag = false;
    for c in s.chars() {
        match c {
            '<' => tag = true,
            '>' => tag = false,
            c if !tag => out.push(c),
            _ => {}
        }
    }
    html_unescape(&out)
}

fn html_unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let decoded = rest.find(';').filter(|&e| e <= 10).and_then(|end| {
            let c = match &rest[1..end] {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" => Some('\''),
                e if e.starts_with("#x") => u32::from_str_radix(&e[2..], 16).ok().and_then(char::from_u32),
                e if e.starts_with('#') => e[1..].parse().ok().and_then(char::from_u32),
                _ => None,
            };
            c.map(|c| (c, end))
        });
        match decoded {
            Some((c, end)) => {
                out.push(c);
                rest = &rest[end + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

fn parse_cloud(body: &str) -> Result<Output, EngineError> {
    let v: serde_json::Value = serde_json::from_str(body).map_err(|_| unexpected())?;
    let t = &v["data"]["translations"][0];
    let text = t["translatedText"].as_str().ok_or_else(unexpected)?.to_owned();
    let source_lang = t["detectedSourceLanguage"].as_str().map(lang::normalize);
    Ok(Output { text, source_lang, ..Output::default() })
}

/// `[[unix_until, wait_secs] | null; 4]`.
fn cooling_to_json(c: &Cooling, now: Instant, wall: SystemTime) -> String {
    let unix = |t: SystemTime| t.duration_since(SystemTime::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let items: Vec<serde_json::Value> = c
        .iter()
        .map(|e| match e {
            Some((until, wait)) if *until > now => {
                serde_json::json!([unix(wall + (*until - now)), wait.as_secs()])
            }
            _ => serde_json::Value::Null,
        })
        .collect();
    serde_json::Value::Array(items).to_string()
}

fn cooling_from_json(text: &str, now: Instant, wall: SystemTime) -> Cooling {
    let mut c = Cooling::default();
    let Ok(serde_json::Value::Array(items)) = serde_json::from_str(text) else { return c };
    for (slot, item) in c.iter_mut().zip(items) {
        let (Some(until), Some(wait)) = (item[0].as_u64(), item[1].as_u64()) else { continue };
        let until = SystemTime::UNIX_EPOCH + Duration::from_secs(until);
        if let Ok(left) = until.duration_since(wall) {
            *slot = Some((now + left.min(COOL_MAX), Duration::from_secs(wait).min(COOL_MAX)));
        }
    }
    c
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client;

    #[test]
    fn parses_a_sentence_with_alternatives() {
        let body = r#"{"sentences":[{"trans":"Где ближайшая аптека?","orig":"Where is the nearest pharmacy?"}],
            "src":"en","alternative_translations":[{"alternative":[
            {"word_postproc":"Где ближайшая аптека?"},{"word_postproc":"Где находится ближайшая аптека?"}]}]}"#;
        let o = parse_single(body).unwrap();
        assert_eq!(o.text, "Где ближайшая аптека?");
        assert_eq!(o.source_lang.as_deref(), Some("en"));
        assert_eq!(o.alternatives, vec!["Где находится ближайшая аптека?"]);
    }

    #[test]
    fn parses_word_dictionary() {
        let body = r#"{"sentences":[{"trans":"кошка"}],"src":"en",
            "dict":[{"pos":"noun","terms":["кошка","кот","кошечка"]}]}"#;
        let o = parse_single(body).unwrap();
        assert_eq!(o.alternatives, vec!["кот", "кошечка"]);
        assert_eq!(o.dict, vec![DictGroup { pos: "noun".into(), terms: vec!["кошка".into(), "кот".into(), "кошечка".into()], entries: vec![] }]);
        let body = r#"{"sentences":[{"trans":"машина","orig":"car"},{"translit":"mashina","src_translit":"kär"}],"src":"en",
            "dict":[{"pos":"noun","terms":["автомобиль","машина"],"entry":[{"word":"автомобиль","reverse_translation":["car","vehicle","automobile","auto"]}]}],
            "examples":{"example":[{"text":"we're going by <b>car</b>"}]}}"#;
        let o = parse_single(body).unwrap();
        assert_eq!(o.transcription.as_deref(), Some("kär"));
        assert_eq!(o.examples, ["we're going by car"]);
        assert_eq!(o.dict[0].entries[0].gloss, ["car", "vehicle", "automobile"]);
    }

    #[test]
    fn cooldowns_survive_a_restart() {
        let (now, wall) = (Instant::now(), SystemTime::now());
        let mut c = Cooling::default();
        c[1] = Some((now + Duration::from_secs(120), Duration::from_secs(120)));
        c[2] = Some((now - Duration::from_secs(1), Duration::from_secs(60)));
        let json = cooling_to_json(&c, now, wall);
        let back = cooling_from_json(&json, now, wall);
        assert!(back[0].is_none() && back[2].is_none() && back[3].is_none());
        let (until, wait) = back[1].unwrap();
        assert_eq!(wait, Duration::from_secs(120));
        assert!(until > now + Duration::from_secs(118));
        // Expired by the time the app starts again.
        assert!(cooling_from_json(&json, now, wall + Duration::from_secs(200))[1].is_none());
        assert_eq!(cooling_from_json("garbage", now, wall), Cooling::default());
    }

    #[test]
    fn parses_dict_and_mobile() {
        assert_eq!(parse_dict(r#"[["Заседание перенесено.","en"]]"#).unwrap().source_lang.as_deref(), Some("en"));
        assert_eq!(parse_dict(r#"["кошка"]"#).unwrap().text, "кошка");
        let html = r#"<div class="result-container">Tom &amp; Jerry &#39;hi&#39;</div>"#;
        assert_eq!(parse_mobile(html).unwrap().text, "Tom & Jerry 'hi'");
    }

    #[test]
    fn captcha_page_is_rate_limit() {
        assert!(is_captcha("<html>Our systems have detected unusual traffic</html>"));
    }

    /// Live request; run with `cargo test -p hl-providers -- --ignored`.
    #[tokio::test]
    #[ignore]
    async fn live_keyless() {
        let g = Arc::new(Google::new(client(), None));
        let job = Arc::new(Job { text: "The meeting has been moved to Thursday.".into(), source: "auto".into(), target: "ru".into(), ..Job::default() });
        let t0 = Instant::now();
        let o = g.translate(job, Sink::discard()).await.unwrap();
        println!("{o:?} in {:?}", t0.elapsed());
        assert!(o.text.to_lowercase().contains("четверг"));
    }
}
