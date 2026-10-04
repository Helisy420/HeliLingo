//! DeepL API v2. Free keys (ending in ":fx") use api-free.deepl.com.

use serde::Deserialize;

use super::{Engine, Error, Translation, lang, parse, send, status_error};
use crate::settings::ProviderKind;

/// API host for a key: Free or Pro.
pub fn base_url(key: &str) -> &'static str {
    if key.ends_with(":fx") { "https://api-free.deepl.com/" } else { "https://api.deepl.com/" }
}

impl Engine {
    /// `context`: earlier text around the selection (DeepL's `context`
    /// parameter: it guides the translation and isn't translated or billed).
    pub(super) fn deepl(&self, key: &str, text: &str, from: &str, to: &str, context: Option<&str>) -> Result<Translation, Error> {
        let mut json = serde_json::json!({
            "text": [text],
            "target_lang": lang::deepl_target(to),
        });
        if from != "auto" {
            json["source_lang"] = lang::deepl_source(from).into();
        }
        if let Some(c) = context {
            json["context"] = c.into();
        }
        let req = self
            .http
            .post(format!("{}v2/translate", base_url(key)))
            .header("Authorization", format!("DeepL-Auth-Key {key}"))
            .json(&json);
        let (status, body) = send(req)?;
        if !status.is_success() {
            return Err(status_error(status, true));
        }
        parse_response(&body, text, to)
    }
}

fn parse_response(body: &str, text: &str, to: &str) -> Result<Translation, Error> {
    #[derive(Deserialize)]
    struct Resp {
        translations: Vec<Item>,
    }
    #[derive(Deserialize)]
    struct Item {
        detected_source_language: Option<String>,
        text: String,
    }

    let r: Resp = parse(body)?;
    let item = r.translations.into_iter().next().ok_or_else(super::empty)?;
    Ok(Translation {
        source: text.to_owned(),
        text: item.text,
        src_lang: item.detected_source_language.as_deref().map(lang::ours),
        tgt_lang: to.to_owned(),
        is_word: false,
        dict: Vec::new(),
        provider: ProviderKind::DeepL,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn response() {
        let body = r#"{"translations":[{"detected_source_language":"EN","text":"Hallo"}]}"#;
        let t = parse_response(body, "Hello", "de").unwrap();
        assert_eq!(t.text, "Hallo");
        assert_eq!(t.src_lang.as_deref(), Some("en"));
        assert_eq!(t.provider, ProviderKind::DeepL);
        let body = r#"{"translations":[{"detected_source_language":"ZH","text":"Hi"}]}"#;
        assert_eq!(parse_response(body, "你好", "en").unwrap().src_lang.as_deref(), Some("zh-CN"));
        assert!(parse_response(r#"{"translations":[]}"#, "Hello", "de").is_err());
    }

    #[test]
    fn free_or_pro() {
        assert_eq!(base_url("abc:fx"), "https://api-free.deepl.com/");
        assert_eq!(base_url("abc"), "https://api.deepl.com/");
    }
}
