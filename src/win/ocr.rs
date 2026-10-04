//! Text recognition with Windows.Media.Ocr (the OCR built into Windows;
//! one recogniser per installed language pack). Words are grouped into
//! lines and the lines into blocks (a sign, a paragraph, a button label),
//! which are translated one by one.

use windows::Globalization::Language;
use windows::Media::Ocr::OcrEngine;
use windows::core::HSTRING;

use super::image::{RgbaImage, ensure_mta, software_bitmap};

/// A rectangle in image pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bounds {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Bounds {
    pub fn right(&self) -> f32 {
        self.x + self.w
    }
    pub fn bottom(&self) -> f32 {
        self.y + self.h
    }
    pub fn union(&self, o: &Bounds) -> Bounds {
        let (x, y) = (self.x.min(o.x), self.y.min(o.y));
        Bounds { x, y, w: self.right().max(o.right()) - x, h: self.bottom().max(o.bottom()) - y }
    }
}

/// A run of words on one line (no big gaps between them).
#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    pub text: String,
    pub bounds: Bounds,
}

/// Lines that belong together.
#[derive(Clone, Debug, PartialEq)]
pub struct Block {
    pub text: String,
    pub bounds: Bounds,
    pub lines: usize,
    /// Average height of its lines (≈ the text size).
    pub line_height: f32,
}

pub struct Recognized {
    pub blocks: Vec<Block>,
    /// BCP-47 tag of the recogniser that was used, e.g. "en-US".
    pub language: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OcrError {
    /// No recogniser for this language (code) or, with `None`, for any of
    /// the user's languages: a Windows language pack is missing.
    NoLanguage(Option<String>),
    Failed(String),
}

/// Recognises the text in `img`. `lang` = the chosen source language, or
/// `None` for the user's profile languages.
pub fn recognize(img: &RgbaImage, lang: Option<&str>) -> Result<Recognized, OcrError> {
    ensure_mta();
    let engine = match lang {
        Some(code) => {
            let base = code.split('-').next().unwrap_or(code);
            let language = Language::CreateLanguage(&HSTRING::from(base))
                .map_err(|_| OcrError::NoLanguage(Some(code.to_owned())))?;
            if !OcrEngine::IsLanguageSupported(&language).unwrap_or(false) {
                return Err(OcrError::NoLanguage(Some(code.to_owned())));
            }
            OcrEngine::TryCreateFromLanguage(&language)
                .map_err(|_| OcrError::NoLanguage(Some(code.to_owned())))?
        }
        None => OcrEngine::TryCreateFromUserProfileLanguages().map_err(|_| OcrError::NoLanguage(None))?,
    };
    let language = engine
        .RecognizerLanguage()
        .and_then(|l| l.LanguageTag())
        .map(|t| t.to_string())
        .unwrap_or_default();

    // OCR has a size limit, and small text (a screen crop) reads better
    // enlarged.
    let max = OcrEngine::MaxImageDimension().unwrap_or(2600) as f32;
    let side = img.width.max(img.height) as f32;
    let scale = if side > max {
        max / side
    } else if side < 700.0 {
        (1400.0 / side).min(3.0).min(max / side)
    } else {
        1.0
    };
    let scaled;
    let input = if (scale - 1.0).abs() > 0.01 {
        scaled = img.resized(
            (img.width as f32 * scale).round() as u32,
            (img.height as f32 * scale).round() as u32,
        );
        &scaled
    } else {
        img
    };

    let failed = |e: windows::core::Error| OcrError::Failed(e.message());
    let bitmap = software_bitmap(input).map_err(failed)?;
    let result = engine.RecognizeAsync(&bitmap).map_err(failed)?.join().map_err(failed)?;
    let joiner = if matches!(language.split('-').next(), Some("zh" | "ja")) { "" } else { " " };
    let mut lines = Vec::new();
    for line in result.Lines().map_err(failed)? {
        let mut words = Vec::new();
        for word in line.Words().map_err(failed)? {
            let r = word.BoundingRect().map_err(failed)?;
            let text = word.Text().map_err(failed)?.to_string();
            let b = Bounds { x: r.X / scale, y: r.Y / scale, w: r.Width / scale, h: r.Height / scale };
            words.push((text, b));
        }
        lines.extend(split_line(&words, joiner));
    }
    Ok(Recognized { blocks: group_lines(&lines, joiner), language })
}

/// Splits a recognised line where the words are far apart (two columns,
/// a label and a value), so each part is translated and placed on its own.
pub fn split_line(words: &[(String, Bounds)], joiner: &str) -> Vec<Line> {
    let mut out: Vec<Line> = Vec::new();
    let height = words.iter().map(|(_, b)| b.h).fold(0.0, f32::max);
    let mut prev_right = f32::NEG_INFINITY;
    for (text, b) in words {
        let gap = b.x - prev_right;
        match out.last_mut() {
            Some(line) if gap < height * 1.6 => {
                line.text.push_str(joiner);
                line.text.push_str(text);
                line.bounds = line.bounds.union(b);
            }
            _ => out.push(Line { text: text.clone(), bounds: *b }),
        }
        prev_right = b.right();
    }
    out
}

/// Groups lines (in reading order) into blocks: a line joins a block when
/// it starts right under the block's last line, has a similar height and
/// overlaps it horizontally.
pub fn group_lines(lines: &[Line], joiner: &str) -> Vec<Block> {
    struct Acc {
        last: Bounds,
        bounds: Bounds,
        texts: Vec<String>,
        heights: f32,
    }
    let mut blocks: Vec<Acc> = Vec::new();
    for line in lines {
        let b = line.bounds;
        let fits = |acc: &Acc| {
            let h = acc.last.h.max(b.h);
            let gap = b.y - acc.last.bottom();
            let similar = acc.last.h.min(b.h) / h > 0.65;
            let overlap = b.x < acc.bounds.right() && b.right() > acc.bounds.x;
            gap > -0.4 * h && gap < 0.9 * h && similar && overlap
        };
        match blocks.iter_mut().rev().take(4).find(|a| fits(a)) {
            Some(acc) => {
                acc.last = b;
                acc.bounds = acc.bounds.union(&b);
                acc.texts.push(line.text.clone());
                acc.heights += b.h;
            }
            None => blocks.push(Acc { last: b, bounds: b, texts: vec![line.text.clone()], heights: b.h }),
        }
    }
    blocks
        .into_iter()
        .map(|acc| {
            let mut text = String::new();
            for t in &acc.texts {
                if text.ends_with('-') && joiner == " " {
                    // A word broken across lines: "transla-" + "tion".
                    text.pop();
                } else if !text.is_empty() {
                    text.push_str(joiner);
                }
                text.push_str(t);
            }
            Block {
                text,
                bounds: acc.bounds,
                lines: acc.texts.len(),
                line_height: acc.heights / acc.texts.len() as f32,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn b(x: f32, y: f32, w: f32, h: f32) -> Bounds {
        Bounds { x, y, w, h }
    }

    #[test]
    fn far_apart_words_split() {
        let words = vec![
            ("Name".to_owned(), b(0.0, 0.0, 40.0, 10.0)),
            ("Total".to_owned(), b(200.0, 0.0, 40.0, 10.0)),
            ("price".to_owned(), b(245.0, 0.0, 40.0, 10.0)),
        ];
        let lines = split_line(&words, " ");
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[1].text, "Total price");
        assert_eq!(lines[1].bounds, b(200.0, 0.0, 85.0, 10.0));
    }

    #[test]
    fn paragraph_lines_group_and_sign_lines_dont() {
        let line = |t: &str, bounds| Line { text: t.into(), bounds };
        let lines = vec![
            line("OPEN", b(60.0, 0.0, 120.0, 40.0)),
            line("Mon–Fri 9:00–18:00", b(30.0, 60.0, 180.0, 18.0)),
            line("The quick brown fox jumps transla-", b(0.0, 120.0, 300.0, 12.0)),
            line("tion over the lazy dog", b(0.0, 135.0, 200.0, 12.0)),
        ];
        let blocks = group_lines(&lines, " ");
        assert_eq!(blocks.len(), 3);
        assert_eq!(blocks[2].text, "The quick brown fox jumps translation over the lazy dog");
        assert_eq!(blocks[2].lines, 2);
        assert_eq!(blocks[2].bounds, b(0.0, 120.0, 300.0, 27.0));
    }
}
