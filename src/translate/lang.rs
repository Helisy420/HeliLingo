//! Language codes per provider. Our codes are the ones in
//! [`settings::LANGUAGES`] (Google's); each provider maps them on the way
//! out, and detected codes are mapped back with [`ours`].

use crate::settings::{LANGUAGES, ProviderKind};

/// Our code as `kind` spells it. `auto` passes through; callers omit the
/// source language for it.
pub fn to_provider(kind: ProviderKind, code: &str) -> String {
    match (kind, code) {
        (_, "auto") => code.to_owned(),
        (ProviderKind::Yandex, "zh-CN") => "zh".into(),
        (ProviderKind::Bing, "zh-CN") => "zh-Hans".into(),
        (ProviderKind::Bing, "no") => "nb".into(),
        _ => code.to_owned(),
    }
}

/// DeepL target language: a few need a variant ("EN-US", "ZH-HANS").
pub fn deepl_target(code: &str) -> String {
    match code {
        "en" => "EN-US".into(),
        "pt" => "PT-PT".into(),
        "zh-CN" | "zh" => "ZH-HANS".into(),
        "no" => "NB".into(),
        c => c.to_uppercase(),
    }
}

/// DeepL source language: base codes only ("ZH", not "ZH-HANS").
pub fn deepl_source(code: &str) -> String {
    match code {
        "no" => "NB".into(),
        c => c.split('-').next().unwrap_or(c).to_uppercase(),
    }
}

/// A detected code from any provider as our code: "zh-Hans" → "zh-CN",
/// "nb" → "no", "EN" → "en", "pt-BR" → "pt". Unknown codes come back
/// lowercased.
pub fn ours(code: &str) -> String {
    let lower = code.trim().to_ascii_lowercase();
    match lower.as_str() {
        "zh" | "zh-cn" | "zh-hans" | "zh-sg" => return "zh-CN".into(),
        "nb" | "nn" => return "no".into(),
        "iw" => return "he".into(),
        "in" => return "id".into(),
        _ => {}
    }
    if let Some((c, _, _)) = LANGUAGES.iter().find(|(c, _, _)| c.eq_ignore_ascii_case(&lower)) {
        return (*c).to_owned();
    }
    let base = lower.split('-').next().unwrap_or(&lower);
    match LANGUAGES.iter().find(|(c, _, _)| *c == base) {
        Some((c, _, _)) => (*c).to_owned(),
        None => lower,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outgoing_codes() {
        assert_eq!(to_provider(ProviderKind::Yandex, "zh-CN"), "zh");
        assert_eq!(to_provider(ProviderKind::Bing, "zh-CN"), "zh-Hans");
        assert_eq!(to_provider(ProviderKind::Bing, "no"), "nb");
        assert_eq!(to_provider(ProviderKind::Google, "zh-CN"), "zh-CN");
        assert_eq!(to_provider(ProviderKind::Bing, "auto"), "auto");
        assert_eq!(to_provider(ProviderKind::Yandex, "ru"), "ru");
        assert_eq!(deepl_target("en"), "EN-US");
        assert_eq!(deepl_target("zh-CN"), "ZH-HANS");
        assert_eq!(deepl_target("ru"), "RU");
        assert_eq!(deepl_source("zh-CN"), "ZH");
        assert_eq!(deepl_source("no"), "NB");
    }

    #[test]
    fn detected_codes() {
        assert_eq!(ours("EN"), "en");
        assert_eq!(ours("zh-Hans"), "zh-CN");
        assert_eq!(ours("zh"), "zh-CN");
        assert_eq!(ours("zh-CN"), "zh-CN");
        assert_eq!(ours("nb"), "no");
        assert_eq!(ours("iw"), "he");
        assert_eq!(ours("pt-BR"), "pt");
        assert_eq!(ours("en-US"), "en");
        assert_eq!(ours("tlh-Latn"), "tlh-latn");
    }
}
