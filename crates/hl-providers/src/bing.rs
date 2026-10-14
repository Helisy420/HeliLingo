//! Bing / Microsoft Translator: keyless through bing.com/translator, or
//! Azure Translator v3 with a key (+ region).
//!
//! Keyless: the translator page carries a session (`IG`, the widget id and
//! an abuse-prevention key + token, valid about an hour). They are fetched
//! once, kept, and fetched again when they expire or Bing stops accepting
//! them. (Edge's `translate/auth` token endpoint no longer exists.)

use std::sync::Arc;
use std::time::{Duration, Instant};

use futures_util::FutureExt;
use futures_util::future::BoxFuture;
use hl_core::lang::{self, AUTO};
use hl_core::{Engine, EngineError, EngineId, Job, Output, Sink};
use reqwest::Client;
use serde::Deserialize;
use tokio::sync::Mutex;

use crate::http;

const AZURE_URL: &str = "https://api.cognitive.microsofttranslator.com/translate";
const PAGE_URL: &str = "https://www.bing.com/translator";
const WEB_URL: &str = "https://www.bing.com/ttranslatev3";
const AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0 Safari/537.36 Edg/130.0";

/// The page session the web endpoint wants.
#[derive(Clone, Debug, PartialEq)]
struct Session {
    ig: String,
    iid: String,
    key: String,
    token: String,
    until: Instant,
}

pub struct Bing {
    http: Client,
    /// Azure key and region.
    azure: Option<(String, String)>,
    session: Mutex<Option<Session>>,
}

impl Bing {
    pub fn new(http: Client, key: Option<String>, region: &str) -> Self {
        let azure = key.filter(|k| !k.trim().is_empty()).map(|k| (k, region.trim().to_owned()));
        Self { http, azure, session: Mutex::new(None) }
    }

    async fn azure(&self, key: &str, region: &str, job: &Job) -> Result<Output, EngineError> {
        let mut query = vec![("api-version", "3.0".to_owned()), ("to", code(&job.target))];
        if job.source != AUTO {
            query.push(("from", code(&job.source)));
        }
        let mut req = self
            .http
            .post(AZURE_URL)
            .query(&query)
            .header("Ocp-Apim-Subscription-Key", key)
            .json(&serde_json::json!([{ "Text": job.text }]));
        if !region.is_empty() {
            req = req.header("Ocp-Apim-Subscription-Region", region);
        }
        let resp = req.send().await.map_err(http::network)?;
        let status = resp.status();
        let body = resp.text().await.map_err(http::network)?;
        if !status.is_success() {
            return Err(http::status(status, true));
        }
        parse(&body)
    }

    async fn keyless(&self, job: &Job) -> Result<Output, EngineError> {
        let mut renewed = false;
        loop {
            let s = self.current(renewed).await?;
            let resp = self
                .http
                .post(WEB_URL)
                .header("User-Agent", AGENT)
                .query(&[("isVertical", "1"), ("IG", s.ig.as_str()), ("IID", s.iid.as_str())])
                .form(&[
                    ("fromLang", if job.source == AUTO { "auto-detect".to_owned() } else { code(&job.source) }.as_str()),
                    ("to", code(&job.target).as_str()),
                    ("text", job.text.as_str()),
                    ("token", s.token.as_str()),
                    ("key", s.key.as_str()),
                ])
                .send()
                .await
                .map_err(http::network)?;
            let status = resp.status();
            let body = resp.text().await.map_err(http::network)?;
            // A stale session: an error object, an empty body or a redirect.
            let stale = status.is_redirection() || body.trim().is_empty() || body.contains("\"statusCode\"");
            if stale && !renewed {
                renewed = true;
                continue;
            }
            if status.as_u16() == 429 || stale {
                return Err(EngineError::RateLimited);
            }
            if !status.is_success() {
                return Err(http::status(status, false));
            }
            return parse(&body);
        }
    }

    /// The kept session, or a fresh one when it expired or `renew` is set.
    async fn current(&self, renew: bool) -> Result<Session, EngineError> {
        let mut s = self.session.lock().await;
        if !renew && let Some(cur) = s.as_ref().filter(|c| c.until > Instant::now()) {
            return Ok(cur.clone());
        }
        let resp = self.http.get(PAGE_URL).header("User-Agent", AGENT).send().await.map_err(http::network)?;
        if !resp.status().is_success() {
            return Err(http::status(resp.status(), false));
        }
        let page = resp.text().await.map_err(http::network)?;
        let fresh = parse_page(&page, Instant::now()).ok_or_else(|| EngineError::Server("no session on the page".into()))?;
        *s = Some(fresh.clone());
        Ok(fresh)
    }
}

impl Engine for Bing {
    fn id(&self) -> EngineId {
        EngineId::new("bing")
    }

    fn translate(self: Arc<Self>, job: Arc<Job>, _sink: Sink) -> BoxFuture<'static, Result<Output, EngineError>> {
        async move {
            match &self.azure {
                Some((key, region)) => self.azure(key, region, &job).await,
                None => self.keyless(&job).await,
            }
        }
        .boxed()
    }
}

/// Our code as Microsoft spells it.
fn code(c: &str) -> String {
    match c {
        "zh-CN" => "zh-Hans".into(),
        "no" => "nb".into(),
        c => c.to_owned(),
    }
}

/// `IG:"…"`, `data-iid="translator.5023"`, and
/// `params_AbusePreventionHelper = [key,"token",ttl_ms]`.
fn parse_page(page: &str, now: Instant) -> Option<Session> {
    let between = |start: &str, end: char| {
        let i = page.find(start)? + start.len();
        let rest = &page[i..];
        Some(rest[..rest.find(end)?].to_owned())
    };
    let ig = between("IG:\"", '"')?;
    let iid = between("data-iid=\"", '"').unwrap_or_else(|| "translator.5023".into());
    let params = between("params_AbusePreventionHelper = [", ']')?;
    let mut parts = params.splitn(3, ',');
    let key = parts.next()?.trim().to_owned();
    let token = parts.next()?.trim().trim_matches('"').to_owned();
    let ttl_ms: u64 = parts.next().and_then(|t| t.trim().parse().ok()).unwrap_or(3_600_000);
    if ig.is_empty() || key.is_empty() || token.is_empty() {
        return None;
    }
    // Renew a little early.
    let ttl = Duration::from_millis(ttl_ms).saturating_sub(Duration::from_secs(120));
    Some(Session { ig, iid: format!("{iid}.1"), key, token, until: now + ttl })
}

/// `[{"detectedLanguage":{"language":"en"},"translations":[{"text":"…"}]}]`
/// (the same shape from Azure and the web endpoint).
fn parse(body: &str) -> Result<Output, EngineError> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Item {
        detected_language: Option<Detected>,
        translations: Vec<Text>,
    }
    #[derive(Deserialize)]
    struct Detected {
        language: String,
    }
    #[derive(Deserialize)]
    struct Text {
        text: String,
    }
    let unexpected = || EngineError::Server("unexpected answer".into());
    let items: Vec<Item> = serde_json::from_str(body).map_err(|_| unexpected())?;
    let item = items.into_iter().next().ok_or_else(unexpected)?;
    let text = item.translations.into_iter().next().ok_or_else(unexpected)?.text;
    Ok(Output { text, source_lang: item.detected_language.map(|d| lang::normalize(&d.language)), ..Output::default() })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_session() {
        let page = r#"<div data-iid="translator.5023">…IG:"2A69F9",…var params_AbusePreventionHelper = [1791993967793,"Vdsa0y_w",3600000];"#;
        let now = Instant::now();
        let s = parse_page(page, now).unwrap();
        assert_eq!((s.ig.as_str(), s.iid.as_str(), s.key.as_str(), s.token.as_str()), ("2A69F9", "translator.5023.1", "1791993967793", "Vdsa0y_w"));
        assert!(s.until > now + Duration::from_secs(3000));
        assert!(parse_page("<html></html>", now).is_none());
    }

    #[test]
    fn answers() {
        let body = r#"[{"translations":[{"text":"Встреча перенесена на четверг.","to":"ru"}],"usedLLM":true,"detectedLanguage":{"language":"en"}}]"#;
        let o = parse(body).unwrap();
        assert_eq!(o.text, "Встреча перенесена на четверг.");
        assert_eq!(o.source_lang.as_deref(), Some("en"));
        assert_eq!(parse(r#"[{"detectedLanguage":{"language":"zh-Hans"},"translations":[{"text":"Hi"}]}]"#).unwrap().source_lang.as_deref(), Some("zh-CN"));
        assert!(parse(r#"{"statusCode":400}"#).is_err() && parse("[]").is_err());
        assert_eq!(code("zh-CN"), "zh-Hans");
    }

    /// `cargo test -p hl-providers -- --ignored bing`
    #[tokio::test]
    #[ignore]
    async fn live_bing() {
        let b = Arc::new(Bing::new(crate::client(), None, ""));
        let job = Arc::new(Job { text: "The meeting has been moved to Thursday.".into(), source: AUTO.into(), target: "ru".into(), ..Job::default() });
        let o = b.clone().translate(job.clone(), Sink::discard()).await.unwrap();
        println!("{o:?}");
        assert!(o.text.to_lowercase().contains("четверг"));
        // Second call reuses the session.
        assert!(b.translate(job, Sink::discard()).await.is_ok());
    }
}
