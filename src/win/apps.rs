//! Which app owns a window: used by the ignore list (shortcuts are off in
//! listed apps and in fullscreen apps) and by its "Add" picker.

use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM, RECT};
use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow};
use windows::Win32::System::Threading::{
    OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GW_OWNER, GWL_EXSTYLE, GetWindow, GetWindowLongPtrW, GetWindowRect,
    GetWindowTextLengthW, GetWindowThreadProcessId, IsIconic, IsWindowVisible, WS_EX_TOOLWINDOW,
};
use windows::core::{BOOL, PWSTR};

use super::Hwnd;

/// File name of the process that owns `h`, e.g. "game.exe".
pub fn exe_of_window(h: Hwnd) -> Option<String> {
    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(h.raw(), Some(&mut pid)) };
    if pid == 0 {
        return None;
    }
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len);
        let _ = CloseHandle(process);
        ok.ok()?;
        let path = String::from_utf16_lossy(&buf[..len as usize]);
        path.rsplit(['\\', '/']).next().map(str::to_owned)
    }
}

/// Executables of the apps that currently have a visible, titled top-level
/// window (what Alt+Tab would show), sorted and without duplicates. Our own
/// process is left out.
pub fn running_apps() -> Vec<String> {
    unsafe extern "system" fn each(h: HWND, lparam: LPARAM) -> BOOL {
        let list = unsafe { &mut *(lparam.0 as *mut Vec<Hwnd>) };
        let app_window = unsafe {
            IsWindowVisible(h).as_bool()
                && GetWindowTextLengthW(h) > 0
                && GetWindow(h, GW_OWNER).is_err()
                && GetWindowLongPtrW(h, GWL_EXSTYLE) & WS_EX_TOOLWINDOW.0 as isize == 0
        };
        if app_window && !super::is_own_window(h) {
            list.push(Hwnd::from_raw(h));
        }
        true.into()
    }
    let mut windows: Vec<Hwnd> = Vec::new();
    unsafe {
        let _ = EnumWindows(Some(each), LPARAM(&mut windows as *mut _ as isize));
    }
    let mut exes: Vec<String> = windows.into_iter().filter_map(exe_of_window).collect();
    exes.sort_by_key(|e| e.to_lowercase());
    exes.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
    exes
}

/// The window covers its whole monitor (a fullscreen game or video). The
/// desktop and the shell windows also cover the screen; they don't count.
pub fn is_fullscreen(h: Hwnd) -> bool {
    if h.is_null() || unsafe { IsIconic(h.raw()).as_bool() } {
        return false;
    }
    let class = super::class_name(h.raw());
    if matches!(class.as_str(), "Progman" | "WorkerW" | "Shell_TrayWnd" | "Shell_SecondaryTrayWnd") {
        return false;
    }
    unsafe {
        let mut r = RECT::default();
        if GetWindowRect(h.raw(), &mut r).is_err() {
            return false;
        }
        let monitor = MonitorFromWindow(h.raw(), MONITOR_DEFAULTTONEAREST);
        let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
        if !GetMonitorInfoW(monitor, &mut mi).as_bool() {
            return false;
        }
        let m = mi.rcMonitor;
        r.left <= m.left && r.top <= m.top && r.right >= m.right && r.bottom >= m.bottom
    }
}
