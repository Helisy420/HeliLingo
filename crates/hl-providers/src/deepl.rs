//! DeepL API v2 (key required). Free keys end in ":fx" and use the Free host.
//! Earlier fragments go in DeepL's `context` parameter: it steers the
//! translation and isn't translated or billed.

use std::sync::Arc;

use futures_util::FutureExt;
use futures_util::future::BoxFuture;
use hl_core::lang::{self, AUTO};
use hl_core::{Engine, EngineError, EngineId, Job, Output, Sink};
use reqwest::Client;

use crate::http;

pub struct DeepL {
    http: Client,
    key: Option<String>,
}

impl DeepL {
    pub fn new(http: Client, key: Option<String>) -> Self {
        Self { http, key: key.filter(|k| !k.trim().is_empty()), }
    }

    async fn run(&self, key: &str, job: &Job) -> Result<Output, EngineError> {
        let mut body = serde_json::json!({ "text": [job.text], "target_lang": target_code(&job.target) });
        if job.source != AUTO {
            body["source_lang"] = source_code(&job.source).into();
        }
        let context: Vec<&str> = job.context.iter().map(|p| p.source.trim()).collect();
        if !context.is_empty() {
            body["context"] = context.join("\n").into();
        }
        let resp = self
            .http
            .post(format!("{}v2/translate", base_url(key)))
            .header("Authorization", format!("DeepL-Auth-Key {key}"))
            .json(&body)
            .send()
            .await
            .map_err(http::network)?;
        let status = resp.status();
        let text = resp.text().await.map_err(http::network)?;
        if !status.is_success() {
            // 400 with "target_lang" / "source_lang": a language DeepL lacks.
            if status.as_u16() == 400 && text.contains("_lang") {
                return Err(EngineError::UnsupportedPair);
            }
            return Err(http::status(status, true));
        }
        parse(&text)
    }
}

impl Engine for DeepL {
    fn id(&self) -> EngineId {
        EngineId::new("deepl")
    }

    fn translate(self: Arc<Self>, job: Arc<Job>, _sink: Sink) -> BoxFuture<'static, Result<Output, EngineError>> {
        async move {
            let Some(key) = self.key.clone() else { return Err(EngineError::NoKey) };
            self.run(&key, &job).await
        }
        .boxed()
    }
}

/// API host for a key: Free or Pro.
pub fn base_url(key: &str) -> &'static str {
    if key.trim().ends_with(":fx") { "https://api-free.deepl.com/" } else { "https://api.deepl.com/" }
}

fn source_code(c: &str) -> String {
    match c {
        "zh-CN" => "ZH".into(),
        "no" => "NB".into(),
        c => c.split('-').next().unwrap_or(c).to_ascii_uppercase(),
    }
}

/// DeepL wants a variant for some targets.
fn target_code(c: &str) -> String {
    match c {
        "en" => "EN-US".into(),
        "pt" => "PT-BR".into(),
        "zh-CN" => "ZH-HANS".into(),
        "no" => "NB".into(),
        c => c.split('-').next().unwrap_or(c).to_ascii_uppercase(),
    }
}

fn parse(body: &str) -> Result<Output, EngineError> {
    let unexpected = || EngineError::Server("unexpected answer".into());
    let v: serde_json::Value = serde_json::from_str(body).map_err(|_| unexpected())?;
    let t = &v["translations"][0];
    let text = t["text"].as_str().ok_or_else(unexpected)?.to_owned();
    let source_lang = t["detected_source_language"].as_str().map(lang::normalize);
    Ok(Output { text, source_lang, ..Output::default() })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn answers_and_codes() {
        let o = parse(r#"{"translations":[{"detected_source_language":"EN","text":"Hallo"}]}"#).unwrap();
        assert_eq!((o.text.as_str(), o.source_lang.as_deref()), ("Hallo", Some("en")));
        assert_eq!(parse(r#"{"translations":[{"detected_source_language":"ZH","text":"Hi"}]}"#).unwrap().source_lang.as_deref(), Some("zh-CN"));
        assert!(parse(r#"{"translations":[]}"#).is_err());
        assert_eq!(base_url("abc:fx"), "https://api-free.deepl.com/");
        assert_eq!(base_url("abc"), "https://api.deepl.com/");
        assert_eq!((target_code("en"), target_code("zh-CN"), source_code("zh-CN")), ("EN-US".into(), "ZH-HANS".into(), "ZH".into()));
    }
}
