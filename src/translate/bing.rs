//! Microsoft Translator v3: keyless with the Edge browser's token, or an
//! Azure Translator key (+ region).

use std::time::{Duration, Instant};

use reqwest::StatusCode;
use serde::Deserialize;

use super::{Engine, Error, SERVICE_ERROR, Translation, UNEXPECTED, lang, parse, send, status_error};
use crate::settings::{ProviderKeys, ProviderKind};

pub const URL: &str = "https://api.cognitive.microsofttranslator.com/translate";
pub const AUTH_URL: &str = "https://edge.microsoft.com/translate/auth";
/// Edge tokens live 10 minutes; renew a bit early.
const TOKEN_TTL: Duration = Duration::from_secs(8 * 60);
/// How long a failed token fetch is reused before trying again.
const FAILURE_TTL: Duration = Duration::from_secs(2 * 60);

impl Engine {
    pub(super) fn bing(&self, keys: &ProviderKeys, text: &str, from: &str, to: &str) -> Result<Translation, Error> {
        let mut query = vec![("api-version", "3.0".to_owned()), ("to", lang::to_provider(ProviderKind::Bing, to))];
        if from != "auto" {
            query.push(("from", lang::to_provider(ProviderKind::Bing, from)));
        }
        let body = serde_json::json!([{ "Text": text }]);
        let request = |token: Option<&str>| {
            let mut req = self.http.post(URL).query(&query).json(&body);
            match token {
                Some(token) => req = req.bearer_auth(token),
                None => {
                    req = req.header("Ocp-Apim-Subscription-Key", keys.azure.expose());
                    let region = keys.azure_region.trim();
                    if !region.is_empty() {
                        req = req.header("Ocp-Apim-Subscription-Region", region);
                    }
                }
            }
            send(req)
        };

        let azure = keys.azure.is_set();
        let (status, resp) = if azure {
            request(None)?
        } else {
            let sent = request(Some(&self.edge_token(false)?))?;
            // The cached token may have been revoked early: renew once.
            if sent.0 == StatusCode::UNAUTHORIZED { request(Some(&self.edge_token(true)?))? } else { sent }
        };
        if !status.is_success() {
            return Err(status_error(status, azure));
        }
        parse_response(&resp, text, to)
    }

    /// The cached Edge token, or a fresh one when it's stale or `renew` is
    /// set. A failed fetch is remembered for a while too, so a dead token
    /// endpoint doesn't cost a round trip on every translation.
    pub(super) fn edge_token(&self, renew: bool) -> Result<String, Error> {
        if !renew && let Some((token, at)) = self.bing_token.lock().unwrap().as_ref() {
            let ttl = if token.is_ok() { TOKEN_TTL } else { FAILURE_TTL };
            if at.elapsed() < ttl {
                return token.clone();
            }
        }
        let token = self.fetch_edge_token();
        // Offline isn't the endpoint's fault: don't remember it.
        if !matches!(token, Err(Error::Offline)) {
            *self.bing_token.lock().unwrap() = Some((token.clone(), Instant::now()));
        }
        token
    }

    fn fetch_edge_token(&self) -> Result<String, Error> {
        let (status, body) = send(self.http.get(AUTH_URL))?;
        if !status.is_success() {
            // A 404 here means the endpoint is gone, not a bad request.
            return Err(match status_error(status, false) {
                Error::Other(_) => Error::Other(SERVICE_ERROR.into()),
                e => e,
            });
        }
        let token = body.trim().to_owned();
        // A JWT: three base64 parts separated by dots.
        if token.split('.').count() != 3 || token.contains(char::is_whitespace) {
            return Err(Error::Other(UNEXPECTED.into()));
        }
        Ok(token)
    }

    /// Forgets the Edge token (and a remembered failure): Check retries for real.
    pub(super) fn reset_edge_token(&self) {
        *self.bing_token.lock().unwrap() = None;
    }
}

fn parse_response(body: &str, text: &str, to: &str) -> Result<Translation, Error> {
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

    let items: Vec<Item> = parse(body)?;
    let item = items.into_iter().next().ok_or_else(super::empty)?;
    let src_lang = item.detected_language.map(|d| lang::ours(&d.language));
    let out = item.translations.into_iter().next().ok_or_else(super::empty)?;
    Ok(Translation {
        source: text.to_owned(),
        text: out.text,
        src_lang,
        tgt_lang: to.to_owned(),
        is_word: false,
        dict: Vec::new(),
        provider: ProviderKind::Bing,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn response() {
        let body = r#"[{"detectedLanguage":{"language":"zh-Hans","score":1.0},
            "translations":[{"text":"Hello","to":"en"}]}]"#;
        let t = parse_response(body, "你好", "en").unwrap();
        assert_eq!(t.text, "Hello");
        assert_eq!(t.src_lang.as_deref(), Some("zh-CN"));
        assert_eq!(t.provider, ProviderKind::Bing);

        let body = r#"[{"translations":[{"text":"Hallo","to":"de"}]}]"#;
        assert_eq!(parse_response(body, "Hello", "de").unwrap().src_lang, None);
        assert!(parse_response(r#"{"error":{"code":401000,"message":"x"}}"#, "Hello", "de").is_err());
        assert!(parse_response("[]", "Hello", "de").is_err());
    }
}
