//! Prompts for the LLM engines (OpenRouter, TranslateGemma via llama.cpp),
//! the glossary, and cleaning up model output.
//!
//! [`VERSION`] is part of every cache key: bump it whenever a prompt or the
//! output clean-up changes, so old answers aren't served from the cache.

use crate::lang;

/// Prompt/clean-up version (cache keys include it).
pub const VERSION: u32 = 1;

/// An earlier fragment and its translation («Переводить с контекстом»).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ContextPair {
    pub source: String,
    pub translation: String,
}

/// English name of a language, as prompts spell it.
pub fn english_name(code: &str) -> String {
    let base = code.split('-').next().unwrap_or(code);
    lang::LANGUAGES
        .iter()
        .find(|(c, _, _)| c.split('-').next().is_some_and(|b| b.eq_ignore_ascii_case(base)))
        .map_or_else(|| base.to_owned(), |(_, en, _)| (*en).to_owned())
}

/// The user's glossary: `term<TAB>translation` per line. Lenient: a BOM,
/// blank lines, `#` comments and extra columns are fine.
pub fn parse_glossary(text: &str) -> Vec<(String, String)> {
    text.trim_start_matches('\u{feff}')
        .lines()
        .filter(|l| !l.trim_start().starts_with('#'))
        .filter_map(|l| {
            let mut cols = l.split('\t').map(str::trim).filter(|c| !c.is_empty());
            Some((cols.next()?.to_owned(), cols.next()?.to_owned()))
        })
        .collect()
}

/// `glossary.tsv`, read again only when the file changes (one metadata
/// call per use). Missing file = no terms.
pub struct Glossary {
    path: std::path::PathBuf,
    cached: std::sync::Mutex<Option<(std::time::SystemTime, Terms)>>,
}

/// Glossary terms: (term, translation).
pub type Terms = std::sync::Arc<Vec<(String, String)>>;

impl Glossary {
    pub fn new(path: std::path::PathBuf) -> Self {
        Self { path, cached: Default::default() }
    }

    pub fn terms(&self) -> std::sync::Arc<Vec<(String, String)>> {
        let Ok(modified) = std::fs::metadata(&self.path).and_then(|m| m.modified()) else {
            return Default::default();
        };
        let mut c = self.cached.lock().unwrap();
        if let Some((at, terms)) = c.as_ref()
            && *at == modified
        {
            return terms.clone();
        }
        let terms = std::sync::Arc::new(parse_glossary(&std::fs::read_to_string(&self.path).unwrap_or_default()));
        *c = Some((modified, terms.clone()));
        terms
    }
}

/// Glossary terms that occur in `text` (case-insensitive), so prompts
/// carry only what matters.
pub fn relevant<'a>(glossary: &'a [(String, String)], text: &str) -> Vec<&'a (String, String)> {
    let lower = text.to_lowercase();
    glossary.iter().filter(|(term, _)| lower.contains(&term.to_lowercase())).collect()
}

/// OpenRouter chat messages: (role, content). Earlier fragments come as
/// previous turns, glossary terms as rules in the system message.
pub fn chat(source: &str, target: &str, text: &str, context: &[ContextPair], glossary: &[(String, String)]) -> Vec<(&'static str, String)> {
    let to = english_name(target);
    let from = if source == lang::AUTO { "the source language".to_owned() } else { english_name(source) };
    let mut system = format!(
        "You are a professional translator. Translate the user's text from {from} into {to}. \
         Keep the meaning, tone, formatting, line breaks, numbers, names, URLs and code exactly. \
         Answer with the translation only: no quotes, no notes, no explanations."
    );
    let terms = relevant(glossary, text);
    if !terms.is_empty() {
        system.push_str("\nAlways translate these terms this way:");
        for (term, tr) in terms {
            system.push_str(&format!("\n{term} → {tr}"));
        }
    }
    let mut msgs = vec![("system", system)];
    for pair in context {
        msgs.push(("user", pair.source.trim().to_owned()));
        msgs.push(("assistant", pair.translation.trim().to_owned()));
    }
    msgs.push(("user", text.trim().to_owned()));
    msgs
}

/// TranslateGemma's own chat template, rendered by hand for llama-server's
/// raw `/completion` (its bundled template rejects plain chat messages).
/// Context as earlier turns, glossary inside the last one.
pub fn translategemma(source: &str, target: &str, text: &str, context: &[ContextPair], glossary: &[(String, String)]) -> String {
    let turn = |text: &str, terms: &[&(String, String)]| {
        let (s, t) = (english_name(source), english_name(target));
        let code = |c: &str| c.split('-').next().unwrap_or(c).to_ascii_lowercase();
        let (sc, tc) = (code(source), code(target));
        let mut p = format!(
            "<start_of_turn>user\nYou are a professional {s} ({sc}) to {t} ({tc}) translator. Your goal is to \
             accurately convey the meaning and nuances of the original {s} text while adhering to {t} grammar, \
             vocabulary, and cultural sensitivities.\n"
        );
        if !terms.is_empty() {
            p.push_str("Use these translations for the following terms:\n");
            for (term, tr) in terms {
                p.push_str(&format!("{term} → {tr}\n"));
            }
        }
        p.push_str(&format!(
            "Produce only the {t} translation, without any additional explanations or commentary. Please \
             translate the following {s} text into {t}:\n\n\n{}<end_of_turn>\n",
            text.trim()
        ));
        p
    };
    let mut p = String::new();
    for pair in context {
        p.push_str(&turn(&pair.source, &[]));
        p.push_str("<start_of_turn>model\n");
        p.push_str(pair.translation.trim());
        p.push_str("<end_of_turn>\n");
    }
    p.push_str(&turn(text, &relevant(glossary, text)));
    p.push_str("<start_of_turn>model\n");
    p
}

/// Cleans an LLM answer: end tokens, a code fence around everything,
/// "Translation:" labels, and quotes the source didn't have.
pub fn clean_output(source: &str, answer: &str) -> String {
    let mut s = answer.split("<end_of_turn>").next().unwrap_or(answer).trim().to_owned();
    if s.starts_with("```") && s.ends_with("```") && s.len() > 6 && !source.trim_start().starts_with("```") {
        s = s.trim_start_matches('`').trim_end_matches('`').to_owned();
        // Drop a language tag line after the opening fence.
        if let Some((first, rest)) = s.split_once('\n')
            && !first.contains(' ')
            && first.len() < 15
        {
            s = rest.to_owned();
        }
        s = s.trim().to_owned();
    }
    for label in ["Translation:", "Перевод:", "Translated text:"] {
        if let Some(rest) = s.strip_prefix(label) {
            s = rest.trim_start().to_owned();
        }
    }
    let src = source.trim();
    for (open, close) in [('"', '"'), ('«', '»'), ('“', '”'), ('\'', '\'')] {
        let quoted = s.starts_with(open) && s.ends_with(close) && s.chars().count() >= 2;
        if quoted && !(src.starts_with(open) || src.starts_with('"') || src.starts_with('«') || src.starts_with('“')) {
            let inner: String = s.chars().skip(1).collect();
            let mut inner: Vec<char> = inner.chars().collect();
            inner.pop();
            s = inner.into_iter().collect::<String>().trim().to_owned();
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gemma_matches_the_official_template() {
        let p = translategemma("cs", "de", "V nejhorším případě i k prasknutí čočky.", &[], &[]);
        assert_eq!(
            p,
            "<start_of_turn>user\nYou are a professional Czech (cs) to German (de) translator. Your goal is to \
             accurately convey the meaning and nuances of the original Czech text while adhering to German grammar, \
             vocabulary, and cultural sensitivities.\nProduce only the German translation, without any additional \
             explanations or commentary. Please translate the following Czech text into German:\n\n\nV nejhorším \
             případě i k prasknutí čočky.<end_of_turn>\n<start_of_turn>model\n"
        );
    }

    #[test]
    fn chat_with_context_and_glossary() {
        let ctx = [ContextPair { source: "The build failed.".into(), translation: "Сборка упала.".into() }];
        let g = parse_glossary("\u{feff}# terms\nbuild\tсборка\nunused\tx\n\nlonely\n");
        assert_eq!(g.len(), 2);
        let m = chat("auto", "ru", "Restart the build.", &ctx, &g);
        assert_eq!(m.len(), 4);
        assert!(m[0].1.contains("into Russian") && m[0].1.contains("build → сборка") && !m[0].1.contains("unused"));
        assert_eq!(m[2], ("assistant", "Сборка упала.".to_owned()));
        assert_eq!(m[3], ("user", "Restart the build.".to_owned()));
    }

    #[test]
    fn cleans_answers() {
        assert_eq!(clean_output("Hi", "  Привет<end_of_turn>junk"), "Привет");
        assert_eq!(clean_output("Hi", "\"Привет\""), "Привет");
        assert_eq!(clean_output("\"Hi\"", "\"Привет\""), "\"Привет\"");
        assert_eq!(clean_output("Hi", "Translation: Привет"), "Привет");
        assert_eq!(clean_output("x = 1", "```python\nx = 1\n```"), "x = 1");
    }
}
