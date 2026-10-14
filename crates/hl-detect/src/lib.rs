//! Language detection for "auto": which language is this text in?
//!
//! lingua over the ten most used languages (each model is ~5 MB in the
//! exe); everything with its own script is recognised from the script. The detector is built once, on a background
//! thread at start ([`warm_up`]). The script alone (Greek, Hebrew, kana,
//! Hangul…) answers without lingua; Latin, Cyrillic, Arabic and Han text
//! needs it. If the warm-up hasn't finished, [`detect`] builds the detector
//! itself (blocking: call it off async threads, see [`ready`]).

use std::sync::OnceLock;

use lingua::Language::{self, *};
use lingua::{LanguageDetector, LanguageDetectorBuilder};

/// Our code (as in `hl_core::lang`) for each lingua language. Only the
/// ten most used: each model adds ~5 MB to the exe (user's choice,
/// 2026-10-14). Other Latin-script languages get a nearest guess — callers
/// use detection only to choose the target language, never as the source
/// they send to an engine.
const LANGS: &[(Language, &str)] = &[
    (English, "en"),
    (Russian, "ru"),
    (Ukrainian, "uk"),
    (German, "de"),
    (French, "fr"),
    (Spanish, "es"),
    (Italian, "it"),
    (Portuguese, "pt"),
    (Polish, "pl"),
    (Turkish, "tr"),
];

static DETECTOR: OnceLock<LanguageDetector> = OnceLock::new();

fn build() -> LanguageDetector {
    let langs: Vec<Language> = LANGS.iter().map(|(l, _)| *l).collect();
    // No minimum distance: one word still gets a best guess. A wrong guess only
    // matters when it equals the target language, and the script check below
    // catches the common case (Latin text guessed as Russian and back).
    LanguageDetectorBuilder::from_languages(&langs).build()
}

/// Builds the detector on a background thread (call once at start).
pub fn warm_up() {
    std::thread::Builder::new()
        .name("lang-detect".into())
        .spawn(|| {
            let d = DETECTOR.get_or_init(build);
            // Touch the models of the two most common languages.
            let _ = d.detect_language_of("warm up the models");
            let _ = d.detect_language_of("прогрев");
        })
        .expect("spawn detector thread");
}

/// Our code for the language of `text`, or `None` when unsure (too short,
/// digits only, mixed). Fast: script check first, lingua only when needed.
pub fn detect(text: &str) -> Option<&'static str> {
    let sample = sample(text);
    let script = script_of(&sample)?;
    match script {
        Script::Only(code) => return Some(code),
        // Persian has letters Arabic doesn't (پ چ ژ گ ی ک).
        Script::Arabic => {
            let persian = sample.chars().any(|c| matches!(c, 'پ' | 'چ' | 'ژ' | 'گ' | 'ی' | 'ک'));
            return Some(if persian { "fa" } else { "ar" });
        }
        // Han without kana: Chinese.
        Script::Han => return Some("zh-CN"),
        _ => {}
    }
    // Normally built by `warm_up` already; otherwise build it now (once).
    let detector = DETECTOR.get_or_init(build);
    let lang = if sample.chars().filter(|c| c.is_alphabetic()).count() <= 24 {
        short_guess(detector, &sample)?
    } else {
        detector.detect_language_of(&sample)?
    };
    let code = LANGS.iter().find(|(l, _)| *l == lang).map(|(_, c)| *c)?;
    // Script and lingua must agree (lingua can be wrong on short texts).
    match script {
        Script::Cyrillic if !matches!(code, "ru" | "uk") => Some("ru"),
        Script::Latin if matches!(code, "ru" | "uk") => None,
        _ => Some(code),
    }
}

/// How often people translate *from* a language, as a prior for words and
/// short phrases, where lingua's statistics are thin ("hello" alone looks
/// as Italian as English to it).
fn prior(lang: Language) -> f64 {
    match lang {
        English => 3.0,
        Russian => 2.0,
        German | French | Spanish => 1.3,
        _ => 1.0,
    }
}

/// lingua's confidence for each language, weighted by [`prior`].
fn short_guess(detector: &LanguageDetector, text: &str) -> Option<Language> {
    detector
        .compute_language_confidence_values(text)
        .into_iter()
        .map(|(l, c)| (l, c * prior(l)))
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .filter(|(_, c)| *c > 0.0)
        .map(|(l, _)| l)
}

/// Whether the detector is built (if not, [`detect`] may block a while).
pub fn ready() -> bool {
    DETECTOR.get().is_some()
}

/// The first ~400 characters: enough for detection, cheap for long texts.
fn sample(text: &str) -> String {
    text.chars().take(400).collect()
}

enum Script {
    /// The script alone says which language.
    Only(&'static str),
    Cyrillic,
    Latin,
    /// Arabic script: Arabic or Persian.
    Arabic,
    /// Han characters: Chinese, or Japanese written mostly in kanji.
    Han,
}

/// The dominant script of the letters in `text`; `None` without letters.
fn script_of(text: &str) -> Option<Script> {
    let (mut latin, mut cyr, mut greek, mut arabic, mut hebrew, mut han, mut kana, mut hangul) =
        (0, 0, 0, 0, 0, 0, 0, 0);
    let (mut thai, mut georgian, mut armenian, mut devanagari) = (0, 0, 0, 0);
    for c in text.chars().filter(|c| c.is_alphabetic()) {
        match c as u32 {
            0x0041..=0x024F | 0x1E00..=0x1EFF => latin += 1,
            0x0400..=0x052F => cyr += 1,
            0x0370..=0x03FF | 0x1F00..=0x1FFF => greek += 1,
            0x0590..=0x05FF => hebrew += 1,
            0x0600..=0x06FF | 0x0750..=0x077F | 0xFB50..=0xFDFF | 0xFE70..=0xFEFF => arabic += 1,
            0x0900..=0x097F => devanagari += 1,
            0x0E00..=0x0E7F => thai += 1,
            0x10A0..=0x10FF => georgian += 1,
            0x0530..=0x058F => armenian += 1,
            0x3040..=0x30FF => kana += 1,
            0xAC00..=0xD7AF | 0x1100..=0x11FF => hangul += 1,
            0x4E00..=0x9FFF | 0x3400..=0x4DBF => han += 1,
            _ => {}
        }
    }
    let counts = [
        (latin, 0),
        (cyr, 1),
        (greek, 2),
        (hebrew, 3),
        (arabic, 4),
        (devanagari, 5),
        (thai, 6),
        (georgian, 7),
        (armenian, 8),
        (kana + han, 9),
        (hangul, 10),
    ];
    let (n, which) = counts.into_iter().max_by_key(|(n, _)| *n)?;
    if n == 0 {
        return None;
    }
    Some(match which {
        0 => Script::Latin,
        1 => Script::Cyrillic,
        2 => Script::Only("el"),
        3 => Script::Only("he"),
        4 => Script::Arabic,
        5 => Script::Only("hi"),
        6 => Script::Only("th"),
        7 => Script::Only("ka"),
        8 => Script::Only("hy"),
        // Any kana means Japanese; Han alone needs lingua.
        9 if kana > 0 => Script::Only("ja"),
        9 => Script::Han,
        _ => Script::Only("ko"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ready() {
        DETECTOR.get_or_init(build);
    }

    #[test]
    fn scripts_decide_alone() {
        assert_eq!(detect("Γειά σου κόσμε"), Some("el"));
        assert_eq!(detect("こんにちは世界"), Some("ja"));
        assert_eq!(detect("안녕하세요"), Some("ko"));
        assert_eq!(detect("12345 !!"), None);
    }

    #[test]
    fn lingua_picks_within_a_script() {
        ready();
        assert_eq!(detect("Where is the nearest pharmacy?"), Some("en"));
        assert_eq!(detect("Где находится ближайшая аптека?"), Some("ru"));
        assert_eq!(detect("Де знаходиться найближча аптека?"), Some("uk"));
        assert_eq!(detect("Wo ist die nächste Apotheke?"), Some("de"));
        assert_eq!(detect("Où est la pharmacie la plus proche ?"), Some("fr"));
        assert_eq!(detect("最近的药店在哪里？"), Some("zh-CN"));
        assert_eq!(detect("سلام دنیا، چطوری؟"), Some("fa"));
        assert_eq!(detect("مرحبا بالعالم"), Some("ar"));
    }

    #[test]
    fn short_words() {
        ready();
        assert_eq!(detect("привет"), Some("ru"));
        assert_eq!(detect("hello"), Some("en"));
        assert_eq!(detect("effectively"), Some("en"));
        assert_eq!(detect("Guten Morgen"), Some("de"));
        assert_eq!(detect("buongiorno a tutti"), Some("it"));
        assert_eq!(detect("дякую"), Some("uk"));
    }
}
