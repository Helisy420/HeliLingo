//! Language codes. Ours are Google's ("en", "zh-CN"); providers map them on
//! the way out and map detected codes back with [`normalize`].

/// The source code meaning "detect it".
pub const AUTO: &str = "auto";

/// (code, English name, Russian name), in picker order.
pub const LANGUAGES: &[(&str, &str, &str)] = &[
    ("en", "English", "Английский"),
    ("ru", "Russian", "Русский"),
    ("uk", "Ukrainian", "Украинский"),
    ("de", "German", "Немецкий"),
    ("fr", "French", "Французский"),
    ("es", "Spanish", "Испанский"),
    ("it", "Italian", "Итальянский"),
    ("pt", "Portuguese", "Португальский"),
    ("nl", "Dutch", "Нидерландский"),
    ("pl", "Polish", "Польский"),
    ("cs", "Czech", "Чешский"),
    ("sk", "Slovak", "Словацкий"),
    ("sv", "Swedish", "Шведский"),
    ("da", "Danish", "Датский"),
    ("no", "Norwegian", "Норвежский"),
    ("fi", "Finnish", "Финский"),
    ("et", "Estonian", "Эстонский"),
    ("lv", "Latvian", "Латышский"),
    ("lt", "Lithuanian", "Литовский"),
    ("hu", "Hungarian", "Венгерский"),
    ("ro", "Romanian", "Румынский"),
    ("bg", "Bulgarian", "Болгарский"),
    ("el", "Greek", "Греческий"),
    ("tr", "Turkish", "Турецкий"),
    ("be", "Belarusian", "Белорусский"),
    ("kk", "Kazakh", "Казахский"),
    ("ka", "Georgian", "Грузинский"),
    ("hy", "Armenian", "Армянский"),
    ("he", "Hebrew", "Иврит"),
    ("ar", "Arabic", "Арабский"),
    ("fa", "Persian", "Персидский"),
    ("hi", "Hindi", "Хинди"),
    ("th", "Thai", "Тайский"),
    ("vi", "Vietnamese", "Вьетнамский"),
    ("id", "Indonesian", "Индонезийский"),
    ("ja", "Japanese", "Японский"),
    ("ko", "Korean", "Корейский"),
    ("zh-CN", "Chinese", "Китайский"),
];

/// Any provider's spelling of a language as our code: "zh-Hans" → "zh-CN",
/// "nb" → "no", "EN" → "en", "pt-BR" → "pt". Unknown codes come back
/// lowercased.
pub fn normalize(code: &str) -> String {
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

/// Picker search: the code's start, or part of the English or Russian
/// name ("ger", "нем", "de" all find German). Empty matches everything.
pub fn matches(code: &str, query: &str) -> bool {
    let q = query.trim().to_lowercase();
    if q.is_empty() || code.to_lowercase().starts_with(&q) {
        return true;
    }
    LANGUAGES
        .iter()
        .find(|(c, _, _)| *c == code)
        .is_some_and(|(_, en, ru)| en.to_lowercase().contains(&q) || ru.to_lowercase().contains(&q))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_provider_codes() {
        assert_eq!(normalize("EN"), "en");
        assert_eq!(normalize("zh-Hans"), "zh-CN");
        assert_eq!(normalize("nb"), "no");
        assert_eq!(normalize("iw"), "he");
        assert_eq!(normalize("pt-BR"), "pt");
        assert_eq!(normalize("tlh-Latn"), "tlh-latn");
    }

    #[test]
    fn searches_names() {
        assert!(matches("de", "GER"));
        assert!(matches("de", "нем"));
        assert!(matches("zh-CN", "zh"));
        assert!(!matches("de", "fr"));
    }
}
