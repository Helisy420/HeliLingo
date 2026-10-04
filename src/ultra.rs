//! Ultra mode: a selection that mixes languages is split into fragments
//! (by line, then by sentence), each fragment's language is detected, and
//! only the fragments not already in the target language are translated.

use crate::translate::Error;

#[derive(Clone, Debug)]
pub struct Fragment {
    /// The fragment without surrounding whitespace.
    pub text: String,
    /// Whitespace that followed it in the selection (kept when rebuilding).
    pub sep: String,
    /// Detected (or assumed) language code; `None` while pending.
    pub lang: Option<String>,
    pub state: FragmentState,
}

#[derive(Clone, Debug)]
pub enum FragmentState {
    Pending,
    Translated(String),
    /// Already in the target language: left as is.
    Unchanged,
    Failed(Error),
}

impl Fragment {
    /// The text that goes into "Copy all" / "Replace selection".
    pub fn output(&self) -> &str {
        match &self.state {
            FragmentState::Translated(t) => t,
            _ => &self.text,
        }
    }
}

/// Leading whitespace of the selection, then fragments.
pub fn split(text: &str) -> (String, Vec<Fragment>) {
    let lead_len = text.len() - text.trim_start().len();
    let lead = text[..lead_len].to_owned();
    let mut out: Vec<Fragment> = Vec::new();
    let mut rest = &text[lead_len..];
    while !rest.is_empty() {
        let end = fragment_end(rest);
        let (piece, tail) = rest.split_at(end);
        let ws = tail.len() - tail.trim_start().len();
        let (sep, next) = tail.split_at(ws);
        let piece = piece.trim_end();
        let trailing = &rest[piece.len()..end];
        out.push(Fragment {
            text: piece.to_owned(),
            sep: format!("{trailing}{sep}"),
            lang: None,
            state: FragmentState::Pending,
        });
        rest = next;
    }
    out.retain(|f| !f.text.is_empty());
    (lead, out)
}

/// Byte index where the first fragment of `s` ends: at a line break, or
/// after sentence punctuation (and closing quotes/brackets) followed by
/// whitespace.
fn fragment_end(s: &str) -> usize {
    let chars: Vec<(usize, char)> = s.char_indices().collect();
    let mut i = 0;
    while i < chars.len() {
        let (pos, c) = chars[i];
        if c == '\n' || c == '\r' {
            return pos;
        }
        if matches!(c, '.' | '!' | '?' | '…' | '。' | '！' | '？') {
            let mut j = i + 1;
            while j < chars.len() && matches!(chars[j].1, '.' | '!' | '?' | '…' | '"' | '\'' | '»' | '”' | '’' | ')' | ']') {
                j += 1;
            }
            match chars.get(j) {
                None => return s.len(),
                Some(&(p, w)) if w.is_whitespace() => return p,
                _ => {}
            }
            i = j;
            continue;
        }
        i += 1;
    }
    s.len()
}

/// Rebuilds the selection with every fragment replaced by its output.
pub fn join(lead: &str, fragments: &[Fragment]) -> String {
    let mut s = lead.to_owned();
    for f in fragments {
        s.push_str(f.output());
        s.push_str(&f.sep);
    }
    s.trim_end().to_owned()
}

/// One language of the summary line: code, fragment count, and whether
/// these fragments were left unchanged (target language).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LangGroup {
    pub lang: String,
    pub count: usize,
    pub unchanged: bool,
}

/// Languages by fragment count (most first), the unchanged target-language
/// group last. Pending fragments are not counted.
pub fn summary(fragments: &[Fragment]) -> Vec<LangGroup> {
    let mut groups: Vec<LangGroup> = Vec::new();
    for f in fragments {
        let Some(lang) = &f.lang else { continue };
        let unchanged = matches!(f.state, FragmentState::Unchanged);
        match groups.iter_mut().find(|g| g.lang == *lang && g.unchanged == unchanged) {
            Some(g) => g.count += 1,
            None => groups.push(LangGroup { lang: lang.clone(), count: 1, unchanged }),
        }
    }
    // Stable: equal counts keep the order of first appearance.
    groups.sort_by_key(|g| (g.unchanged, std::cmp::Reverse(g.count)));
    groups
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_lines_then_sentences() {
        let text = "Please send me the report by Friday. Die Besprechung wurde verschoben!\nGracias por tu ayuda.\n\nЭто уже по-русски.";
        let (lead, f) = split(text);
        assert_eq!(lead, "");
        let texts: Vec<&str> = f.iter().map(|f| f.text.as_str()).collect();
        assert_eq!(
            texts,
            [
                "Please send me the report by Friday.",
                "Die Besprechung wurde verschoben!",
                "Gracias por tu ayuda.",
                "Это уже по-русски."
            ]
        );
        assert_eq!(f[1].sep, "\n");
        assert_eq!(f[2].sep, "\n\n");
        assert_eq!(join(&lead, &f), text);
    }

    #[test]
    fn keeps_abbreviation_like_dots_and_quotes() {
        let (_, f) = split("Version 2.5 is out. He said \"hi.\" Then left");
        let texts: Vec<&str> = f.iter().map(|f| f.text.as_str()).collect();
        assert_eq!(texts, ["Version 2.5 is out.", "He said \"hi.\"", "Then left"]);
    }

    #[test]
    fn join_uses_translations() {
        let (lead, mut f) = split("  Hello. Привет.");
        f[0].state = FragmentState::Translated("Здравствуйте.".into());
        f[1].state = FragmentState::Unchanged;
        assert_eq!(join(&lead, &f), "  Здравствуйте. Привет.");
    }

    #[test]
    fn summary_orders_by_count_target_last() {
        let mk = |lang: &str, state| Fragment { text: "x".into(), sep: String::new(), lang: Some(lang.into()), state };
        let t = || FragmentState::Translated("y".into());
        let f = vec![
            mk("de", t()),
            mk("en", t()),
            mk("ru", FragmentState::Unchanged),
            mk("en", t()),
            mk("es", t()),
        ];
        let s = summary(&f);
        let order: Vec<(&str, usize, bool)> = s.iter().map(|g| (g.lang.as_str(), g.count, g.unchanged)).collect();
        assert_eq!(order, [("en", 2, false), ("de", 1, false), ("es", 1, false), ("ru", 1, true)]);
    }
}
