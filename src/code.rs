//! Programmer mode: when the text to translate is source code, only its
//! string literals and comments are translated; the code around them is
//! left byte-for-byte as it was (Settings → Context & modes).

/// One piece of the source text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Piece {
    Code(String),
    /// A string literal: opening quote, contents, closing quote.
    Str { open: String, body: String, close: String },
    /// A comment: marker (`//`, `/*`, `#`, …), body, and closing marker.
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

/// Does this look like source code rather than prose? Counts typical
/// signs (braces, semicolons, keywords, operators, indentation).
pub fn is_code(text: &str) -> bool {
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    if lines.is_empty() {
        return false;
    }
    const KEYWORDS: &[&str] = &[
        "fn ", "let ", "const ", "def ", "class ", "import ", "return ", "function ", "var ", "public ",
        "private ", "#include", "using ", "package ", "struct ", "impl ", "if (", "for (", "while (",
        "elif ", "print(", "println!", "console.log", "=>", "==", "!=", "::", "->", "&&", "||",
    ];
    let mut score = 0;
    for l in &lines {
        let t = l.trim_end();
        if t.ends_with(';') || t.ends_with('{') || t.ends_with('}') || t.ends_with(':') && t.trim_start().starts_with(|c: char| c.is_ascii_lowercase()) {
            score += 2;
        }
        if l.starts_with("    ") || l.starts_with('\t') {
            score += 1;
        }
        score += KEYWORDS.iter().filter(|k| t.contains(*k)).count() as i32 * 2;
        if t.trim_start().starts_with("//") || t.trim_start().starts_with("/*") {
            score += 2;
        }
    }
    // Prose has few of these; a short snippet needs more of them per line.
    score as f32 / lines.len() as f32 >= 2.0 && score >= 4
}

/// Splits source code into code, string literals and comments.
pub fn split(src: &str) -> Vec<Piece> {
    let chars: Vec<char> = src.chars().collect();
    let mut out: Vec<Piece> = Vec::new();
    let mut code = String::new();
    let mut i = 0;
    let at = |i: usize, s: &str| s.chars().enumerate().all(|(k, c)| chars.get(i + k) == Some(&c));
    let flush = |code: &mut String, out: &mut Vec<Piece>| {
        if !code.is_empty() {
            out.push(Piece::Code(std::mem::take(code)));
        }
    };
    while i < chars.len() {
        let c = chars[i];
        let line_start = i == 0 || chars[i - 1] == '\n' || code.ends_with(char::is_whitespace);
        // Comments.
        let comment = if at(i, "//") {
            Some(("//", "\n"))
        } else if at(i, "/*") {
            Some(("/*", "*/"))
        } else if at(i, "<!--") {
            Some(("<!--", "-->"))
        } else if c == '#' && line_start && chars.get(i + 1).is_some_and(|n| *n == ' ' || *n == '#') {
            // `# comment` (Python, shell, YAML), not `#include`.
            Some(("#", "\n"))
        } else if at(i, "-- ") && line_start {
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
            // Line comments keep their newline in the code that follows.
            let close_s = if close == "\n" || j >= chars.len() { String::new() } else { close.to_owned() };
            i = if close == "\n" { j } else { (j + close.chars().count()).min(chars.len()) };
            out.push(Piece::Comment { open: open.to_owned(), body, close: close_s });
            continue;
        }
        // Strings: """…""", '''…''', "…", '…', `…`.
        if c == '"' || c == '\'' || c == '`' {
            let triple: String = std::iter::repeat_n(c, 3).collect();
            let quote = if at(i, &triple) { triple } else { c.to_string() };
            // 'x' in C/Rust/Java is a char, and a lone ' is an apostrophe or
            // a lifetime ('a): only treat ' as a string when it closes on the line.
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

/// Prose worth translating inside a string: some letters, and not an
/// identifier, key, path, URL, format string or hex/number.
fn is_prose(s: &str) -> bool {
    let letters = s.chars().filter(|c| c.is_alphabetic()).count();
    if letters < 2 {
        return false;
    }
    if s.contains(char::is_whitespace) {
        return true;
    }
    if !s.is_ascii() {
        return true;
    }
    // One ASCII token: translate "Cancel" or "Settings", not "utf-8",
    // "user_id", "/api/v1", "%Y-%m-%d", "camelCase" or "lowercase_key".
    let symbolic = s.chars().any(|c| "./\\_:%{}=<>[]@#$&?*+".contains(c));
    let starts_upper = s.chars().next().is_some_and(char::is_uppercase);
    let rest_lower = s.chars().skip(1).all(|c| c.is_lowercase() || c == '\'' || c == '-');
    !symbolic && starts_upper && rest_lower && letters >= 3
}

/// Puts the pieces back together, with translated texts for the pieces
/// that have one (`translations` in the order of [`Piece::text`] hits).
pub fn join(pieces: &[Piece], translations: &[String]) -> String {
    let mut next = translations.iter();
    let mut s = String::new();
    for p in pieces {
        match p {
            Piece::Code(c) => s.push_str(c),
            Piece::Str { open, body, close } | Piece::Comment { open, body, close } => {
                s.push_str(open);
                if p.text().is_some() {
                    let t = next.next().map(String::as_str).unwrap_or(body);
                    s.push_str(&keep_edges(body, t, matches!(p, Piece::Str { .. })));
                } else {
                    s.push_str(body);
                }
                s.push_str(close);
            }
        }
    }
    s
}

/// The translation with the original's leading/trailing whitespace, and
/// with quotes escaped for string literals.
fn keep_edges(original: &str, translated: &str, in_string: bool) -> String {
    let lead = &original[..original.len() - original.trim_start().len()];
    let trail = &original[original.trim_end().len()..];
    let mut t = translated.trim().to_owned();
    if in_string {
        t = t.replace('"', "\\\"");
    }
    format!("{lead}{t}{trail}")
}

/// Translates `src` the programmer-mode way when it looks like code
/// (strings and comments only, one request each, from the cache when
/// possible); `None` when it isn't code.
pub fn translate(
    engine: &crate::translate::Engine,
    src: &str,
    from: &str,
    to: &str,
    prefer: Option<crate::settings::ProviderKind>,
) -> Option<Result<std::sync::Arc<crate::translate::Translation>, crate::translate::Error>> {
    if !is_code(src) {
        return None;
    }
    let pieces = split(src);
    let mut out = Vec::new();
    let mut first = None;
    for text in pieces.iter().filter_map(Piece::text) {
        match engine.translate_using(text.trim(), from, to, prefer) {
            Ok(t) => {
                first.get_or_insert_with(|| t.clone());
                out.push(t.text.clone());
            }
            Err(e) => return Some(Err(e)),
        }
    }
    let provider = first.as_ref().map_or(crate::settings::ProviderKind::Google, |t| t.provider);
    Some(Ok(std::sync::Arc::new(crate::translate::Translation {
        source: src.to_owned(),
        text: join(&pieces, &out),
        src_lang: first.and_then(|t| t.src_lang.clone()),
        tgt_lang: to.to_owned(),
        is_word: false,
        dict: Vec::new(),
        provider,
    })))
}

#[cfg(test)]
mod tests {
    use super::*;

    const RUST: &str = r#"fn main() {
    // Greet the user
    let name = "user_id";
    println!("Hello, world! Welcome back.");
    let c = 'x';
}"#;

    #[test]
    fn detects_code_and_prose() {
        assert!(is_code(RUST));
        assert!(is_code("def hello():\n    print(\"Hi there\")\n    return 1\n"));
        assert!(is_code("const x = a == b ? 'yes' : 'no';\nconsole.log(x);"));
        assert!(!is_code("Please send me the report by Friday. Thanks!"));
        assert!(!is_code("Hello,\nI hope you are well.\nBest regards"));
    }

    #[test]
    fn splits_and_rejoins_exactly() {
        let pieces = split(RUST);
        let texts: Vec<&str> = pieces.iter().filter_map(Piece::text).collect();
        assert_eq!(texts, [" Greet the user", "Hello, world! Welcome back."]);
        // Untranslated, the code comes back byte for byte.
        let same: Vec<String> = texts.iter().map(|s| (*s).to_owned()).collect();
        assert_eq!(join(&pieces, &same), RUST);
        let out = join(&pieces, &["Поприветствовать пользователя".into(), "Привет, мир! С \"возвращением\".".into()]);
        assert!(out.contains("// Поприветствовать пользователя\n"));
        assert!(out.contains(r#"println!("Привет, мир! С \"возвращением\".");"#));
        assert!(out.contains(r#"let name = "user_id";"#));
        assert!(out.contains("let c = 'x';"));
    }

    #[test]
    fn python_comments_and_triple_quotes() {
        let src = "#include <stdio.h>\n# A comment\nx = \"\"\"Long text here\"\"\"\n";
        let pieces = split(src);
        let texts: Vec<&str> = pieces.iter().filter_map(Piece::text).collect();
        assert_eq!(texts, [" A comment", "Long text here"]);
        assert_eq!(join(&pieces, &[]), src);
    }

    #[test]
    fn prose_filter() {
        for s in ["Cancel", "Save changes", "Привет", "Don't do that"] {
            assert!(is_prose(s), "{s}");
        }
        for s in ["utf-8", "user_id", "/api/v1", "%Y-%m-%d", "camelCase", "id", "OK1", "lowercase", "a.b"] {
            assert!(!is_prose(s), "{s}");
        }
    }
}
