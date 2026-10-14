//! Text handling around translation: normalizing a selection, and splitting
//! text into segments (lines, then sentences) that can be translated one by
//! one and put back with the original spacing — Ultra mode, sentence pairs
//! in the translator, and offline models with a short input limit.

/// Trims, turns Windows line breaks into `\n`, drops zero-width characters
/// and collapses runs of spaces inside lines (selections from PDFs and web
/// pages are full of them). Line breaks are kept.
pub fn normalize(text: &str) -> String {
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    let mut out = String::with_capacity(text.len());
    for (i, line) in text.split('\n').enumerate() {
        if i > 0 {
            out.push('\n');
        }
        let mut space = false;
        for c in line.chars() {
            match c {
                '\u{200B}' | '\u{200C}' | '\u{200D}' | '\u{FEFF}' | '\u{00AD}' => {}
                c if c == ' ' || c == '\t' || c == '\u{00A0}' => space = true,
                c => {
                    if space && !out.is_empty() && !out.ends_with('\n') {
                        out.push(' ');
                    }
                    space = false;
                    out.push(c);
                }
            }
        }
    }
    // At most one empty line in a row.
    while out.contains("\n\n\n") {
        out = out.replace("\n\n\n", "\n\n");
    }
    out.trim().to_owned()
}

/// A piece of text and the whitespace after it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Segment {
    pub text: String,
    pub sep: String,
}

/// Splits into sentences (and lines). Joining `text + sep` of every segment
/// gives back the input without its leading whitespace.
pub fn segments(text: &str) -> Vec<Segment> {
    let mut out = Vec::new();
    let mut rest = text.trim_start();
    while !rest.is_empty() {
        let end = sentence_end(rest);
        let (piece, tail) = rest.split_at(end);
        let trimmed = piece.trim_end();
        let ws = tail.len() - tail.trim_start().len();
        let sep = format!("{}{}", &piece[trimmed.len()..], &tail[..ws]);
        if !trimmed.is_empty() {
            out.push(Segment { text: trimmed.to_owned(), sep });
        } else if let Some(last) = out.last_mut() {
            last.sep.push_str(&sep);
        }
        rest = &tail[ws..];
    }
    out
}

/// Puts segments back together (with their own text replaced by `texts`
/// where given).
pub fn join(segments: &[Segment], texts: &[String]) -> String {
    let mut s = String::new();
    for (i, seg) in segments.iter().enumerate() {
        s.push_str(texts.get(i).map_or(seg.text.as_str(), String::as_str));
        s.push_str(&seg.sep);
    }
    s.trim_end().to_owned()
}

/// Words that end with a dot but don't end a sentence.
const ABBREVIATIONS: &[&str] = &[
    "mr", "mrs", "ms", "dr", "prof", "st", "vs", "etc", "e.g", "i.e", "approx", "no", "fig", "vol", "jr", "sr",
    "inc", "ltd", "co", "т", "е", "д", "т.е", "т.д", "т.п", "и.т.д", "г", "гг", "ул", "др", "пр", "см", "стр", "рис",
    "им", "св", "тыс", "млн", "млрд", "руб", "коп", "z.b", "bzw", "usw",
];

/// Byte index where the first segment of `s` ends: a line break, or
/// sentence punctuation (plus closing quotes/brackets) before whitespace —
/// but not after an abbreviation or a single initial ("J. R. R.").
fn sentence_end(s: &str) -> usize {
    let chars: Vec<(usize, char)> = s.char_indices().collect();
    let mut i = 0;
    while i < chars.len() {
        let (pos, c) = chars[i];
        if c == '\n' {
            return pos;
        }
        if matches!(c, '.' | '!' | '?' | '…' | '。' | '！' | '？') {
            let mut j = i + 1;
            while j < chars.len() && matches!(chars[j].1, '.' | '!' | '?' | '…' | '"' | '\'' | '»' | '”' | '’' | ')' | ']') {
                j += 1;
            }
            // CJK full stops end a sentence even without a space.
            if matches!(c, '。' | '！' | '？') {
                return chars.get(j).map_or(s.len(), |&(p, _)| p);
            }
            match chars.get(j) {
                None => return s.len(),
                Some(&(p, w)) if w.is_whitespace() && !(c == '.' && is_abbreviation(&s[..pos])) => return p,
                _ => {}
            }
            i = j;
            continue;
        }
        i += 1;
    }
    s.len()
}

/// Does `before` (the text up to a dot) end with an abbreviation?
fn is_abbreviation(before: &str) -> bool {
    let word: String = before
        .chars()
        .rev()
        .take_while(|c| c.is_alphabetic() || *c == '.')
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    if word.is_empty() {
        return false;
    }
    // A single letter: an initial or "т. е.".
    if word.chars().count() == 1 {
        return true;
    }
    ABBREVIATIONS.contains(&word.to_lowercase().as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_selections() {
        assert_eq!(normalize("  Hello\u{00A0}\u{00A0}world \r\n\r\n\r\nnext\u{200B}  "), "Hello world\n\nnext");
    }

    #[test]
    fn splits_sentences_and_lines() {
        let s = segments("The meeting has been moved to Thursday. Could you send me the slides?\nThanks!");
        let texts: Vec<_> = s.iter().map(|x| x.text.as_str()).collect();
        assert_eq!(texts, ["The meeting has been moved to Thursday.", "Could you send me the slides?", "Thanks!"]);
        assert_eq!(join(&s, &[]), "The meeting has been moved to Thursday. Could you send me the slides?\nThanks!");
    }

    #[test]
    fn keeps_abbreviations_together() {
        let s = segments("Mr. Smith arrived, e.g. today. Т. е. всё хорошо. J. R. R. Tolkien wrote it.");
        let texts: Vec<_> = s.iter().map(|x| x.text.as_str()).collect();
        assert_eq!(texts, ["Mr. Smith arrived, e.g. today.", "Т. е. всё хорошо.", "J. R. R. Tolkien wrote it."]);
    }

    #[test]
    fn joins_with_replacements() {
        let s = segments("One. Two.");
        assert_eq!(join(&s, &["Один.".into(), "Два.".into()]), "Один. Два.");
    }

    #[test]
    fn cjk_stops() {
        let s = segments("你好。我很好！");
        assert_eq!(s.len(), 2);
    }
}
