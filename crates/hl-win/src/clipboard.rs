//! Clipboard text, and a snapshot of the user's clipboard so a synthetic
//! paste can put it back afterwards.

use std::time::Duration;

use windows::Win32::Foundation::{GlobalFree, HANDLE, HGLOBAL};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, GetClipboardSequenceNumber, IsClipboardFormatAvailable,
    OpenClipboard, RegisterClipboardFormatW, SetClipboardData,
};
use windows::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock};
use windows::Win32::System::Ole::{CF_DIB, CF_DIBV5, CF_HDROP, CF_UNICODETEXT};
use windows::core::{HSTRING, PCWSTR};

struct Open;

impl Drop for Open {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseClipboard();
        }
    }
}

/// Another process may hold the clipboard for a few ms; retry briefly.
fn open() -> Option<Open> {
    for _ in 0..25 {
        if unsafe { OpenClipboard(None) }.is_ok() {
            return Some(Open);
        }
        std::thread::sleep(Duration::from_millis(4));
    }
    None
}

fn register(name: &str) -> u32 {
    unsafe { RegisterClipboardFormatW(PCWSTR(HSTRING::from(name).as_ptr())) }
}

fn read_format(format: u32) -> Option<Vec<u8>> {
    unsafe {
        let handle = GetClipboardData(format).ok()?;
        let hg = HGLOBAL(handle.0);
        let ptr = GlobalLock(hg) as *const u8;
        if ptr.is_null() {
            return None;
        }
        let bytes = std::slice::from_raw_parts(ptr, GlobalSize(hg)).to_vec();
        let _ = GlobalUnlock(hg);
        Some(bytes)
    }
}

fn write_format(format: u32, bytes: &[u8]) -> bool {
    unsafe {
        let Ok(hg) = GlobalAlloc(GMEM_MOVEABLE, bytes.len().max(1)) else {
            return false;
        };
        let ptr = GlobalLock(hg) as *mut u8;
        if ptr.is_null() {
            let _ = GlobalFree(Some(hg));
            return false;
        }
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr, bytes.len());
        let _ = GlobalUnlock(hg);
        if SetClipboardData(format, Some(HANDLE(hg.0))).is_err() {
            let _ = GlobalFree(Some(hg));
            return false;
        }
        true
    }
}

/// Keeps our temporary writes out of Win+V history and cloud sync.
fn exclude_from_history() {
    write_format(register("ExcludeClipboardContentFromMonitorProcessing"), &[0]);
    write_format(register("CanIncludeInClipboardHistory"), &0u32.to_le_bytes());
    write_format(register("CanUploadToCloudClipboard"), &0u32.to_le_bytes());
}

fn utf16_bytes(text: &str) -> Vec<u8> {
    text.encode_utf16().chain([0]).flat_map(u16::to_le_bytes).collect()
}

/// Puts `text` on the clipboard. `temporary`: kept out of clipboard history.
pub fn set_text(text: &str, temporary: bool) -> bool {
    let Some(_open) = open() else { return false };
    unsafe {
        let _ = EmptyClipboard();
    }
    let ok = write_format(CF_UNICODETEXT.0 as u32, &utf16_bytes(text));
    if temporary {
        exclude_from_history();
    }
    ok
}

/// The user's clipboard, saved around a synthetic paste. Only the formats
/// people rely on: rendering every delay-rendered format of, say, an Excel
/// range could take seconds.
pub struct Snapshot(Vec<(u32, Vec<u8>)>);

pub fn snapshot() -> Snapshot {
    let formats = [
        CF_UNICODETEXT.0 as u32,
        CF_DIBV5.0 as u32,
        CF_DIB.0 as u32,
        CF_HDROP.0 as u32,
        register("HTML Format"),
        register("Rich Text Format"),
        register("PNG"),
        register("Preferred DropEffect"),
    ];
    let Some(_open) = open() else { return Snapshot(Vec::new()) };
    Snapshot(
        formats
            .into_iter()
            .filter(|&f| unsafe { IsClipboardFormatAvailable(f) }.is_ok())
            .filter_map(|f| Some((f, read_format(f)?)))
            .collect(),
    )
}

pub fn restore(snapshot: Snapshot) {
    let Some(_open) = open() else { return };
    unsafe {
        let _ = EmptyClipboard();
    }
    for (format, bytes) in &snapshot.0 {
        write_format(*format, bytes);
    }
    // The same content the user already had; don't add it to history again.
    if !snapshot.0.is_empty() {
        exclude_from_history();
    }
}

/// Changes every time anything is put on the clipboard.
pub fn sequence() -> u32 {
    unsafe { GetClipboardSequenceNumber() }
}

/// Text on the clipboard, if any.
pub fn get_text() -> Option<String> {
    let _open = open()?;
    let bytes = read_format(CF_UNICODETEXT.0 as u32)?;
    let wide: Vec<u16> =
        bytes.as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes(*c)).take_while(|&c| c != 0).collect();
    Some(String::from_utf16_lossy(&wide))
}

/// Copies the selection of the foreground app (synthetic Ctrl+C) and
/// returns it, leaving the user's clipboard as it was. `None` when nothing
/// was selected (the clipboard didn't change within `timeout`).
pub fn copy_selection(terminal: bool, timeout: Duration) -> Option<String> {
    let saved = snapshot();
    let before = sequence();
    crate::input::release_modifiers();
    crate::input::send_copy(terminal);
    let start = std::time::Instant::now();
    while sequence() == before {
        if start.elapsed() > timeout {
            return None;
        }
        std::thread::sleep(Duration::from_millis(3));
    }
    // Some apps put text on the clipboard in two steps; let them finish.
    std::thread::sleep(Duration::from_millis(15));
    let text = get_text();
    restore(saved);
    text.filter(|t| !t.trim().is_empty())
}

/// Files copied in Explorer (Ctrl+C on files), for Ctrl+V in «Перевод файлов».
pub fn files() -> Vec<std::path::PathBuf> {
    let Some(_open) = open() else { return Vec::new() };
    let Some(b) = read_format(CF_HDROP.0 as u32) else { return Vec::new() };
    parse_drop_files(&b)
}

/// DROPFILES: an offset to a list of paths, each ending in NUL, the list
/// ending in an empty one.
fn parse_drop_files(b: &[u8]) -> Vec<std::path::PathBuf> {
    if b.len() < 20 {
        return Vec::new();
    }
    let offset = u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize;
    let wide = u32::from_le_bytes([b[16], b[17], b[18], b[19]]) != 0;
    let Some(list) = b.get(offset..) else { return Vec::new() };
    let text = if wide {
        let w: Vec<u16> = list.as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes(*c)).collect();
        String::from_utf16_lossy(&w)
    } else {
        String::from_utf8_lossy(list).into_owned()
    };
    text.split('\0').take_while(|s| !s.is_empty()).map(std::path::PathBuf::from).collect()
}

/// An image is on the clipboard (a screenshot, a copied picture).
pub fn has_image() -> bool {
    unsafe { IsClipboardFormatAvailable(CF_DIB.0 as u32).is_ok() }
}

/// The clipboard's image as PNG (24- and 32-bit bitmaps — what screenshots
/// and browsers put there).
pub fn image_png() -> Option<Vec<u8>> {
    let b = {
        let _open = open()?;
        read_format(CF_DIB.0 as u32)?
    };
    let (w, h, bgra) = dib_to_bgra(&b)?;
    crate::capture::encode_png(w, h, &bgra)
}

fn dib_to_bgra(b: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    let u32_at = |o: usize| b.get(o..o + 4).map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]));
    let header = u32_at(0)? as usize;
    let width = u32_at(4)? as i32;
    let height = u32_at(8)? as i32;
    let bits = u16::from_le_bytes([*b.get(14)?, *b.get(15)?]);
    let compression = u32_at(16)?;
    let colors = u32_at(32)? as usize;
    if width <= 0 || height == 0 || !(bits == 24 || bits == 32) {
        return None;
    }
    // BI_BITFIELDS with the small header: three colour masks follow it.
    let masks = if compression == 3 && header == 40 { 12 } else { 0 };
    let start = header + masks + colors * 4;
    let (w, h) = (width as usize, height.unsigned_abs() as usize);
    let stride = (w * bits as usize / 8).div_ceil(4) * 4;
    let pixels = b.get(start..start + stride * h)?;
    let mut out = vec![0u8; w * h * 4];
    let mut any_alpha = false;
    for row in 0..h {
        // Positive height: rows are stored bottom-up.
        let src_row = if height > 0 { h - 1 - row } else { row };
        let src = &pixels[src_row * stride..];
        for x in 0..w {
            let (o, i) = ((row * w + x) * 4, x * bits as usize / 8);
            out[o..o + 3].copy_from_slice(&src[i..i + 3]);
            out[o + 3] = if bits == 32 { src[i + 3] } else { 255 };
            any_alpha |= bits == 32 && src[i + 3] != 0;
        }
    }
    if bits == 32 && !any_alpha {
        // Many apps leave the alpha byte at 0: the picture is opaque.
        out.as_chunks_mut::<4>().0.iter_mut().for_each(|p| p[3] = 255);
    }
    Some((w as u32, h as u32, out))
}

#[cfg(test)]
mod drop_tests {
    #[test]
    fn parses_wide_file_list() {
        let mut b = vec![20, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0];
        for c in "C:\\a.docx\0D:\\b.pdf\0\0".encode_utf16() {
            b.extend(c.to_le_bytes());
        }
        let files = super::parse_drop_files(&b);
        assert_eq!(files, [std::path::PathBuf::from("C:\\a.docx"), std::path::PathBuf::from("D:\\b.pdf")]);
    }

    #[test]
    fn bottom_up_24_bit() {
        // 1×2: bottom row red, top row blue (BGR), rows padded to 4 bytes.
        let mut b = vec![0u8; 40];
        b[0] = 40;
        b[4] = 1;
        b[8] = 2;
        b[14] = 24;
        b.extend([0, 0, 255, 0, 255, 0, 0, 0]);
        let (w, h, px) = super::dib_to_bgra(&b).unwrap();
        assert_eq!((w, h), (1, 2));
        assert_eq!(&px[..4], &[255, 0, 0, 255]); // top: blue
        assert_eq!(&px[4..], &[0, 0, 255, 255]); // bottom: red
    }
}
