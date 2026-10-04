//! A small language guess for "auto" with the offline tiers (they don't
//! detect languages themselves): the script decides, and for Cyrillic and
//! Latin text a few telltale letters and common words narrow it down.
//! Good enough to pick a model; Latin text with no clue counts as English.

/// Language code (`settings::LANGUAGES` style) for `text`.
pub fn guess(text: &str) -> String {
    let mut counts = [0usize; 12];
    const CYR: usize = 0;
    const LAT: usize = 1;
    const HAN: usize = 2;
    const KANA: usize = 3;
    const HANGUL: usize = 4;
    const ARABIC: usize = 5;
    const GREEK: usize = 6;
    const HEBREW: usize = 7;
    const THAI: usize = 8;
    const DEVA: usize = 9;
    const GEO: usize = 10;
    const ARM: usize = 11;
    for c in text.chars() {
        let i = match c as u32 {
            0x0400..=0x04FF => CYR,
            0x0041..=0x005A | 0x0061..=0x007A | 0x00C0..=0x024F => LAT,
            0x3040..=0x30FF => KANA,
            0x4E00..=0x9FFF | 0x3400..=0x4DBF => HAN,
            0xAC00..=0xD7AF | 0x1100..=0x11FF => HANGUL,
            0x0600..=0x06FF | 0x0750..=0x077F => ARABIC,
            0x0370..=0x03FF => GREEK,
            0x0590..=0x05FF => HEBREW,
            0x0E00..=0x0E7F => THAI,
            0x0900..=0x097F => DEVA,
            0x10A0..=0x10FF => GEO,
            0x0530..=0x058F => ARM,
            _ => continue,
        };
        counts[i] += 1;
    }
    // Any kana means Japanese even when kanji dominate.
    if counts[KANA] > 0 {
        return "ja".into();
    }
    let best = (0..counts.len()).max_by_key(|&i| counts[i]).unwrap_or(LAT);
    if counts[best] == 0 {
        return "en".into();
    }
    match best {
        CYR => cyrillic(text),
        HAN => "zh-CN".into(),
        HANGUL => "ko".into(),
        ARABIC => if text.chars().any(|c| matches!(c, 'پ' | 'چ' | 'ژ' | 'گ' | 'ی')) { "fa" } else { "ar" }.into(),
        GREEK => "el".into(),
        HEBREW => "he".into(),
        THAI => "th".into(),
        DEVA => "hi".into(),
        GEO => "ka".into(),
        ARM => "hy".into(),
        _ => latin(text),
    }
}

fn cyrillic(text: &str) -> String {
    let has = |set: &[char]| text.chars().any(|c| set.contains(&c.to_lowercase().next().unwrap_or(c)));
    if has(&['ә', 'ғ', 'қ', 'ң', 'ө', 'ұ', 'ү', 'һ']) {
        "kk"
    } else if has(&['ў']) {
        "be"
    } else if has(&['ї', 'є', 'ґ']) || (has(&['і']) && !has(&['ы', 'э', 'ё'])) {
        "uk"
    } else if has(&['ы', 'э', 'ё']) {
        "ru"
    } else if text.split_whitespace().any(|w| matches!(w.to_lowercase().as_str(), "съм" | "със" | "във" | "това" | "които")) {
        "bg"
    } else {
        "ru"
    }
    .into()
}

/// Latin-script languages: whatlang's trigram model, limited to the
/// languages the app offers. Very short text (a word or two) is too little
/// for trigrams; the telltale letters and stop words decide there.
fn latin(text: &str) -> String {
    use whatlang::{Detector, Lang};
    const LANGS: [(Lang, &str); 22] = [
        (Lang::Eng, "en"),
        (Lang::Deu, "de"),
        (Lang::Fra, "fr"),
        (Lang::Spa, "es"),
        (Lang::Ita, "it"),
        (Lang::Por, "pt"),
        (Lang::Nld, "nl"),
        (Lang::Pol, "pl"),
        (Lang::Ces, "cs"),
        (Lang::Slk, "sk"),
        (Lang::Swe, "sv"),
        (Lang::Dan, "da"),
        (Lang::Nob, "no"),
        (Lang::Fin, "fi"),
        (Lang::Est, "et"),
        (Lang::Lav, "lv"),
        (Lang::Lit, "lt"),
        (Lang::Hun, "hu"),
        (Lang::Ron, "ro"),
        (Lang::Tur, "tr"),
        (Lang::Vie, "vi"),
        (Lang::Ind, "id"),
    ];
    let (heuristic, score) = latin_heuristic(text);
    let letters = text.chars().filter(|c| c.is_alphabetic()).count();
    if letters >= 12 {
        let detector = Detector::with_allowlist(LANGS.iter().map(|(l, _)| *l).collect());
        if let Some(info) = detector.detect(text) {
            let code = LANGS.iter().find(|(l, _)| *l == info.lang()).map_or("en", |(_, c)| c);
            // Trust the model unless the clues are strong (two stop words or a telltale letter).
            if info.is_reliable() || score < 4 || info.confidence() > 0.5 {
                return code.into();
            }
        }
    }
    heuristic.into()
}

/// Telltale letters and stop words: (language, score).
fn latin_heuristic(text: &str) -> (&'static str, usize) {
    let lower = text.to_lowercase();
    let words: Vec<&str> = lower
        .split(|c: char| !c.is_alphabetic() && c != '\'')
        .filter(|w| !w.is_empty())
        .collect();
    let langs: [(&str, &[&str], &[char]); 12] = [
        ("en", &["the", "and", "is", "are", "of", "to", "you", "this", "that", "with", "it", "for", "was", "be", "have"], &[]),
        ("de", &["der", "die", "das", "und", "ist", "nicht", "ich", "mit", "sie", "ein", "eine", "zu", "auf", "wir"], &['ß', 'ä', 'ö', 'ü']),
        ("fr", &["le", "la", "les", "et", "est", "une", "des", "je", "pas", "vous", "nous", "que", "dans", "pour"], &['ç', 'è', 'ê', 'à', 'œ']),
        ("es", &["el", "los", "las", "y", "es", "una", "que", "por", "para", "con", "muy", "pero", "está", "gracias"], &['ñ', '¿', '¡']),
        ("it", &["il", "gli", "e", "è", "di", "che", "non", "per", "una", "sono", "della", "con", "grazie"], &['ò', 'ì']),
        ("pt", &["o", "os", "as", "e", "é", "não", "um", "uma", "que", "com", "obrigado", "você", "para"], &['ã', 'õ']),
        ("nl", &["de", "het", "een", "en", "is", "niet", "ik", "dat", "van", "zijn", "met", "voor"], &['ĳ']),
        ("pl", &["i", "jest", "nie", "się", "to", "na", "że", "w", "z", "do", "jak", "czy"], &['ą', 'ę', 'ł', 'ś', 'ź', 'ż', 'ć', 'ń']),
        ("cs", &["je", "a", "se", "na", "že", "to", "jsem", "není", "jak", "ale"], &['ř', 'ů', 'ě']),
        ("tr", &["ve", "bir", "bu", "için", "değil", "ben", "çok", "ne"], &['ğ', 'ş', 'ı']),
        ("sv", &["och", "är", "att", "det", "som", "inte", "jag", "en", "på"], &['å']),
        ("vi", &["và", "là", "của", "không", "có", "tôi", "một"], &['ư', 'ơ', 'đ', 'ạ', 'ả', 'ế', 'ộ']),
    ];
    let mut best = ("en", 0usize);
    for (code, stop, letters) in langs {
        let score = words.iter().filter(|w| stop.contains(w)).count() * 2
            + lower.chars().filter(|c| letters.contains(c)).count() * 3;
        if score > best.1 {
            best = (code, score);
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scripts_and_common_languages() {
        assert_eq!(guess("Hello, how are you?"), "en");
        assert_eq!(guess("Привет, как дела?"), "ru");
        assert_eq!(guess("Дякую, все добре, ї"), "uk");
        assert_eq!(guess("Die Besprechung wurde auf Montag verschoben."), "de");
        assert_eq!(guess("Gracias por tu ayuda."), "es");
        assert_eq!(guess("Merci pour votre aide, c'est très gentil."), "fr");
        assert_eq!(guess("你好世界"), "zh-CN");
        assert_eq!(guess("こんにちは世界"), "ja");
        assert_eq!(guess("안녕하세요"), "ko");
        assert_eq!(guess("12345"), "en");
        assert_eq!(guess("effectively"), "en");
        // No stop words from the old list: the trigram model decides.
        assert_eq!(guess("Guten Morgen, wie geht es dir?"), "de");
        assert_eq!(guess("Buongiorno, come stai oggi?"), "it");
        assert_eq!(guess("Dzień dobry, jak się masz dzisiaj?"), "pl");
        assert_eq!(guess("The new process cut review time in half."), "en");
    }
}
