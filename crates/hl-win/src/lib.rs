//! Windows integration HeliLingo needs outside the webview:
//!
//! - [`window`]: the foreground window, and bringing ours to the front.
//! - [`clipboard`]: text in/out, and saving/restoring what the user had.
//! - [`input`]: synthetic Ctrl+V (Shift+Insert in terminals).
//! - [`secret`]: API keys in Windows Credential Manager.
//! - [`net`]: is the PC online.
//! - [`uia`]: the selected text through UI Automation.
//! - [`ocr`]: text in images (Windows.Media.Ocr).
//! - [`capture`]: a screen rectangle as PNG.
//! - [`paste_into`]: replace the selection in another app.
//!
//! Hooks (double Ctrl, mouse buttons), selection capture, OCR and the
//! rest follow (TODO §6).

#![cfg(windows)]

pub mod capture;
pub mod clipboard;
pub mod dialog;
pub mod hook;
pub mod input;
pub mod net;
pub mod ocr;
pub mod pdf;
pub mod secret;
pub mod uia;
pub mod window;

use std::time::Duration;

pub use window::Hwnd;

/// Replaces the selection in `target` with `text`: focuses it, pastes
/// through the clipboard, and puts the user's clipboard back. Blocking
/// (~0.4 s); run it off the UI thread.
pub fn paste_into(target: Hwnd, text: &str) {
    if target.is_null() {
        return;
    }
    if !target.is_null() && window::foreground() != target {
        window::force_foreground(target);
        std::thread::sleep(Duration::from_millis(60));
    }
    let saved = clipboard::snapshot();
    if !clipboard::set_text(text, true) {
        return;
    }
    input::release_modifiers();
    input::send_paste(window::is_terminal(target));
    // The target reads the clipboard asynchronously while handling the paste.
    std::thread::sleep(Duration::from_millis(350));
    clipboard::restore(saved);
}

/// Today as a local day number (days since 1970-01-01 in local time), for
/// per-day statistics.
pub fn local_day() -> u32 {
    let t = unsafe { windows::Win32::System::SystemInformation::GetLocalTime() };
    days_from_civil(t.wYear.into(), t.wMonth.into(), t.wDay.into())
}

/// Days since 1970-01-01 for a calendar date (Howard Hinnant's algorithm).
fn days_from_civil(y: i64, m: i64, d: i64) -> u32 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    (era * 146_097 + doe - 719_468) as u32
}

/// Opens a URL or a file with its default handler (browser, Notepad…), on
/// its own thread so a slow shell never blocks the caller.
pub fn open(target: &str) {
    let target = windows::core::HSTRING::from(target);
    std::thread::spawn(move || unsafe {
        use windows::Win32::UI::Shell::ShellExecuteW;
        use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
        ShellExecuteW(None, windows::core::w!("open"), &target, None, None, SW_SHOWNORMAL);
    });
}

/// The user's Documents folder (wherever it was moved, OneDrive included).
pub fn documents_dir() -> Option<std::path::PathBuf> {
    use windows::Win32::System::Com::CoTaskMemFree;
    use windows::Win32::UI::Shell::{FOLDERID_Documents, KF_FLAG_DEFAULT, SHGetKnownFolderPath};
    unsafe {
        let p = SHGetKnownFolderPath(&FOLDERID_Documents, KF_FLAG_DEFAULT, None).ok()?;
        let path = p.to_string().ok().map(std::path::PathBuf::from);
        CoTaskMemFree(Some(p.0 as *const _));
        path
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn civil_days() {
        assert_eq!(super::days_from_civil(1970, 1, 1), 0);
        assert_eq!(super::days_from_civil(2000, 3, 1), 11_017);
        assert_eq!(super::days_from_civil(2026, 10, 14), 20_740);
    }
}
