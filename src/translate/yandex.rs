//! Yandex: keyless by default, Yandex Cloud Translate v2 when a key is set.
//!
//! Keyless uses the endpoint the Yandex Translate mobile app talks to (a
//! random client id per session, no key) and, for words and short phrases,
//! the web dictionary (part of speech, translations, synonyms), the same
//! data translate.yandex.ru shows under a word. Like keyless Google, an
//! answer that means "too many requests" makes the app leave Yandex alone
//! for a while (1 minute, doubling up to 30) and the next provider is used.

use std::hash::{BuildHasher, Hasher};
use std::time::{Duration, Instant};

use serde::Deserialize;

use super::{DictGroup, Engine, Error, FOLDER_REQUIRED, Translation, lang, parse, send, status_error};
use crate::settings::ProviderKind;

pub const URL: &str = "https://translate.api.cloud.yandex.net/translate/v2/translate";
/// Keyless translation (the mobile app's endpoint).
const APP_URL: &str = "https://translate.yandex.net/api/v1/tr.json/translate";
const APP_AGENT: &str = "ru.yandex.translate/24.10.30 (Android 14)";
/// Keyless dictionary (translate.yandex.ru's word cards).
const DICT_URL: &str = "https://dictionary.yandex.net/dicservice.json/lookup";
/// Warming the keyless connection: the host only.
pub const WARM_URL: &str = "https://translate.yandex.net/";
/// The dictionary is extra: don't hold the translation up for long.
const DICT_TIMEOUT: Duration = Duration::from_millis(1500);
/// Unsupported language pair (Yandex API code 501).
pub const UNSUPPORTED: &str = "Yandex doesn't translate between these languages";

const BACKOFF_START: Duration = Duration::from_secs(60);
const BACKOFF_MAX: Duration = Duration::from_secs(30 * 60);

/// Keyless Yandex answered "busy": until when, and the wait that was used.
#[derive(Default, Debug)]
pub struct Cooldown(Option<(Instant, Duration)>);

impl Cooldown {
    fn cooling(&self, now: Instant) -> bool {
        self.0.is_some_and(|(until, _)| until > now)
    }

    fn busy(&mut self, now: Instant) {
        let next = match self.0 {
            Some((_, wait)) => (wait * 2).min(BACKOFF_MAX),
            None => BACKOFF_START,
        };
        self.0 = Some((now + next, next));
    }

    fn ok(&mut self) {
        self.0 = None;
    }
}

/// A random 32-hex-digit client id, new every session (the app sends one).
pub fn new_client_id() -> String {
    let random = |n: u64| {
        let mut h = std::collections::hash_map::RandomState::new().build_hasher();
        h.write_u64(n ^ std::process::id() as u64);
        h.finish()
    };
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos() as u64);
    format!("{:016x}{:016x}", random(nanos), random(nanos.rotate_left(17)))
}

/// Words and short phrases get dictionary data.
fn wants_dictionary(text: &str) -> bool {
    text.split_whitespace().count() <= 3 && text.chars().count() <= 40
}

impl Engine {
    pub(super) fn yandex(&self, key: &str, folder: &str, text: &str, from: &str, to: &str) -> Result<Translation, Error> {
        let folder = folder.trim();
        let mut json = serde_json::json!({
            "texts": [text],
            "targetLanguageCode": lang::to_provider(ProviderKind::Yandex, to),
        });
        if !folder.is_empty() {
            json["folderId"] = folder.into();
        }
        if from != "auto" {
            json["sourceLanguageCode"] = lang::to_provider(ProviderKind::Yandex, from).into();
        }
        let req = self.http.post(URL).header("Authorization", format!("Api-Key {key}")).json(&json);
        let (status, body) = send(req)?;
        if !status.is_success() {
            // User-account keys need a folder; the service says so in the message.
            if folder.is_empty() && body.to_ascii_lowercase().contains("folder") {
                return Err(Error::Other(FOLDER_REQUIRED.into()));
            }
            return Err(status_error(status, true));
        }
        parse_response(&body, text, to)
    }

    /// Keyless Yandex: the translation, plus dictionary data for a word.
    pub(super) fn yandex_keyless(&self, text: &str, from: &str, to: &str) -> Result<Translation, Error> {
        if self.yandex_backoff.lock().unwrap().cooling(Instant::now()) {
            return Err(Error::Busy);
        }
        let target = lang::to_provider(ProviderKind::Yandex, to);
        // "en" alone = detect the source; "ru-en" = from Russian.
        let pair = if from == "auto" {
            target
        } else {
            format!("{}-{target}", lang::to_provider(ProviderKind::Yandex, from))
        };
        let req = self
            .http
            .post(APP_URL)
            .header("User-Agent", APP_AGENT)
            .query(&[("ucid", self.yandex_client_id.as_str()), ("srv", "android"), ("format", "text")])
            .form(&[("text", text), ("lang", pair.as_str())]);
        let (status, body) = send(req)?;
        let (mut t, used_pair) = match parse_app(status, &body, text, to) {
            Err(Error::Busy) => {
                self.yandex_backoff.lock().unwrap().busy(Instant::now());
                return Err(Error::Busy);
            }
            other => other?,
        };
        self.yandex_backoff.lock().unwrap().ok();
        if wants_dictionary(text) {
            // Best effort: without it the popup just has no alternatives.
            t.dict = self.yandex_dict(text, &used_pair).unwrap_or_default();
        }
        Ok(t)
    }

    /// Dictionary entries for `text` in `pair` ("en-ru").
    fn yandex_dict(&self, text: &str, pair: &str) -> Result<Vec<DictGroup>, Error> {
        let req = self
            .http
            .get(DICT_URL)
            .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64)")
            .timeout(DICT_TIMEOUT)
            .query(&[("ui", "en"), ("srv", "tr-text"), ("text", text), ("type", ""), ("lang", pair), ("flags", "4")]);
        let (status, body) = send(req)?;
        if !status.is_success() {
            return Err(status_error(status, false));
        }
        parse_dict(&body)
    }
}

/// `{"code":200,"lang":"ru-en","text":["Hello how are you"]}`. Returns the
/// translation and the pair Yandex used (for the dictionary).
fn parse_app(status: reqwest::StatusCode, body: &str, text: &str, to: &str) -> Result<(Translation, String), Error> {
    #[derive(Deserialize)]
    struct Resp {
        code: u32,
        #[serde(default)]
        lang: String,
        #[serde(default)]
        text: Vec<String>,
    }
    // A captcha page instead of JSON: rate limited.
    if body.contains("captcha") || body.contains("SmartCaptcha") {
        return Err(Error::Busy);
    }
    let r: Resp = match serde_json::from_str(body) {
        Ok(r) => r,
        Err(_) if !status.is_success() => return Err(status_error(status, false)),
        Err(_) => return Err(super::empty()),
    };
    match r.code {
        200 => {}
        // Blocked / daily limit / too many requests.
        402..=404 | 429 => return Err(Error::Busy),
        501 => return Err(Error::Other(UNSUPPORTED.into())),
        _ => return Err(status_error(reqwest::StatusCode::from_u16(r.code as u16).unwrap_or(status), false)),
    }
    let translated = r.text.concat();
    if translated.trim().is_empty() {
        return Err(super::empty());
    }
    let src = r.lang.split('-').next().filter(|s| !s.is_empty() && r.lang.contains('-')).map(lang::ours);
    Ok((
        Translation {
            source: text.to_owned(),
            text: translated,
            src_lang: src,
            tgt_lang: to.to_owned(),
            is_word: false,
            dict: Vec::new(),
            provider: ProviderKind::Yandex,
        },
        r.lang,
    ))
}

/// `{"def":[{"pos":"adverb","tr":[{"text":"эффективно","syn":[{"text":…}]}]}]}`:
/// one group per part of speech, the translations and their synonyms.
fn parse_dict(body: &str) -> Result<Vec<DictGroup>, Error> {
    #[derive(Deserialize)]
    struct Resp {
        #[serde(default)]
        def: Vec<Def>,
    }
    #[derive(Deserialize)]
    struct Def {
        #[serde(default)]
        pos: String,
        #[serde(default)]
        tr: Vec<Tr>,
    }
    #[derive(Deserialize)]
    struct Tr {
        text: String,
        #[serde(default)]
        syn: Vec<Syn>,
    }
    #[derive(Deserialize)]
    struct Syn {
        text: String,
    }
    let r: Resp = parse(body)?;
    Ok(r.def
        .into_iter()
        .map(|d| {
            let mut terms: Vec<String> = Vec::new();
            for tr in d.tr {
                for t in std::iter::once(tr.text).chain(tr.syn.into_iter().map(|s| s.text)) {
                    if !terms.iter().any(|x| x.eq_ignore_ascii_case(&t)) {
                        terms.push(t);
                    }
                }
            }
            terms.truncate(10);
            DictGroup { pos: d.pos, terms }
        })
        .filter(|g| !g.terms.is_empty())
        .collect())
}

fn parse_response(body: &str, text: &str, to: &str) -> Result<Translation, Error> {
    #[derive(Deserialize)]
    struct Resp {
        translations: Vec<Item>,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Item {
        text: String,
        detected_language_code: Option<String>,
    }

    let r: Resp = parse(body)?;
    let item = r.translations.into_iter().next().ok_or_else(super::empty)?;
    Ok(Translation {
        source: text.to_owned(),
        text: item.text,
        src_lang: item.detected_language_code.as_deref().filter(|c| !c.is_empty()).map(lang::ours),
        tgt_lang: to.to_owned(),
        is_word: false,
        dict: Vec::new(),
        provider: ProviderKind::Yandex,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::StatusCode;

    #[test]
    fn response() {
        let body = r#"{"translations":[{"text":"你好","detectedLanguageCode":"en"}]}"#;
        let t = parse_response(body, "Hello", "zh-CN").unwrap();
        assert_eq!(t.text, "你好");
        assert_eq!(t.src_lang.as_deref(), Some("en"));
        assert_eq!(t.provider, ProviderKind::Yandex);

        let body = r#"{"translations":[{"text":"Привет","detectedLanguageCode":"zh"}]}"#;
        assert_eq!(parse_response(body, "你好", "ru").unwrap().src_lang.as_deref(), Some("zh-CN"));

        // Source given: no detected code.
        let body = r#"{"translations":[{"text":"Hallo"}]}"#;
        assert_eq!(parse_response(body, "Hello", "de").unwrap().src_lang, None);
        assert!(parse_response(r#"{"code":16,"message":"Unknown api key"}"#, "Hello", "de").is_err());
    }

    #[test]
    fn keyless_app_response() {
        // Real answers (ru → en, and auto-detected German).
        let body = r#"{"code":200,"lang":"ru-en","nmt_code":200,"text":["Hello how are you"]}"#;
        let (t, pair) = parse_app(StatusCode::OK, body, "Привет как дела", "en").unwrap();
        assert_eq!(t.text, "Hello how are you");
        assert_eq!(t.src_lang.as_deref(), Some("ru"));
        assert_eq!(pair, "ru-en");
        let body = r#"{"code":200,"lang":"de-ru","nmt_code":200,"text":["Доброе утро, как у тебя дела?"]}"#;
        assert_eq!(parse_app(StatusCode::OK, body, "Guten Morgen", "ru").unwrap().0.src_lang.as_deref(), Some("de"));
        // Limits and captcha pages are "busy"; 501 is an unsupported pair.
        assert!(matches!(parse_app(StatusCode::OK, r#"{"code":404,"message":"limit"}"#, "x", "en"), Err(Error::Busy)));
        assert!(matches!(parse_app(StatusCode::OK, "<html>SmartCaptcha</html>", "x", "en"), Err(Error::Busy)));
        assert!(matches!(parse_app(StatusCode::OK, r#"{"code":501}"#, "x", "en"), Err(Error::Other(m)) if m == UNSUPPORTED));
    }

    #[test]
    fn keyless_dictionary() {
        let body = r#"{"head":{},"def":[{"text":"effectively","pos":"adverb","tr":[{"text":"эффективно","pos":"adverb","syn":[{"text":"результативно"},{"text":"успешно"}]},{"text":"действенно","pos":"adverb"}]},{"text":"effectively","pos":"adjective","tr":[]}]}"#;
        let groups = parse_dict(body).unwrap();
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].pos, "adverb");
        assert_eq!(groups[0].terms, ["эффективно", "результативно", "успешно", "действенно"]);
        assert!(wants_dictionary("effectively") && !wants_dictionary("one two three four"));
    }

    #[test]
    fn cooldown_doubles_and_resets() {
        let t0 = Instant::now();
        let mut c = Cooldown::default();
        assert!(!c.cooling(t0));
        c.busy(t0);
        assert!(c.cooling(t0 + Duration::from_secs(59)) && !c.cooling(t0 + Duration::from_secs(61)));
        c.busy(t0);
        assert!(c.cooling(t0 + Duration::from_secs(119)));
        c.ok();
        assert!(!c.cooling(t0));
        assert_eq!(new_client_id().len(), 32);
    }
}
