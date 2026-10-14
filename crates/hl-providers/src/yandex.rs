//! Yandex: keyless by default, Yandex Cloud Translate v2 with a key.
//!
//! Keyless uses the endpoint the Yandex Translate mobile app talks to (a
//! random client id per run, no key) and, for words and short phrases,
//! the web dictionary (parts of speech, translations, synonyms) — the data
//! translate.yandex.ru shows under a word. The dictionary runs alongside
//! the translation and is dropped if it's late. A captcha or limit answer
//! is `RateLimited`: the router benches Yandex and moves on.

use std::sync::Arc;
use std::time::Duration;

use futures_util::FutureExt;
use futures_util::future::BoxFuture;
use hl_core::lang::{self, AUTO};
use hl_core::{DictEntry, DictGroup, Engine, EngineError, EngineId, Job, Output, Sink};
use reqwest::{Client, StatusCode};
use serde::Deserialize;

use crate::http;

const CLOUD_URL: &str = "https://translate.api.cloud.yandex.net/translate/v2/translate";
const APP_URL: &str = "https://translate.yandex.net/api/v1/tr.json/translate";
const APP_AGENT: &str = "ru.yandex.translate/24.10.30 (Android 14)";
const DICT_URL: &str = "https://dictionary.yandex.net/dicservice.json/lookup";
/// The dictionary is extra: don't hold the translation up for it.
const DICT_GRACE: Duration = Duration::from_millis(600);

pub struct Yandex {
    http: Client,
    /// Yandex Cloud API key and folder id (folder needed for user-account keys).
    key: Option<(String, String)>,
    client_id: String,
}

impl Yandex {
    pub fn new(http: Client, key: Option<String>, folder: &str) -> Self {
        let key = key.filter(|k| !k.trim().is_empty()).map(|k| (k, folder.trim().to_owned()));
        Self { http, key, client_id: client_id() }
    }

    async fn keyless(&self, job: &Job) -> Result<Output, EngineError> {
        let target = code(&job.target);
        // "en" alone = detect the source; "ru-en" = from Russian.
        let pair = if job.source == AUTO { target.clone() } else { format!("{}-{target}", code(&job.source)) };
        let req = self
            .http
            .post(APP_URL)
            .header("User-Agent", APP_AGENT)
            .query(&[("ucid", self.client_id.as_str()), ("srv", "android"), ("format", "text")])
            .form(&[("text", job.text.as_str()), ("lang", pair.as_str())]);
        let translate = async {
            let resp = req.send().await.map_err(http::network)?;
            let status = resp.status();
            let body = resp.text().await.map_err(http::network)?;
            parse_app(status, &body)
        };
        if !wants_dictionary(&job.text) {
            return translate.await.map(|(o, _)| o);
        }
        // The dictionary needs the pair: start it with the translation when
        // the source is known or guessed, else after Yandex names it.
        let guess = match (&job.source, &job.hint) {
            (s, _) if s != AUTO => Some(pair.clone()),
            (_, Some(h)) => Some(format!("{}-{target}", code(h))),
            _ => None,
        };
        if let Some(guess) = guess {
            let (t, d) = tokio::join!(translate, self.dict(&job.text, &guess));
            let (mut out, used) = t?;
            if used == guess || job.source != AUTO {
                d.unwrap_or_default().apply(&mut out);
                return Ok(out);
            }
            if let Ok(Ok(d)) = tokio::time::timeout(DICT_GRACE, self.dict(&job.text, &used)).await {
                d.apply(&mut out);
            }
            return Ok(out);
        }
        let (mut out, used) = translate.await?;
        if let Ok(Ok(d)) = tokio::time::timeout(DICT_GRACE, self.dict(&job.text, &used)).await {
            d.apply(&mut out);
        }
        Ok(out)
    }

    /// Dictionary groups for `text` in `pair` ("en-ru").
    async fn dict(&self, text: &str, pair: &str) -> Result<Dict, EngineError> {
        let resp = self
            .http
            .get(DICT_URL)
            .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64)")
            .timeout(Duration::from_millis(1500))
            .query(&[("ui", "en"), ("srv", "tr-text"), ("text", text), ("type", ""), ("lang", pair), ("flags", "4")])
            .send()
            .await
            .map_err(http::network)?;
        if !resp.status().is_success() {
            return Err(http::status(resp.status(), false));
        }
        parse_dict(&resp.text().await.map_err(http::network)?)
    }

    async fn cloud(&self, key: &str, folder: &str, job: &Job) -> Result<Output, EngineError> {
        let mut body = serde_json::json!({ "texts": [job.text], "targetLanguageCode": code(&job.target) });
        if !folder.is_empty() {
            body["folderId"] = folder.into();
        }
        if job.source != AUTO {
            body["sourceLanguageCode"] = code(&job.source).into();
        }
        let resp = self
            .http
            .post(CLOUD_URL)
            .header("Authorization", format!("Api-Key {key}"))
            .json(&body)
            .send()
            .await
            .map_err(http::network)?;
        let status = resp.status();
        let text = resp.text().await.map_err(http::network)?;
        if !status.is_success() {
            // User-account keys need a folder; the message says so.
            if folder.is_empty() && text.to_ascii_lowercase().contains("folder") {
                return Err(EngineError::Other("folder id required".into()));
            }
            return Err(http::status(status, true));
        }
        parse_cloud(&text)
    }
}

impl Engine for Yandex {
    fn id(&self) -> EngineId {
        EngineId::new("yandex")
    }

    fn translate(self: Arc<Self>, job: Arc<Job>, _sink: Sink) -> BoxFuture<'static, Result<Output, EngineError>> {
        async move {
            match &self.key {
                Some((key, folder)) => self.cloud(key, folder, &job).await,
                None => self.keyless(&job).await,
            }
        }
        .boxed()
    }
}

/// Our code as Yandex spells it.
fn code(c: &str) -> String {
    match c {
        "zh-CN" => "zh".into(),
        c => c.split('-').next().unwrap_or(c).to_owned(),
    }
}

/// A random 32-hex-digit client id, new every run (the app sends one).
fn client_id() -> String {
    use std::hash::{BuildHasher, Hasher};
    let part = |n: u64| {
        let mut h = std::collections::hash_map::RandomState::new().build_hasher();
        h.write_u64(n);
        h.finish()
    };
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos() as u64);
    format!("{:016x}{:016x}", part(nanos), part(nanos.rotate_left(17) ^ u64::from(std::process::id())))
}

/// Words and short phrases get dictionary data.
fn wants_dictionary(text: &str) -> bool {
    text.split_whitespace().count() <= 3 && text.chars().count() <= 40
}

fn unexpected() -> EngineError {
    EngineError::Server("unexpected answer".into())
}

/// `{"code":200,"lang":"ru-en","text":["Hello how are you"]}`: the output
/// and the pair Yandex used (for the dictionary).
fn parse_app(status: StatusCode, body: &str) -> Result<(Output, String), EngineError> {
    #[derive(Deserialize)]
    struct Resp {
        code: u32,
        #[serde(default)]
        lang: String,
        #[serde(default)]
        text: Vec<String>,
    }
    if body.contains("captcha") || body.contains("SmartCaptcha") {
        return Err(EngineError::RateLimited);
    }
    let r: Resp = match serde_json::from_str(body) {
        Ok(r) => r,
        Err(_) if !status.is_success() => return Err(http::status(status, false)),
        Err(_) => return Err(unexpected()),
    };
    match r.code {
        200 => {}
        // Blocked / daily limit / too many requests.
        402..=404 | 429 => return Err(EngineError::RateLimited),
        501 => return Err(EngineError::UnsupportedPair),
        c => return Err(EngineError::Server(format!("code {c}"))),
    }
    let text = r.text.concat();
    if text.trim().is_empty() {
        return Err(unexpected());
    }
    let source_lang = r.lang.split_once('-').map(|(s, _)| lang::normalize(s));
    Ok((Output { text, source_lang, ..Output::default() }, r.lang))
}

/// What the dictionary adds to a translation.
#[derive(Debug, Default, PartialEq)]
struct Dict {
    groups: Vec<DictGroup>,
    /// The source word's transcription ("kæt").
    transcription: Option<String>,
}

impl Dict {
    fn apply(self, out: &mut Output) {
        out.alternatives = flat_terms(&self.groups, &out.text);
        out.dict = self.groups;
        out.transcription = self.transcription;
    }
}

/// `{"def":[{"pos":"noun","ts":"kæt","tr":[{"text":"кошка","syn":[{"text":"кот"}],"mean":[{"text":"feline"}]}]}]}`.
fn parse_dict(body: &str) -> Result<Dict, EngineError> {
    #[derive(Deserialize)]
    struct Resp {
        #[serde(default)]
        def: Vec<Def>,
    }
    #[derive(Deserialize)]
    struct Def {
        #[serde(default)]
        pos: String,
        ts: Option<String>,
        #[serde(default)]
        tr: Vec<Tr>,
    }
    #[derive(Deserialize)]
    struct Tr {
        text: String,
        #[serde(default)]
        syn: Vec<Syn>,
        #[serde(default)]
        mean: Vec<Syn>,
    }
    #[derive(Deserialize)]
    struct Syn {
        text: String,
    }
    let r: Resp = serde_json::from_str(body).map_err(|_| unexpected())?;
    let transcription = r.def.iter().find_map(|d| d.ts.clone()).filter(|t| !t.is_empty());
    let mut groups: Vec<DictGroup> = Vec::new();
    for d in r.def {
        let entries: Vec<DictEntry> = d
            .tr
            .iter()
            .take(5)
            .map(|tr| DictEntry { word: tr.text.clone(), gloss: tr.mean.iter().take(3).map(|m| m.text.clone()).collect() })
            .collect();
        let mut terms: Vec<String> = Vec::new();
        for tr in d.tr {
            for t in std::iter::once(tr.text).chain(tr.syn.into_iter().map(|s| s.text)) {
                if !terms.iter().any(|x| x.to_lowercase() == t.to_lowercase()) {
                    terms.push(t);
                }
            }
        }
        terms.truncate(8);
        if terms.is_empty() {
            continue;
        }
        // The same part of speech twice (different readings): merge.
        match groups.iter_mut().find(|g| g.pos == d.pos) {
            Some(g) => {
                for t in terms {
                    if !g.terms.iter().any(|x| x.to_lowercase() == t.to_lowercase()) {
                        g.terms.push(t);
                    }
                }
                g.entries.extend(entries);
            }
            None => groups.push(DictGroup { pos: d.pos, terms, entries }),
        }
    }
    Ok(Dict { groups, transcription })
}

fn parse_cloud(body: &str) -> Result<Output, EngineError> {
    let v: serde_json::Value = serde_json::from_str(body).map_err(|_| unexpected())?;
    let t = &v["translations"][0];
    let text = t["text"].as_str().ok_or_else(unexpected)?.to_owned();
    let source_lang = t["detectedLanguageCode"].as_str().filter(|c| !c.is_empty()).map(lang::normalize);
    Ok(Output { text, source_lang, ..Output::default() })
}

/// The dictionary as one list without the main translation, for windows
/// that show a flat list.
fn flat_terms(dict: &[DictGroup], main: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for t in dict.iter().flat_map(|g| g.terms.iter()) {
        if t.to_lowercase() != main.trim().to_lowercase() && !out.iter().any(|o| o.to_lowercase() == t.to_lowercase()) {
            out.push(t.clone());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyless_answers() {
        let (o, pair) = parse_app(StatusCode::OK, r#"{"code":200,"lang":"ru-en","nmt_code":200,"text":["Hello how are you"]}"#).unwrap();
        assert_eq!(o.text, "Hello how are you");
        assert_eq!(o.source_lang.as_deref(), Some("ru"));
        assert_eq!(pair, "ru-en");
        assert_eq!(parse_app(StatusCode::OK, r#"{"code":404,"message":"limit"}"#).unwrap_err(), EngineError::RateLimited);
        assert_eq!(parse_app(StatusCode::OK, "<html>SmartCaptcha</html>").unwrap_err(), EngineError::RateLimited);
        assert_eq!(parse_app(StatusCode::OK, r#"{"code":501}"#).unwrap_err(), EngineError::UnsupportedPair);
    }

    #[test]
    fn dictionary_groups() {
        let body = r#"{"head":{},"def":[{"text":"cat","pos":"noun","tr":[{"text":"кошка","syn":[{"text":"кот"}]},{"text":"котенок"}]},
            {"text":"cat","pos":"verb","tr":[{"text":"блевать"}]},{"text":"cat","pos":"noun","tr":[{"text":"Кот"},{"text":"кошечка"}]}]}"#;
        let g = parse_dict(body).unwrap().groups;
        assert_eq!(g.len(), 2);
        assert_eq!(g[0].terms, ["кошка", "кот", "котенок", "кошечка"]);
        assert_eq!(g[1].pos, "verb");
        assert_eq!(flat_terms(&g, "кошка"), ["кот", "котенок", "кошечка", "блевать"]);
    }

    #[test]
    fn cloud_answer_and_codes() {
        let o = parse_cloud(r#"{"translations":[{"text":"你好","detectedLanguageCode":"en"}]}"#).unwrap();
        assert_eq!((o.text.as_str(), o.source_lang.as_deref()), ("你好", Some("en")));
        assert_eq!(code("zh-CN"), "zh");
        assert_eq!(client_id().len(), 32);
        assert!(wants_dictionary("cat") && !wants_dictionary("one two three four"));
    }

    /// `cargo test -p hl-providers -- --ignored yandex`
    #[tokio::test]
    #[ignore]
    async fn live_yandex() {
        let y = Arc::new(Yandex::new(crate::client(), None, ""));
        let job = Arc::new(Job { text: "cat".into(), source: AUTO.into(), target: "ru".into(), hint: Some("en".into()), ..Job::default() });
        let o = y.translate(job, Sink::discard()).await.unwrap();
        println!("{o:?}");
        assert!(o.text.to_lowercase().contains("кош") || o.text.to_lowercase().contains("кот"));
        assert!(!o.dict.is_empty());
    }
}
