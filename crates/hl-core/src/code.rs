//! Programmer mode: when the text is source code, only string literals and
//! comments are translated; the code around them stays byte for byte, and
//! identifiers, keys, paths and format strings ("user_id", "/api/v1",
//! "%Y-%m-%d") are left alone.

/// One piece of source code.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Piece {
    Code(String),
    /// A string literal: opening quote, contents, closing quote.
    Str { open: String, body: String, close: String },
    /// A comment: marker (`//`, `/*`, `#`, `--`, `<!--`), body, closing marker.
    Comment { open: String, body: String, close: String },
}

impl Piece {
    /// The human text to translate, if this piece has any.
    pub fn text(&self) -> Option<&str> {
        match self {
            Piece::Str { body, .. } if is_prose(body) => Some(body),
            Piece::Comment { body, .. } if body.chars().any(char::is_alphabetic) => Some(body),
            _ => None,
        }
    }
}

const KEYWORDS: &[&str] = &[
    "fn ", "let ", "const ", "def ", "class ", "import ", "return ", "function ", "var ", "public ", "private ",
    "#include", "using ", "package ", "struct ", "impl ", "if (", "for (", "while (", "elif ", "print(", "println!",
    "console.log", "=>", "==", "!=", "::", "->", "&&", "||",
];

/// Does this look like source code rather than prose? Scores typical signs
/// (line endings `;{}`, keywords, operators, indentation, comments).
pub fn is_code(text: &str) -> bool {
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    if lines.is_empty() {
        return false;
    }
    let mut score = 0i32;
    for l in &lines {
        let t = l.trim_end();
        let head = t.trim_start();
        if t.ends_with(';')
            || t.ends_with('{')
            || t.ends_with('}')
            || (t.ends_with(':') && head.starts_with(|c: char| c.is_ascii_lowercase()))
        {
            score += 2;
        }
        if l.starts_with("    ") || l.starts_with('\t') {
            score += 1;
        }
        score += KEYWORDS.iter().filter(|k| t.contains(*k)).count() as i32 * 2;
        if head.starts_with("//") || head.starts_with("/*") {
            score += 2;
        }
    }
    score >= 4 && score as f32 / lines.len() as f32 >= 2.0
}

/// Splits source into code, string literals and comments.
pub fn split(src: &str) -> Vec<Piece> {
    let chars: Vec<char> = src.chars().collect();
    let at = |i: usize, s: &str| s.chars().enumerate().all(|(k, c)| chars.get(i + k) == Some(&c));
    let mut out = Vec::new();
    let mut code = String::new();
    let flush = |code: &mut String, out: &mut Vec<Piece>| {
        if !code.is_empty() {
            out.push(Piece::Code(std::mem::take(code)));
        }
    };
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let line_start = i == 0 || chars[i - 1] == '\n' || code.ends_with(char::is_whitespace);
        let comment = if at(i, "//") {
            Some(("//", "\n"))
        } else if at(i, "/*") {
            Some(("/*", "*/"))
        } else if at(i, "<!--") {
            Some(("<!--", "-->"))
        } else if c == '#' && line_start && chars.get(i + 1).is_some_and(|n| *n == ' ' || *n == '#') {
            // `# comment` (Python, shell, YAML), not `#include`.
            Some(("#", "\n"))
        } else if line_start && at(i, "-- ") {
            Some(("--", "\n"))
        } else {
            None
        };
        if let Some((open, close)) = comment {
            flush(&mut code, &mut out);
            let start = i + open.chars().count();
            let mut j = start;
            while j < chars.len() && !at(j, close) {
                j += 1;
            }
            let body: String = chars[start..j].iter().collect();
            // A line comment leaves its newline to the code after it.
            let closed = close != "\n" && j < chars.len();
            i = if closed { j + close.chars().count() } else { j };
            out.push(Piece::Comment { open: open.into(), body, close: if closed { close.into() } else { String::new() } });
            continue;
        }
        // Rust lifetimes ('a, 'static) after `<`, `&` or as 'static: code.
        let lifetime = c == '\''
            && (i > 0 && matches!(chars[i - 1], '<' | '&') || at(i + 1, "static"))
            && chars.get(i + 1).is_some_and(|n| n.is_alphabetic() || *n == '_');
        if !lifetime && (c == '"' || c == '\'' || c == '`') {
            let triple: String = std::iter::repeat_n(c, 3).collect();
            let quote = if at(i, &triple) { triple } else { c.to_string() };
            let qlen = quote.chars().count();
            let mut j = i + qlen;
            let mut closed = false;
            while j < chars.len() {
                if chars[j] == '\\' {
                    j += 2;
                    continue;
                }
                if at(j, &quote) {
                    closed = true;
                    break;
                }
                // A one-line quote that doesn't close on its line is an
                // apostrophe or a lifetime ('a), not a string.
                if chars[j] == '\n' && qlen == 1 && c != '`' {
                    break;
                }
                j += 1;
            }
            if closed {
                flush(&mut code, &mut out);
                let body: String = chars[i + qlen..j].iter().collect();
                out.push(Piece::Str { open: quote.clone(), body, close: quote });
                i = j + qlen;
                continue;
            }
        }
        code.push(c);
        i += 1;
    }
    flush(&mut code, &mut out);
    out
}

/// Prose worth translating inside a string: letters, and not an identifier,
/// key, path, URL, format string or number.
fn is_prose(s: &str) -> bool {
    let letters = s.chars().filter(|c| c.is_alphabetic()).count();
    if letters < 2 {
        return false;
    }
    if s.contains(char::is_whitespace) || !s.is_ascii() {
        return true;
    }
    // One ASCII token: "Cancel" yes; "utf-8", "user_id", "/api", "%Y", "camelCase" no.
    let symbolic = s.chars().any(|c| "./\\_:%{}=<>[]@#$&?*+".contains(c));
    let starts_upper = s.chars().next().is_some_and(char::is_uppercase);
    let rest_lower = s.chars().skip(1).all(|c| c.is_lowercase() || c == '\'' || c == '-');
    !symbolic && starts_upper && rest_lower && letters >= 3
}

/// The texts to translate, in order.
pub fn texts(pieces: &[Piece]) -> Vec<String> {
    pieces.iter().filter_map(|p| p.text().map(str::to_owned)).collect()
}

/// Rebuilds the code with `translations` (in the order of [`texts`]).
pub fn join(pieces: &[Piece], translations: &[String]) -> String {
    let mut next = translations.iter();
    let mut s = String::new();
    for p in pieces {
        match p {
            Piece::Code(c) => s.push_str(c),
            Piece::Str { open, body, close } | Piece::Comment { open, body, close } => {
                s.push_str(open);
                if p.text().is_some() {
                    let t = next.next().map_or(body.as_str(), String::as_str);
                    s.push_str(&keep_edges(body, t, matches!(p, Piece::Str { .. }), open));
                } else {
                    s.push_str(body);
                }
                s.push_str(close);
            }
        }
    }
    s
}

/// The translation with the original's surrounding whitespace, and the
/// literal's own quote escaped inside it.
fn keep_edges(original: &str, translated: &str, in_string: bool, quote: &str) -> String {
    let lead = &original[..original.len() - original.trim_start().len()];
    let trail = &original[original.trim_end().len()..];
    let mut t = translated.trim().to_owned();
    if in_string && quote.chars().count() == 1 {
        t = t.replace(quote, &format!("\\{quote}"));
    }
    format!("{lead}{t}{trail}")
}

#[cfg(test)]
mod tests {
    use super::*;

    const RUST: &str = "// Show the greeting\nfn main() {\n    let key = \"user_id\";\n    println!(\"Hello, world\");\n}\n";

    #[test]
    fn detects_code_not_prose() {
        assert!(is_code(RUST));
        assert!(is_code("def f(x):\n    return x * 2  # double it\n"));
        assert!(!is_code("The meeting has been moved to Thursday. Could you send the slides?"));
    }

    #[test]
    fn translates_only_strings_and_comments() {
        let p = split(RUST);
        assert_eq!(texts(&p), [" Show the greeting", "Hello, world"]);
        let out = join(&p, &["Показать приветствие".into(), "Привет, \"мир\"".into()]);
        assert_eq!(
            out,
            "// Показать приветствие\nfn main() {\n    let key = \"user_id\";\n    println!(\"Привет, \\\"мир\\\"\");\n}\n"
        );
    }

    #[test]
    fn apostrophes_and_lifetimes_are_code() {
        let p = split("fn f<'a>(x: &'a str) {}\n");
        assert!(texts(&p).is_empty());
    }
}
