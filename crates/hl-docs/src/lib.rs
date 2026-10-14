//! Documents for «Перевод файлов»: a file's text as units (paragraphs,
//! slide texts, cells, subtitle cues), and the file rebuilt with their
//! translations. No network and no Tauri here — the app translates the
//! units with the user's engines and hands the translations back.
//!
//! - DOCX / PPTX / XLSX: the package is copied byte for byte except the
//!   parts with text, where each paragraph's translation goes into its
//!   first run (see `ooxml`).
//! - TXT: line by line. SRT: cue by cue, numbers and timings untouched.
//! - PDF is read by the app (pages rendered and recognised, see
//!   `hl_win::pdf`) and arrives here as plain units: [`Doc::from_units`].

mod ooxml;

use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Docx,
    Pptx,
    Xlsx,
    Txt,
    Srt,
    Pdf,
}

impl Kind {
    pub fn from_path(path: &Path) -> Option<Kind> {
        let ext = path.extension()?.to_str()?.to_ascii_lowercase();
        Some(match ext.as_str() {
            "docx" => Kind::Docx,
            "pptx" => Kind::Pptx,
            "xlsx" => Kind::Xlsx,
            "txt" | "md" => Kind::Txt,
            "srt" => Kind::Srt,
            "pdf" => Kind::Pdf,
            _ => return None,
        })
    }

    pub fn label(self) -> &'static str {
        match self {
            Kind::Docx => "DOCX",
            Kind::Pptx => "PPTX",
            Kind::Xlsx => "XLSX",
            Kind::Txt => "TXT",
            Kind::Srt => "SRT",
            Kind::Pdf => "PDF",
        }
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum Error {
    #[error("the file is damaged or not what its name says")]
    Broken,
    #[error("the translated file couldn't be written")]
    Write,
}

/// One piece of text to translate. Never contains a line break.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unit {
    pub text: String,
    /// Slide or page number (1-based) for progress; 0 = none.
    pub group: u32,
}

/// What the translated file becomes («Формат результата»).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Output {
    /// The original's format (a PDF becomes DOCX: its layout can't be rebuilt).
    Same,
    Docx,
    Txt,
}

#[derive(Clone, Copy, Debug)]
pub struct WriteOptions {
    pub output: Output,
    /// «Сохранять форматирование»: off = a plain document with the text.
    pub keep_formatting: bool,
    /// «Двуязычный документ»: original and translation, unit by unit.
    pub bilingual: bool,
}

#[derive(Debug)]
enum Inner {
    Package(ooxml::Package),
    /// Lines; `units[i]` is line `line_of[i]`.
    Text { lines: Vec<String>, line_of: Vec<usize> },
    /// Cues: header lines (number, timing) and text lines.
    Srt { cues: Vec<(Vec<String>, Vec<String>)>, cue_of: Vec<usize> },
    /// Units only (PDF).
    Plain,
}

/// A read document.
#[derive(Debug)]
pub struct Doc {
    pub kind: Kind,
    pub units: Vec<Unit>,
    /// Pages (DOCX) or slides (PPTX) when the file says.
    pub pages: Option<u32>,
    inner: Inner,
}

/// Has letters: numbers, dates and punctuation stay as they are.
pub(crate) fn worth_translating(s: &str) -> bool {
    s.chars().any(char::is_alphabetic)
}

/// Line breaks inside a unit become spaces (batches are split by lines).
pub(crate) fn one_line(s: &str) -> String {
    s.split(['\n', '\r', '\u{2028}']).map(str::trim_end).collect::<Vec<_>>().join(" ").trim().to_owned()
}

/// Text from bytes: UTF-8 (with or without BOM) or UTF-16 with a BOM.
fn decode(bytes: &[u8]) -> String {
    if let Some(b) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        let w: Vec<u16> = b.as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes(*c)).collect();
        return String::from_utf16_lossy(&w);
    }
    if let Some(b) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        let w: Vec<u16> = b.as_chunks::<2>().0.iter().map(|c| u16::from_be_bytes(*c)).collect();
        return String::from_utf16_lossy(&w);
    }
    let b = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    String::from_utf8_lossy(b).into_owned()
}

impl Doc {
    pub fn read(kind: Kind, bytes: &[u8]) -> Result<Doc, Error> {
        let mut units = Vec::new();
        let (inner, pages) = match kind {
            Kind::Docx | Kind::Pptx | Kind::Xlsx => {
                let pkg = ooxml::read(kind, bytes, &mut units)?;
                let pages = pkg.pages;
                (Inner::Package(pkg), pages)
            }
            Kind::Txt => {
                let text = decode(bytes);
                let lines: Vec<String> = text.replace("\r\n", "\n").split('\n').map(str::to_owned).collect();
                let mut line_of = Vec::new();
                for (i, l) in lines.iter().enumerate() {
                    if worth_translating(l) {
                        line_of.push(i);
                        units.push(Unit { text: l.trim().to_owned(), group: 0 });
                    }
                }
                (Inner::Text { lines, line_of }, None)
            }
            Kind::Srt => {
                let text = decode(bytes).replace("\r\n", "\n");
                let mut cues = Vec::new();
                let mut cue_of = Vec::new();
                for block in text.split("\n\n").filter(|b| !b.trim().is_empty()) {
                    let lines: Vec<&str> = block.lines().collect();
                    // Number and timing, then the text.
                    let head_len = lines.iter().position(|l| l.contains("-->")).map_or(0, |p| p + 1);
                    let head: Vec<String> = lines[..head_len].iter().map(|s| s.to_string()).collect();
                    let body: Vec<String> = lines[head_len..].iter().map(|s| s.to_string()).collect();
                    let joined = one_line(&body.join("\n"));
                    if worth_translating(&joined) {
                        cue_of.push(cues.len());
                        units.push(Unit { text: joined, group: 0 });
                    }
                    cues.push((head, body));
                }
                (Inner::Srt { cues, cue_of }, None)
            }
            Kind::Pdf => (Inner::Plain, None),
        };
        Ok(Doc { kind, units, pages, inner })
    }

    /// A document known only by its units (a recognised PDF).
    pub fn from_units(kind: Kind, units: Vec<Unit>, pages: Option<u32>) -> Doc {
        Doc { kind, units, pages, inner: Inner::Plain }
    }

    /// The file with `tr` (one translation per unit) put in, and its
    /// extension.
    pub fn write(&self, tr: &[String], o: &WriteOptions) -> Result<(Vec<u8>, &'static str), Error> {
        let pairs = || self.units.iter().zip(tr).map(|(u, t)| (u.text.as_str(), t.as_str()));
        match (o.output, &self.inner) {
            (Output::Txt, _) => Ok((plain_text(pairs(), o.bilingual).into_bytes(), "txt")),
            (Output::Docx, Inner::Package(_)) if self.kind == Kind::Docx && o.keep_formatting => self.write_same(tr, o),
            (Output::Docx, _) => Ok((plain_docx(pairs(), o.bilingual)?, "docx")),
            (Output::Same, Inner::Package(_)) if !o.keep_formatting && self.kind == Kind::Docx => {
                Ok((plain_docx(pairs(), o.bilingual)?, "docx"))
            }
            (Output::Same, Inner::Plain) => Ok((plain_docx(pairs(), o.bilingual)?, "docx")),
            (Output::Same, _) => self.write_same(tr, o),
        }
    }

    fn write_same(&self, tr: &[String], o: &WriteOptions) -> Result<(Vec<u8>, &'static str), Error> {
        match &self.inner {
            Inner::Package(pkg) => {
                let ext = match self.kind {
                    Kind::Pptx => "pptx",
                    Kind::Xlsx => "xlsx",
                    _ => "docx",
                };
                Ok((ooxml::write(pkg, tr, o.bilingual)?, ext))
            }
            Inner::Text { lines, line_of } => {
                let mut out: Vec<String> = Vec::with_capacity(lines.len());
                let mut k = 0;
                for (i, l) in lines.iter().enumerate() {
                    if line_of.get(k) == Some(&i) {
                        let t = tr.get(k).map_or(l.as_str(), String::as_str);
                        // Keep the line's indent.
                        let indent: String = l.chars().take_while(|c| c.is_whitespace()).collect();
                        if o.bilingual {
                            out.push(l.clone());
                        }
                        out.push(format!("{indent}{t}"));
                        k += 1;
                    } else {
                        out.push(l.clone());
                    }
                }
                Ok((out.join("\r\n").into_bytes(), "txt"))
            }
            Inner::Srt { cues, cue_of } => {
                let mut out = String::new();
                let mut k = 0;
                for (i, (head, body)) in cues.iter().enumerate() {
                    for h in head {
                        out.push_str(h);
                        out.push_str("\r\n");
                    }
                    if cue_of.get(k) == Some(&i) {
                        if o.bilingual {
                            for b in body {
                                out.push_str(b);
                                out.push_str("\r\n");
                            }
                        }
                        out.push_str(tr.get(k).map_or("", String::as_str));
                        out.push_str("\r\n");
                        k += 1;
                    } else {
                        for b in body {
                            out.push_str(b);
                            out.push_str("\r\n");
                        }
                    }
                    out.push_str("\r\n");
                }
                Ok((out.into_bytes(), "srt"))
            }
            Inner::Plain => Ok((plain_docx(self.units.iter().zip(tr).map(|(u, t)| (u.text.as_str(), t.as_str())), o.bilingual)?, "docx")),
        }
    }
}

fn plain_text<'a>(pairs: impl Iterator<Item = (&'a str, &'a str)>, bilingual: bool) -> String {
    let mut out = String::new();
    for (src, tr) in pairs {
        if bilingual {
            out.push_str(src);
            out.push_str("\r\n");
        }
        out.push_str(tr);
        out.push_str(if bilingual { "\r\n\r\n" } else { "\r\n" });
    }
    out
}

/// A minimal Word document: one paragraph per unit (the original in grey
/// before it when bilingual).
fn plain_docx<'a>(pairs: impl Iterator<Item = (&'a str, &'a str)>, bilingual: bool) -> Result<Vec<u8>, Error> {
    use std::io::Write;
    let para = |t: &str, grey: bool| {
        let rpr = if grey { "<w:rPr><w:color w:val=\"7F7F7F\"/></w:rPr>" } else { "" };
        format!("<w:p><w:r>{rpr}<w:t xml:space=\"preserve\">{}</w:t></w:r></w:p>", ooxml::escape(t))
    };
    let mut body = String::new();
    for (src, tr) in pairs {
        if bilingual {
            body.push_str(&para(src, true));
        }
        body.push_str(&para(tr, false));
    }
    let document = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
<w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:body>{body}<w:sectPr/></w:body></w:document>"
    );
    let content_types = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\">\
<Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/>\
<Default Extension=\"xml\" ContentType=\"application/xml\"/>\
<Override PartName=\"/word/document.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml\"/>\
</Types>";
    let rels = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">\
<Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"word/document.xml\"/>\
</Relationships>";
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for (name, data) in [("[Content_Types].xml", content_types), ("_rels/.rels", rels), ("word/document.xml", document.as_str())] {
        zip.start_file(name, opts).map_err(|_| Error::Write)?;
        zip.write_all(data.as_bytes()).map_err(|_| Error::Write)?;
    }
    Ok(zip.finish().map_err(|_| Error::Write)?.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAME: WriteOptions = WriteOptions { output: Output::Same, keep_formatting: true, bilingual: false };

    fn docx(document: &str) -> Vec<u8> {
        use std::io::Write;
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let o = zip::write::SimpleFileOptions::default();
        zip.start_file("word/document.xml", o).unwrap();
        zip.write_all(document.as_bytes()).unwrap();
        zip.start_file("word/media/image1.png", o).unwrap();
        zip.write_all(b"\x89PNG not really").unwrap();
        zip.start_file("docProps/app.xml", o).unwrap();
        zip.write_all(b"<Properties><Pages>12</Pages></Properties>").unwrap();
        zip.finish().unwrap().into_inner()
    }

    #[test]
    fn docx_round_trip_keeps_other_parts() {
        let bytes = docx("<w:document><w:body><w:p><w:r><w:t>Supply contract</w:t></w:r></w:p><w:p><w:r><w:t>15.05.2026</w:t></w:r></w:p></w:body></w:document>");
        let doc = Doc::read(Kind::Docx, &bytes).unwrap();
        assert_eq!(doc.units.iter().map(|u| u.text.as_str()).collect::<Vec<_>>(), ["Supply contract"]);
        assert_eq!(doc.pages, Some(12));
        let (out, ext) = doc.write(&["Договор поставки".into()], &SAME).unwrap();
        assert_eq!(ext, "docx");
        let again = Doc::read(Kind::Docx, &out).unwrap();
        assert_eq!(again.units[0].text, "Договор поставки");
        let mut z = zip::ZipArchive::new(std::io::Cursor::new(out)).unwrap();
        let mut png = Vec::new();
        std::io::Read::read_to_end(&mut z.by_name("word/media/image1.png").unwrap(), &mut png).unwrap();
        assert_eq!(png, b"\x89PNG not really");
    }

    #[test]
    fn plain_docx_reads_back() {
        let doc = Doc::from_units(Kind::Pdf, vec![Unit { text: "Invoice".into(), group: 1 }], Some(1));
        let (out, ext) = doc.write(&["Счёт".into()], &WriteOptions { bilingual: true, ..SAME }).unwrap();
        assert_eq!(ext, "docx");
        let back = Doc::read(Kind::Docx, &out).unwrap();
        assert_eq!(back.units.iter().map(|u| u.text.as_str()).collect::<Vec<_>>(), ["Invoice", "Счёт"]);
    }

    #[test]
    fn text_keeps_empty_lines_and_indents() {
        let doc = Doc::read(Kind::Txt, "\u{feff}Hello\r\n\r\n  World\r\n42".as_bytes()).unwrap();
        assert_eq!(doc.units.len(), 2);
        let (out, _) = doc.write(&["Привет".into(), "Мир".into()], &SAME).unwrap();
        assert_eq!(String::from_utf8(out).unwrap(), "Привет\r\n\r\n  Мир\r\n42");
    }

    #[test]
    fn subtitles_keep_numbers_and_timings() {
        let srt = "1\n00:00:01,000 --> 00:00:02,000\nHello\nthere\n\n2\n00:00:03,000 --> 00:00:04,000\n♪\n";
        let doc = Doc::read(Kind::Srt, srt.as_bytes()).unwrap();
        assert_eq!(doc.units[0].text, "Hello there");
        let (out, ext) = doc.write(&["Привет".into()], &SAME).unwrap();
        assert_eq!(ext, "srt");
        assert_eq!(
            String::from_utf8(out).unwrap(),
            "1\r\n00:00:01,000 --> 00:00:02,000\r\nПривет\r\n\r\n2\r\n00:00:03,000 --> 00:00:04,000\r\n♪\r\n\r\n"
        );
    }

    #[test]
    fn kinds_by_extension() {
        assert_eq!(Kind::from_path(Path::new("a/Договор.DOCX")), Some(Kind::Docx));
        assert_eq!(Kind::from_path(Path::new("x.srt")), Some(Kind::Srt));
        assert_eq!(Kind::from_path(Path::new("x.exe")), None);
    }
}
