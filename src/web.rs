//! Links that open in the browser: "Open in Google Translate" (the
//! workaround when the translation service is busy) and URL encoding for
//! the Wikipedia client.

/// Percent-encodes `s` for a URL query value or path segment (RFC 3986
/// unreserved characters stay as they are; UTF-8 bytes are escaped).
pub fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Longest text put into a Google Translate link, in characters (the web
/// page accepts 5000; long Cyrillic text triples in size when encoded, and
/// very long URLs get cut by some browsers).
const MAX_LINK_CHARS: usize = 1800;

/// `https://translate.google.com/?sl=<from>&tl=<to>&text=<text>&op=translate`.
/// `from` is a language code or "auto".
pub fn google_translate_url(text: &str, from: &str, to: &str) -> String {
    let text: String = text.trim().chars().take(MAX_LINK_CHARS).collect();
    let from = if from.is_empty() { "auto" } else { from };
    format!(
        "https://translate.google.com/?sl={}&tl={}&text={}&op=translate",
        percent_encode(from),
        percent_encode(to),
        percent_encode(&text)
    )
}

/// Opens the text in Google Translate in the default browser.
pub fn open_google_translate(text: &str, from: &str, to: &str) {
    crate::win::open_url(&google_translate_url(text, from, to));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_utf8_and_reserved() {
        assert_eq!(percent_encode("a b&c"), "a%20b%26c");
        assert_eq!(percent_encode("ё"), "%D1%91");
        assert_eq!(percent_encode("zh-CN_x.~"), "zh-CN_x.~");
    }

    #[test]
    fn google_link() {
        assert_eq!(
            google_translate_url(" Where is it? ", "auto", "ru"),
            "https://translate.google.com/?sl=auto&tl=ru&text=Where%20is%20it%3F&op=translate"
        );
        assert!(google_translate_url("x", "", "zh-CN").contains("sl=auto&tl=zh-CN"));
        let long = "я".repeat(5000);
        assert_eq!(google_translate_url(&long, "ru", "en").matches("%D1%8F").count(), MAX_LINK_CHARS);
    }
}
