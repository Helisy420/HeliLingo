//! Text from a screenshot with Granite Vision (offline, optional). Windows
//! OCR stays the default for the image tab and the screen-area capture;
//! Granite is used when the user picks it in Settings → Офлайн. It was
//! trained on English instructions: it reads Latin script best.

use hl_core::EngineError;

use crate::{Config, Model};

const PROMPT: &str = "Transcribe all text visible in this image exactly as written, in reading order. \
Keep line breaks between lines and blank lines between separate blocks. Do not translate, describe or \
comment. Output only the text.";

/// The text in an encoded image (PNG/JPEG/BMP bytes).
pub async fn read_text(config: &Config, image: &[u8]) -> Result<String, EngineError> {
    if !config.installed(Model::Granite) {
        return Err(EngineError::Other("model not installed".into()));
    }
    let mime = mime(image);
    let text = crate::llama::ask_image(config, Model::Granite, image, mime, PROMPT).await?;
    Ok(strip_fence(&text))
}

/// Width and height of a PNG or JPEG, from its header.
pub fn image_size(image: &[u8]) -> Option<(u32, u32)> {
    if image.starts_with(&[0x89, b'P', b'N', b'G']) && image.len() >= 24 {
        let be = |i: usize| u32::from_be_bytes([image[i], image[i + 1], image[i + 2], image[i + 3]]);
        return Some((be(16), be(20)));
    }
    if image.starts_with(&[0xFF, 0xD8]) {
        // Walk the segments to a start-of-frame marker.
        let mut i = 2;
        while i + 9 < image.len() {
            if image[i] != 0xFF {
                return None;
            }
            let marker = image[i + 1];
            let len = u16::from_be_bytes([image[i + 2], image[i + 3]]) as usize;
            if matches!(marker, 0xC0..=0xC3 | 0xC5..=0xC7 | 0xC9..=0xCB | 0xCD..=0xCF) {
                let h = u16::from_be_bytes([image[i + 5], image[i + 6]]) as u32;
                let w = u16::from_be_bytes([image[i + 7], image[i + 8]]) as u32;
                return Some((w, h));
            }
            i += 2 + len;
        }
    }
    None
}

/// The image type from its first bytes.
fn mime(image: &[u8]) -> &'static str {
    match image {
        [0x89, b'P', b'N', b'G', ..] => "image/png",
        [0xFF, 0xD8, ..] => "image/jpeg",
        [b'R', b'I', b'F', b'F', _, _, _, _, b'W', b'E', b'B', b'P', ..] => "image/webp",
        [b'B', b'M', ..] => "image/bmp",
        _ => "image/png",
    }
}

/// Models sometimes wrap the answer in a code fence.
fn strip_fence(s: &str) -> String {
    let t = s.trim();
    if let Some(inner) = t.strip_prefix("```").and_then(|r| r.strip_suffix("```")) {
        let inner = inner.split_once('\n').map_or(inner, |(first, rest)| if first.contains(' ') { inner } else { rest });
        return inner.trim().to_owned();
    }
    t.to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_types_and_fences() {
        assert_eq!(mime(&[0x89, b'P', b'N', b'G', 0, 0]), "image/png");
        assert_eq!(mime(&[0xFF, 0xD8, 0xFF]), "image/jpeg");
        assert_eq!(strip_fence("```text\nHello\nWorld\n```"), "Hello\nWorld");
        assert_eq!(strip_fence("  Hello  "), "Hello");
        let mut png = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 13, b'I', b'H', b'D', b'R'];
        png.extend_from_slice(&640u32.to_be_bytes());
        png.extend_from_slice(&480u32.to_be_bytes());
        assert_eq!(image_size(&png), Some((640, 480)));
        let jpeg = [0xFF, 0xD8, 0xFF, 0xE0, 0, 4, 0, 0, 0xFF, 0xC0, 0, 17, 8, 1, 0xE0, 2, 0x80, 3, 0, 0];
        assert_eq!(image_size(&jpeg), Some((640, 480)));
    }
}
