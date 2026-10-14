//! Office Open XML (DOCX, PPTX, XLSX): the parts with text, their
//! paragraphs, and the parts rebuilt with translations.
//!
//! Every format keeps text the same way — a paragraph element holding text
//! runs: `w:p` / `w:t` in Word, `a:p` / `a:t` in PowerPoint, `si` / `t` in
//! Excel's shared strings. A small scanner finds them without a full XML
//! parser. A translated paragraph goes into its first run (keeping that
//! run's look: font, size, bold…) and the other runs are emptied, so
//! paragraph styles, lists, tables and pictures stay as they were.
//! Paragraphs nested in text boxes are handled as their own paragraphs.

use std::io::{Cursor, Read, Write};

use crate::{Error, Unit};

/// Which tags make paragraphs and text in a package.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Dialect {
    pub para: &'static str,
    pub text: &'static str,
    /// Text inside these isn't shown (Excel's phonetic hints).
    pub skip: &'static [&'static str],
    /// `<w:t>` drops edge spaces without `xml:space="preserve"`.
    pub preserve: bool,
}

pub const WORD: Dialect = Dialect { para: "w:p", text: "w:t", skip: &[], preserve: true };
pub const SLIDES: Dialect = Dialect { para: "a:p", text: "a:t", skip: &[], preserve: false };
pub const SHEETS: Dialect = Dialect { para: "si", text: "t", skip: &["rPh"], preserve: true };
/// Text kept in the cells themselves (LibreOffice, openpyxl): `<is><t>`.
pub const CELLS: Dialect = Dialect { para: "is", text: "t", skip: &["rPh"], preserve: true };

/// One paragraph: its byte range and the content ranges of its text runs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Para {
    pub start: usize,
    pub end: usize,
    /// (open tag start, content start, content end) per text run.
    pub runs: Vec<(usize, usize, usize)>,
    /// Contains other paragraphs (a text box in a run).
    pub nested: bool,
    /// Index into the document's units, when it has text to translate.
    pub unit: Option<usize>,
}

/// Finds the paragraphs of `xml` (innermost first, as they close).
pub fn scan(xml: &str, d: &Dialect) -> Vec<Para> {
    let mut out: Vec<Para> = Vec::new();
    let mut open: Vec<Para> = Vec::new();
    let mut skip = 0usize;
    let mut i = 0;
    while let Some(off) = xml[i..].find('<') {
        let lt = i + off;
        let Some(gt_off) = xml[lt..].find('>') else { break };
        let gt = lt + gt_off;
        i = gt + 1;
        let tag = &xml[lt + 1..gt];
        if tag.starts_with('?') || tag.starts_with('!') {
            // Comments and CDATA can hold '>': skip to their real end.
            if tag.starts_with("!--")
                && let Some(e) = xml[lt..].find("-->")
            {
                i = lt + e + 3;
            }
            continue;
        }
        let closing = tag.starts_with('/');
        let self_closing = tag.ends_with('/');
        let name_part = tag.trim_start_matches('/').trim_end_matches('/');
        let name = name_part.split(|c: char| c.is_whitespace()).next().unwrap_or("");
        if d.skip.contains(&name) {
            if closing {
                skip = skip.saturating_sub(1);
            } else if !self_closing {
                skip += 1;
            }
            continue;
        }
        if name == d.para {
            if closing {
                if let Some(mut p) = open.pop() {
                    p.end = gt + 1;
                    if let Some(parent) = open.last_mut() {
                        parent.nested = true;
                    }
                    out.push(p);
                }
            } else if self_closing {
                // An empty paragraph: nothing to translate.
            } else {
                open.push(Para { start: lt, end: lt, runs: Vec::new(), nested: false, unit: None });
            }
            continue;
        }
        if name == d.text && !closing && !self_closing && skip == 0 {
            let close = format!("</{}>", d.text);
            let Some(c) = xml[i..].find(&close) else { break };
            if let Some(p) = open.last_mut() {
                p.runs.push((lt, i, i + c));
            }
            i += c + close.len();
        }
    }
    out
}

/// The paragraph's visible text.
pub fn text_of(xml: &str, p: &Para) -> String {
    p.runs.iter().map(|&(_, a, b)| unescape(&xml[a..b])).collect()
}

/// `xml` with the translations put in: `tr[unit]` for each paragraph that
/// has a unit. `bilingual`: the original paragraph stays and a translated
/// copy follows it.
pub fn rebuild(xml: &str, paras: &[Para], tr: &[String], d: &Dialect, bilingual: bool) -> String {
    // (position, remove until, insert)
    let mut edits: Vec<(usize, usize, String)> = Vec::new();
    for p in paras {
        let Some(u) = p.unit else { continue };
        let Some(t) = tr.get(u) else { continue };
        if bilingual && !p.nested {
            let local = runs_edits(xml, p, t, d, p.start);
            let copy = apply(&xml[p.start..p.end], local);
            edits.push((p.end, p.end, copy));
        } else {
            edits.extend(runs_edits(xml, p, t, d, 0));
        }
    }
    apply(xml, edits)
}

/// Edits that put `t` into the first run and empty the rest (positions
/// relative to `base`).
fn runs_edits(xml: &str, p: &Para, t: &str, d: &Dialect, base: usize) -> Vec<(usize, usize, String)> {
    let mut edits = Vec::new();
    for (k, &(open, a, b)) in p.runs.iter().enumerate() {
        if k == 0 {
            let open_tag = &xml[open..a];
            if d.preserve && !open_tag.contains("xml:space") {
                // `<w:t>` → `<w:t xml:space="preserve">`.
                let at = a - 1;
                edits.push((at - base, at - base, " xml:space=\"preserve\"".to_owned()));
            }
            edits.push((a - base, b - base, escape(t)));
        } else {
            edits.push((a - base, b - base, String::new()));
        }
    }
    edits
}

fn apply(src: &str, mut edits: Vec<(usize, usize, String)>) -> String {
    edits.sort_by_key(|e| (e.0, e.1));
    let mut out = String::with_capacity(src.len() + edits.iter().map(|e| e.2.len()).sum::<usize>());
    let mut pos = 0;
    for (a, b, ins) in edits {
        if a < pos {
            continue; // overlapping (shouldn't happen): keep the original
        }
        out.push_str(&src[pos..a]);
        out.push_str(&ins);
        pos = b;
    }
    out.push_str(&src[pos..]);
    out
}

pub fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            // Control characters aren't allowed in XML 1.0.
            c if (c as u32) < 0x20 && c != '\t' && c != '\n' && c != '\r' => {}
            c => out.push(c),
        }
    }
    out
}

pub fn unescape(s: &str) -> String {
    if !s.contains('&') {
        return s.to_owned();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let Some(semi) = rest.find(';').filter(|&k| k <= 10) else {
            out.push('&');
            rest = &rest[1..];
            continue;
        };
        let ent = &rest[1..semi];
        let ch = match ent {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            e if e.starts_with("#x") || e.starts_with("#X") => u32::from_str_radix(&e[2..], 16).ok().and_then(char::from_u32),
            e if e.starts_with('#') => e[1..].parse().ok().and_then(char::from_u32),
            _ => None,
        };
        match ch {
            Some(c) => {
                out.push(c);
                rest = &rest[semi + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// A part of the package with text in it.
#[derive(Debug)]
pub struct Part {
    pub name: String,
    pub xml: String,
    pub paras: Vec<Para>,
    pub dialect: Dialect,
}

/// The package: the original zip and the parts with text.
#[derive(Debug)]
pub struct Package {
    pub zip: Vec<u8>,
    pub parts: Vec<Part>,
    /// Pages (Word) or slides (PowerPoint) from docProps/app.xml.
    pub pages: Option<u32>,
}

/// Which parts of a package hold the text worth translating, in reading
/// order, and how each counts for progress (slide number; 0 = no group).
fn text_parts(kind: crate::Kind, names: &[String]) -> Vec<(String, Dialect, u32)> {
    let num = |n: &str, prefix: &str| -> Option<u32> { n.strip_prefix(prefix)?.strip_suffix(".xml")?.parse().ok() };
    let mut out: Vec<(String, Dialect, u32)> = Vec::new();
    match kind {
        crate::Kind::Docx => {
            let mut rest: Vec<&String> = names
                .iter()
                .filter(|n| {
                    n.starts_with("word/header") || n.starts_with("word/footer") || *n == "word/footnotes.xml" || *n == "word/endnotes.xml"
                })
                .filter(|n| n.ends_with(".xml"))
                .collect();
            rest.sort();
            if names.iter().any(|n| n == "word/document.xml") {
                out.push(("word/document.xml".into(), WORD, 0));
            }
            out.extend(rest.into_iter().map(|n| (n.clone(), WORD, 0)));
        }
        crate::Kind::Pptx => {
            let mut slides: Vec<(u32, &String)> = names.iter().filter_map(|n| num(n, "ppt/slides/slide").map(|k| (k, n))).collect();
            slides.sort();
            for (i, (_, n)) in slides.into_iter().enumerate() {
                out.push((n.clone(), SLIDES, i as u32 + 1));
            }
        }
        crate::Kind::Xlsx => {
            if names.iter().any(|n| n == "xl/sharedStrings.xml") {
                out.push(("xl/sharedStrings.xml".into(), SHEETS, 0));
            }
            let mut sheets: Vec<(u32, &String)> =
                names.iter().filter_map(|n| num(n, "xl/worksheets/sheet").map(|k| (k, n))).collect();
            sheets.sort();
            out.extend(sheets.into_iter().map(|(_, n)| (n.clone(), CELLS, 0)));
        }
        _ => {}
    }
    out
}

/// Reads a package and lists its units (`units` gets them appended).
pub fn read(kind: crate::Kind, bytes: &[u8], units: &mut Vec<Unit>) -> Result<Package, Error> {
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|_| Error::Broken)?;
    let names: Vec<String> = zip.file_names().map(str::to_owned).collect();
    let mut parts = Vec::new();
    for (name, dialect, group) in text_parts(kind, &names) {
        let mut xml = String::new();
        zip.by_name(&name).map_err(|_| Error::Broken)?.read_to_string(&mut xml).map_err(|_| Error::Broken)?;
        let mut paras = scan(&xml, &dialect);
        // Reading order: paragraphs close innermost-first; sort by start.
        paras.sort_by_key(|p| p.start);
        for p in &mut paras {
            let text = text_of(&xml, p);
            if crate::worth_translating(&text) {
                p.unit = Some(units.len());
                units.push(Unit { text: crate::one_line(&text), group });
            }
        }
        parts.push(Part { name, xml, paras, dialect });
    }
    let pages = zip.by_name("docProps/app.xml").ok().and_then(|mut f| {
        let mut s = String::new();
        f.read_to_string(&mut s).ok()?;
        let tag = if kind == crate::Kind::Pptx { "Slides" } else { "Pages" };
        let a = s.find(&format!("<{tag}>"))? + tag.len() + 2;
        let b = a + s[a..].find('<')?;
        s[a..b].trim().parse().ok().filter(|&n: &u32| n > 0)
    });
    Ok(Package { zip: bytes.to_vec(), parts, pages })
}

/// The package with every text part rebuilt; everything else is copied
/// byte for byte.
pub fn write(pkg: &Package, tr: &[String], bilingual: bool) -> Result<Vec<u8>, Error> {
    let mut src = zip::ZipArchive::new(Cursor::new(&pkg.zip)).map_err(|_| Error::Broken)?;
    let mut out = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for i in 0..src.len() {
        let name = src.by_index_raw(i).map_err(|_| Error::Broken)?.name().to_owned();
        if let Some(part) = pkg.parts.iter().find(|p| p.name == name) {
            // Sheets: a translated copy in the same cell would be odd.
            let bi = bilingual && part.dialect != SHEETS && part.dialect != CELLS;
            let xml = rebuild(&part.xml, &part.paras, tr, &part.dialect, bi);
            out.start_file(name, opts).map_err(|_| Error::Write)?;
            out.write_all(xml.as_bytes()).map_err(|_| Error::Write)?;
        } else {
            let f = src.by_index_raw(i).map_err(|_| Error::Broken)?;
            out.raw_copy_file(f).map_err(|_| Error::Write)?;
        }
    }
    Ok(out.finish().map_err(|_| Error::Write)?.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOC: &str = r#"<w:body><w:p><w:pPr><w:pStyle w:val="Title"/></w:pPr><w:r><w:rPr><w:b/></w:rPr><w:t>Hello</w:t></w:r><w:r><w:t xml:space="preserve"> big &amp; wide</w:t></w:r><w:r><w:t> world</w:t></w:r></w:p><w:p/><w:p><w:r><w:t>2024</w:t></w:r></w:p></w:body>"#;

    #[test]
    fn finds_paragraph_text() {
        let ps = scan(DOC, &WORD);
        assert_eq!(ps.len(), 2);
        assert_eq!(text_of(DOC, &ps[0]), "Hello big & wide world");
        assert_eq!(text_of(DOC, &ps[1]), "2024");
    }

    #[test]
    fn translation_goes_into_the_first_run() {
        let mut ps = scan(DOC, &WORD);
        ps[0].unit = Some(0);
        let out = rebuild(DOC, &ps, &["Привет, мир & всё".into()], &WORD, false);
        assert!(out.contains(r#"<w:t xml:space="preserve">Привет, мир &amp; всё</w:t>"#), "{out}");
        assert!(out.contains(r#"<w:t xml:space="preserve"></w:t>"#));
        assert!(out.contains("<w:t></w:t>"));
        // Style and bold kept.
        assert!(out.contains(r#"<w:pStyle w:val="Title"/>"#) && out.contains("<w:b/>"));
        assert_eq!(text_of(&out, &scan(&out, &WORD)[0]), "Привет, мир & всё");
    }

    #[test]
    fn bilingual_adds_a_copy_after_the_original() {
        let mut ps = scan(DOC, &WORD);
        ps[0].unit = Some(0);
        let out = rebuild(DOC, &ps, &["Привет".into()], &WORD, true);
        let texts: Vec<String> = scan(&out, &WORD).iter().map(|p| text_of(&out, p)).collect();
        assert_eq!(texts, ["Hello big & wide world", "Привет", "2024"]);
    }

    #[test]
    fn nested_text_boxes_are_their_own_paragraphs() {
        let xml = r#"<w:p><w:r><w:t>Outer</w:t></w:r><w:r><w:txbxContent><w:p><w:r><w:t>Inner</w:t></w:r></w:p></w:txbxContent></w:r></w:p>"#;
        let mut ps = scan(xml, &WORD);
        ps.sort_by_key(|p| p.start);
        assert_eq!(ps.iter().map(|p| text_of(xml, p)).collect::<Vec<_>>(), ["Outer", "Inner"]);
        assert!(ps[0].nested && !ps[1].nested);
        ps[0].unit = Some(0);
        ps[1].unit = Some(1);
        let out = rebuild(xml, &ps, &["Внешний".into(), "Внутренний".into()], &WORD, false);
        assert!(out.contains(">Внешний<") && out.contains(">Внутренний<"), "{out}");
    }

    #[test]
    fn sheets_skip_phonetic_hints() {
        let xml = r#"<sst><si><t>Name</t></si><si><r><t>Total</t></r><rPh><t>とうたる</t></rPh></si></sst>"#;
        let ps = scan(xml, &SHEETS);
        assert_eq!(ps.iter().map(|p| text_of(xml, p)).collect::<Vec<_>>(), ["Name", "Total"]);
    }

    #[test]
    fn entities() {
        assert_eq!(unescape("a &lt;b&gt; &#1055;&#x438; &amp;amp; &bogus;"), "a <b> Пи &amp; &bogus;");
        assert_eq!(escape("<a & \"b\">"), "&lt;a &amp; &quot;b&quot;&gt;");
    }
}
