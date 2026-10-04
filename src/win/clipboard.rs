//! Clipboard access, used to read the selection (synthetic Ctrl+C) and to
//! replace it (synthetic Ctrl+V) without clobbering what the user had copied.

use std::time::{Duration, Instant};

use windows::Win32::Foundation::{GlobalFree, HANDLE, HGLOBAL};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, GetClipboardSequenceNumber,
    IsClipboardFormatAvailable, OpenClipboard, RegisterClipboardFormatW, SetClipboardData,
};
use windows::Win32::System::Memory::{
    GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock,
};
use windows::Win32::System::Ole::{CF_DIB, CF_DIBV5, CF_HDROP, CF_UNICODETEXT};
use windows::core::{HSTRING, PCWSTR};

use super::{Hwnd, input};

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

/// Keeps our temporary clipboard writes out of Win+V history and cloud sync.
fn write_history_exclusion() {
    write_format(register("ExcludeClipboardContentFromMonitorProcessing"), &[0]);
    write_format(register("CanIncludeInClipboardHistory"), &0u32.to_le_bytes());
    write_format(register("CanUploadToCloudClipboard"), &0u32.to_le_bytes());
}

fn utf16_bytes(text: &str) -> Vec<u8> {
    text.encode_utf16()
        .chain([0])
        .flat_map(u16::to_le_bytes)
        .collect()
}

pub fn sequence() -> u32 {
    unsafe { GetClipboardSequenceNumber() }
}

pub fn get_text() -> Option<String> {
    let _open = open()?;
    let bytes = read_format(CF_UNICODETEXT.0 as u32)?;
    let wide: Vec<u16> = bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| u16::from_le_bytes(*c))
        .take_while(|&c| c != 0)
        .collect();
    Some(String::from_utf16_lossy(&wide))
}

pub fn set_text(text: &str, temporary: bool) -> bool {
    let Some(_open) = open() else { return false };
    unsafe {
        let _ = EmptyClipboard();
    }
    let ok = write_format(CF_UNICODETEXT.0 as u32, &utf16_bytes(text));
    if temporary {
        write_history_exclusion();
    }
    ok
}

/// The user's clipboard contents, saved around our synthetic copy/paste.
/// Only the formats people actually rely on are kept: rendering every
/// delay-rendered format of e.g. an Excel range could take seconds.
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
    let Some(_open) = open() else {
        return Snapshot(Vec::new());
    };
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
    if snapshot.0.is_empty() {
        return;
    }
    for (format, bytes) in &snapshot.0 {
        write_format(*format, bytes);
    }
    // It's the same content the user already had; don't duplicate it in history.
    write_history_exclusion();
}

/// Copies the current selection of the foreground app and returns it,
/// leaving the user's clipboard as it was. `None` if nothing is selected.
pub fn copy_selection(target: Hwnd, timeout: Duration) -> Option<String> {
    let saved = snapshot();
    let before = sequence();
    input::release_modifiers();
    input::send_copy(super::is_terminal(target));

    let start = Instant::now();
    while sequence() == before {
        if start.elapsed() > timeout {
            return None;
        }
        std::thread::sleep(Duration::from_millis(3));
    }
    let text = get_text();
    restore(saved);
    text.filter(|t| !t.trim().is_empty())
}

/// Replaces the selection in `target` with `text` via a synthetic paste.
pub fn paste_text(target: Hwnd, text: &str) {
    if !target.is_null() && super::foreground() != target {
        super::focus(target);
        std::thread::sleep(Duration::from_millis(60));
    }
    let saved = snapshot();
    if !set_text(text, true) {
        return;
    }
    input::release_modifiers();
    input::send_paste(super::is_terminal(target));
    // The target reads the clipboard asynchronously while handling the paste.
    std::thread::sleep(Duration::from_millis(350));
    restore(saved);
}
