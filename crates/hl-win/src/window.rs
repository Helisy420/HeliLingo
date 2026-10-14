//! The foreground window, and getting ours in front of it.

use windows::Win32::Foundation::HWND;
use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
use windows::Win32::UI::Input::KeyboardAndMouse::{SetActiveWindow, SetFocus};
use windows::Win32::UI::WindowsAndMessaging::{
    BringWindowToTop, GetClassNameW, GetForegroundWindow, GetWindowThreadProcessId, SetForegroundWindow,
};

/// A `Send`-able window handle.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Hwnd(pub isize);

impl Hwnd {
    pub fn raw(self) -> HWND {
        HWND(self.0 as *mut _)
    }
    pub fn from_raw(h: HWND) -> Self {
        Self(h.0 as isize)
    }
    pub fn is_null(self) -> bool {
        self.0 == 0
    }
}

pub fn foreground() -> Hwnd {
    Hwnd::from_raw(unsafe { GetForegroundWindow() })
}

/// The window's process id.
pub fn process_id(h: Hwnd) -> u32 {
    let mut pid = 0;
    unsafe { GetWindowThreadProcessId(h.raw(), Some(&mut pid)) };
    pid
}

/// Brings `h` to the front with keyboard focus. Windows lets only the app
/// that got the last input do that, and a global shortcut doesn't count, so
/// a plain `SetForegroundWindow` often fails: the window shows but typing
/// still goes to the other app. Joining the foreground thread's input for a
/// moment is the standard way around it.
pub fn force_foreground(h: Hwnd) -> bool {
    unsafe {
        let fg = GetForegroundWindow();
        if fg == h.raw() {
            return true;
        }
        let me = GetCurrentThreadId();
        let other = if fg.is_invalid() { 0 } else { GetWindowThreadProcessId(fg, None) };
        let attached = other != 0 && other != me && AttachThreadInput(me, other, true).as_bool();
        let _ = BringWindowToTop(h.raw());
        let _ = SetForegroundWindow(h.raw());
        let _ = SetActiveWindow(h.raw());
        let _ = SetFocus(Some(h.raw()));
        if attached {
            let _ = AttachThreadInput(me, other, false);
        }
        GetForegroundWindow() == h.raw()
    }
}

fn class_name(h: Hwnd) -> String {
    let mut buf = [0u16; 128];
    let n = unsafe { GetClassNameW(h.raw(), &mut buf) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

/// Consoles paste with Shift+Insert (Ctrl+V may be a key binding there).
pub fn is_terminal(h: Hwnd) -> bool {
    matches!(
        class_name(h).as_str(),
        "ConsoleWindowClass" | "CASCADIA_HOSTING_WINDOW_CLASS" | "mintty" | "VirtualConsoleClass"
    )
}

/// The mouse pointer, in physical screen pixels.
pub fn cursor_pos() -> (i32, i32) {
    let mut p = windows::Win32::Foundation::POINT::default();
    unsafe {
        let _ = windows::Win32::UI::WindowsAndMessaging::GetCursorPos(&mut p);
    }
    (p.x, p.y)
}

/// Where to put a popup: a point and the text line's top and bottom
/// (physical pixels), so the popup can go below the line or above it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub struct Anchor {
    pub x: i32,
    pub top: i32,
    pub bottom: i32,
}

/// The text caret of the foreground app, when it exposes one (classic Win32
/// edit controls, Notepad, many IDEs; browsers and Electron usually don't).
pub fn caret_anchor() -> Option<Anchor> {
    use windows::Win32::Foundation::POINT;
    use windows::Win32::Graphics::Gdi::ClientToScreen;
    use windows::Win32::UI::WindowsAndMessaging::{GUITHREADINFO, GetGUIThreadInfo};
    unsafe {
        let fg = GetForegroundWindow();
        if fg.is_invalid() {
            return None;
        }
        let tid = GetWindowThreadProcessId(fg, None);
        let mut gui = GUITHREADINFO { cbSize: std::mem::size_of::<GUITHREADINFO>() as u32, ..Default::default() };
        GetGUIThreadInfo(tid, &mut gui).ok()?;
        if gui.hwndCaret.is_invalid() || gui.rcCaret.bottom <= gui.rcCaret.top {
            return None;
        }
        let r = gui.rcCaret;
        let mut tl = POINT { x: r.left, y: r.top };
        let mut br = POINT { x: r.right, y: r.bottom };
        let _ = ClientToScreen(gui.hwndCaret, &mut tl);
        let _ = ClientToScreen(gui.hwndCaret, &mut br);
        Some(Anchor { x: tl.x, top: tl.y, bottom: br.y })
    }
}

/// The caret if the app has one, else the mouse pointer (where the user
/// just selected).
pub fn selection_anchor() -> Anchor {
    caret_anchor().unwrap_or_else(|| {
        let (x, y) = cursor_pos();
        Anchor { x, top: y - 8, bottom: y + 18 }
    })
}

/// Is `h` a fullscreen window (a game, a video)? The desktop and taskbar
/// don't count.
pub fn is_fullscreen(h: Hwnd) -> bool {
    use windows::Win32::Foundation::RECT;
    use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow};
    use windows::Win32::UI::WindowsAndMessaging::{GetWindowRect, IsIconic};
    if h.is_null() || unsafe { IsIconic(h.raw()).as_bool() } {
        return false;
    }
    if matches!(class_name(h).as_str(), "Progman" | "WorkerW" | "Shell_TrayWnd" | "Shell_SecondaryTrayWnd") {
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

/// File name of the process that owns `h` ("chrome.exe"), lowercase.
pub fn exe_name(h: Hwnd) -> String {
    use windows::Win32::System::Threading::{
        OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
    };
    let pid = process_id(h);
    if pid == 0 {
        return String::new();
    }
    unsafe {
        let Ok(p) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else { return String::new() };
        let mut buf = [0u16; 520];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(p, PROCESS_NAME_WIN32, windows::core::PWSTR(buf.as_mut_ptr()), &mut len).is_ok();
        let _ = windows::Win32::Foundation::CloseHandle(p);
        if !ok {
            return String::new();
        }
        let path = String::from_utf16_lossy(&buf[..len as usize]);
        path.rsplit(['\\', '/']).next().unwrap_or("").to_lowercase()
    }
}

/// A popup that never takes focus: clicks don't activate it, and it has
/// no taskbar button. Call once after the window is created.
pub fn make_no_activate(h: Hwnd) {
    use windows::Win32::UI::WindowsAndMessaging::{GWL_EXSTYLE, GetWindowLongPtrW, SetWindowLongPtrW, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW};
    unsafe {
        let ex = GetWindowLongPtrW(h.raw(), GWL_EXSTYLE);
        SetWindowLongPtrW(h.raw(), GWL_EXSTYLE, ex | (WS_EX_NOACTIVATE.0 | WS_EX_TOOLWINDOW.0) as isize);
    }
}

/// Shows `h` topmost at (x, y) (physical pixels) without activating it, so
/// the app underneath keeps focus and its selection.
pub fn show_no_activate(h: Hwnd, x: i32, y: i32) {
    use windows::Win32::UI::WindowsAndMessaging::{HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOSIZE, SWP_SHOWWINDOW, SetWindowPos};
    unsafe {
        let _ = SetWindowPos(h.raw(), Some(HWND_TOPMOST), x, y, 0, 0, SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW);
    }
}

/// Hides `h` without touching focus.
pub fn hide(h: Hwnd) {
    use windows::Win32::UI::WindowsAndMessaging::{SW_HIDE, ShowWindow};
    unsafe {
        let _ = ShowWindow(h.raw(), SW_HIDE);
    }
}

/// Lets clicks fall through `h` (over its transparent margin) or not.
pub fn set_click_through(h: Hwnd, on: bool) {
    use windows::Win32::UI::WindowsAndMessaging::{GWL_EXSTYLE, GetWindowLongPtrW, SetWindowLongPtrW, WS_EX_LAYERED, WS_EX_TRANSPARENT};
    unsafe {
        let ex = GetWindowLongPtrW(h.raw(), GWL_EXSTYLE);
        let next = if on {
            ex | (WS_EX_TRANSPARENT.0 | WS_EX_LAYERED.0) as isize
        } else {
            ex & !(WS_EX_TRANSPARENT.0 as isize)
        };
        if next != ex {
            SetWindowLongPtrW(h.raw(), GWL_EXSTYLE, next);
        }
    }
}

/// The work area (screen minus taskbar) of the monitor at (x, y): left,
/// top, right, bottom, and the DPI scale.
pub fn work_area_at(x: i32, y: i32) -> (i32, i32, i32, i32, f32) {
    use windows::Win32::Foundation::POINT;
    use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromPoint};
    use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
    unsafe {
        let hm = MonitorFromPoint(POINT { x, y }, MONITOR_DEFAULTTONEAREST);
        let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
        let _ = GetMonitorInfoW(hm, &mut mi);
        let (mut dx, mut dy) = (96u32, 96u32);
        let _ = GetDpiForMonitor(hm, MDT_EFFECTIVE_DPI, &mut dx, &mut dy);
        let w = mi.rcWork;
        (w.left, w.top, w.right, w.bottom, dx as f32 / 96.0)
    }
}

/// Top-level windows a person can see, front to back, with their visible
/// frame (screen pixels: left, top, right, bottom) — for «Окно» capture.
/// Skips minimised, cloaked (hidden UWP / other-desktop) and tiny windows,
/// the desktop, and this process's own windows.
pub fn visible_windows() -> Vec<(Hwnd, (i32, i32, i32, i32))> {
    use windows::Win32::Foundation::{LPARAM, RECT};
    use windows::Win32::Graphics::Dwm::{DWMWA_CLOAKED, DWMWA_EXTENDED_FRAME_BOUNDS, DwmGetWindowAttribute};
    use windows::Win32::UI::WindowsAndMessaging::{EnumWindows, GetWindowRect, IsIconic, IsWindowVisible};
    use windows::core::BOOL;

    unsafe extern "system" fn each(h: HWND, l: LPARAM) -> BOOL {
        let list = unsafe { &mut *(l.0 as *mut Vec<HWND>) };
        list.push(h);
        BOOL(1)
    }
    let mut all: Vec<HWND> = Vec::new();
    unsafe {
        let _ = EnumWindows(Some(each), LPARAM(&mut all as *mut _ as isize));
    }
    let me = std::process::id();
    let mut out = Vec::new();
    for h in all {
        let hw = Hwnd(h.0 as isize);
        unsafe {
            if !IsWindowVisible(h).as_bool() || IsIconic(h).as_bool() || process_id(hw) == me {
                continue;
            }
            let mut cloaked = 0u32;
            let _ = DwmGetWindowAttribute(h, DWMWA_CLOAKED, &mut cloaked as *mut _ as *mut _, 4);
            if cloaked != 0 {
                continue;
            }
        }
        if matches!(class_name(hw).as_str(), "Progman" | "WorkerW") {
            continue;
        }
        let mut r = RECT::default();
        let framed = unsafe {
            DwmGetWindowAttribute(h, DWMWA_EXTENDED_FRAME_BOUNDS, &mut r as *mut _ as *mut _, std::mem::size_of::<RECT>() as u32)
        };
        if framed.is_err() && unsafe { GetWindowRect(h, &mut r) }.is_err() {
            continue;
        }
        if r.right - r.left < 80 || r.bottom - r.top < 40 {
            continue;
        }
        out.push((hw, (r.left, r.top, r.right, r.bottom)));
    }
    out
}

/// The window's title bar text.
pub fn title(h: Hwnd) -> String {
    let mut buf = [0u16; 256];
    let n = unsafe { windows::Win32::UI::WindowsAndMessaging::GetWindowTextW(h.raw(), &mut buf) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

/// Leaves `h` out of every screen capture (ours included): the live area
/// translation reads the screen under its own overlay without hiding it.
/// Windows 10 2004+; `false` where it isn't available.
pub fn exclude_from_capture(h: Hwnd) -> bool {
    use windows::Win32::UI::WindowsAndMessaging::{SetWindowDisplayAffinity, WDA_EXCLUDEFROMCAPTURE};
    unsafe { SetWindowDisplayAffinity(h.raw(), WDA_EXCLUDEFROMCAPTURE).is_ok() }
}

/// Turns "never takes focus" on or off for `h` (see `make_no_activate`).
pub fn set_no_activate(h: Hwnd, on: bool) {
    use windows::Win32::UI::WindowsAndMessaging::{GWL_EXSTYLE, GetWindowLongPtrW, SetWindowLongPtrW, WS_EX_NOACTIVATE};
    unsafe {
        let ex = GetWindowLongPtrW(h.raw(), GWL_EXSTYLE);
        let next = if on { ex | WS_EX_NOACTIVATE.0 as isize } else { ex & !(WS_EX_NOACTIVATE.0 as isize) };
        if next != ex {
            SetWindowLongPtrW(h.raw(), GWL_EXSTYLE, next);
        }
    }
}
