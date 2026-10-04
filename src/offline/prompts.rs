//! Prompts for the llama.cpp tiers (TranslateGemma).
//!
//! The base is TranslateGemma's own chat template (from the GGUF's
//! `tokenizer.chat_template`), rendered by hand so we can send a raw
//! prompt to llama-server's `/completion`. Context and glossary follow the
//! HY-MT recipe: previous fragments go in as earlier user/model turns of
//! the same conversation (the template allows alternating turns), and
//! glossary terms found in the text are listed as required translations
//! ("terminology intervention") inside the final user turn.

use super::ContextPair;

/// English language name as TranslateGemma's template spells it.
pub fn lang_name(code: &str) -> String {
    let base = super::base_code(code);
    crate::settings::LANGUAGES
        .iter()
        .find(|(c, _, _)| super::base_code(c).eq_ignore_ascii_case(base))
        .map(|(_, en, _)| (*en).to_owned())
        .unwrap_or_else(|| base.to_owned())
}

/// The code the template accepts ("zh-CN" isn't one of its keys; "zh" is).
pub fn template_code(code: &str) -> String {
    super::base_code(code).to_ascii_lowercase()
}

fn user_turn(src: &str, tgt: &str, text: &str, glossary: &[(String, String)]) -> String {
    let (s, t) = (lang_name(src), lang_name(tgt));
    let (sc, tc) = (template_code(src), template_code(tgt));
    let mut p = format!(
        "<start_of_turn>user\nYou are a professional {s} ({sc}) to {t} ({tc}) translator. Your goal is to accurately \
         convey the meaning and nuances of the original {s} text while adhering to {t} grammar, vocabulary, and \
         cultural sensitivities.\n"
    );
    if !glossary.is_empty() {
        p.push_str("Use these translations for the following terms:\n");
        for (term, tr) in glossary {
            p.push_str(&format!("{term} → {tr}\n"));
        }
    }
    p.push_str(&format!(
        "Produce only the {t} translation, without any additional explanations or commentary. Please translate the \
         following {s} text into {t}:\n\n\n{}<end_of_turn>\n",
        text.trim()
    ));
    p
}

/// The full prompt (without BOS: llama-server adds it). Ends with the
/// open model turn; generation stops at `<end_of_turn>`.
pub fn translategemma(src: &str, tgt: &str, text: &str, context: &[ContextPair], glossary: &[(String, String)]) -> String {
    let mut p = String::new();
    for pair in context {
        p.push_str(&user_turn(src, tgt, &pair.source, &[]));
        p.push_str("<start_of_turn>model\n");
        p.push_str(pair.translation.trim());
        p.push_str("<end_of_turn>\n");
    }
    p.push_str(&user_turn(src, tgt, text, glossary));
    p.push_str("<start_of_turn>model\n");
    p
}

/// Cleans the model's answer: stray end tokens and surrounding space.
pub fn clean_output(s: &str) -> String {
    s.split("<end_of_turn>").next().unwrap_or(s).trim().to_owned()
}

/// Joins previous source fragments for DeepL's `context` parameter.
pub fn deepl_context(context: &[ContextPair]) -> Option<String> {
    let s = context.iter().map(|p| p.source.trim()).collect::<Vec<_>>().join("\n");
    (!s.is_empty()).then_some(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_the_official_template_without_extras() {
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
    fn context_and_glossary() {
        let ctx = [ContextPair { source: "The build failed.".into(), translation: "Сборка упала.".into() }];
        let g = [("build".to_owned(), "сборка".to_owned())];
        let p = translategemma("en", "zh-CN", "Restart the build.", &ctx, &g);
        assert_eq!(p.matches("<start_of_turn>user").count(), 2);
        assert!(p.contains("Сборка упала.<end_of_turn>\n<start_of_turn>user"));
        assert!(p.contains("English (en) to Chinese (zh)"));
        assert!(p.contains("build → сборка\nProduce only"));
        assert!(p.ends_with("Restart the build.<end_of_turn>\n<start_of_turn>model\n"));
        // The glossary is only in the last turn.
        assert_eq!(p.matches("build → сборка").count(), 1);
    }

    #[test]
    fn output_and_deepl_context() {
        assert_eq!(clean_output("  Привет<end_of_turn>junk"), "Привет");
        assert_eq!(deepl_context(&[]), None);
        let ctx = [
            ContextPair { source: "One.".into(), translation: "Раз.".into() },
            ContextPair { source: "Two.".into(), translation: "Два.".into() },
        ];
        assert_eq!(deepl_context(&ctx).as_deref(), Some("One.\nTwo."));
    }
}
