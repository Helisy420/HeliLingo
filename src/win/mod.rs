//! Thin wrappers over the Win32 APIs the app needs: window placement,
//! caret/cursor position, autostart, and single-instance guard.

pub mod apps;
pub mod capture;
pub mod clipboard;
pub mod hook;
pub mod image;
pub mod input;
pub mod ocr;
pub mod secret;
pub mod speech;

use windows::Win32::Foundation::{
    COLORREF, ERROR_ALREADY_EXISTS, GetLastError, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM,
};
use windows::Win32::Graphics::Gdi::{
    ClientToScreen, GetMonitorInfoW, ScreenToClient, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromPoint,
};
use windows::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, KEY_SET_VALUE, REG_SZ, RRF_RT_REG_SZ, RegCloseKey, RegDeleteValueW,
    RegGetValueW, RegOpenKeyExW, RegSetValueExW,
};
use windows::Win32::System::Threading::{CreateMutexW, GetCurrentProcessId};
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::WindowsAndMessaging::{
    CallWindowProcW, DefWindowProcW, FindWindowW, GUITHREADINFO, GWLP_WNDPROC, WM_NCCALCSIZE, WNDPROC, GWL_EXSTYLE, GWL_STYLE, SWP_FRAMECHANGED, SWP_NOMOVE, SWP_NOZORDER,
    WS_CAPTION, WS_MAXIMIZEBOX, WS_MINIMIZEBOX, WS_POPUP, WS_SYSMENU, WS_THICKFRAME, GetClassNameW, GetCursorPos, GetForegroundWindow,
    GetGUIThreadInfo, GetWindowLongPtrW, GetWindowRect, GetWindowThreadProcessId, HWND_TOPMOST,
    IsWindowVisible, LWA_ALPHA, PostMessageW, WM_MOUSEMOVE, SW_HIDE, SetLayeredWindowAttributes, SW_SHOW, SW_SHOWNOACTIVATE, SWP_NOACTIVATE, SWP_NOSIZE,
    SetForegroundWindow, SetWindowLongPtrW, SetWindowPos, ShowWindow, WS_EX_LAYERED,
    WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TRANSPARENT,
};
use windows::core::{HSTRING, w};

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

/// Screen rectangle (physical pixels) the popup should sit next to.
#[derive(Clone, Copy, Debug)]
pub struct Anchor {
    pub x: i32,
    pub top: i32,
    pub bottom: i32,
}

#[derive(Clone, Copy, Debug)]
pub struct Monitor {
    pub work: RECT,
    /// Physical pixels per logical point.
    pub scale: f32,
}

/// Lets `println!` reach the terminal the app was started from. The app is
/// a GUI-subsystem binary, so it has no console of its own.
pub fn attach_parent_console() {
    unsafe {
        let _ = windows::Win32::System::Console::AttachConsole(
            windows::Win32::System::Console::ATTACH_PARENT_PROCESS,
        );
    }
}

/// Windows' "Show animations in Windows" (Settings → Accessibility →
/// Visual effects). When off, the app doesn't animate either.
pub fn system_animations() -> bool {
    use windows::Win32::UI::WindowsAndMessaging::{
        SPI_GETCLIENTAREAANIMATION, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, SystemParametersInfoW,
    };
    let mut on = windows::core::BOOL(1);
    unsafe {
        let _ = SystemParametersInfoW(
            SPI_GETCLIENTAREAANIMATION,
            0,
            Some(&mut on as *mut _ as *mut _),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        );
    }
    on.as_bool()
}

pub fn single_instance() -> bool {
    unsafe {
        match CreateMutexW(None, true, w!("Local\\HeliLingo.SingleInstance")) {
            // Leak the handle: the mutex must live as long as the process.
            Ok(_handle) => GetLastError() != ERROR_ALREADY_EXISTS,
            Err(_) => true,
        }
    }
}

pub fn cursor_pos() -> (i32, i32) {
    let mut p = POINT::default();
    unsafe {
        let _ = GetCursorPos(&mut p);
    }
    (p.x, p.y)
}

/// The text caret of the foreground app, when it exposes one (classic Win32
/// edit controls, Notepad, many IDEs). Browsers/Electron usually don't.
pub fn caret_anchor() -> Option<Anchor> {
    unsafe {
        let fg = GetForegroundWindow();
        if fg.is_invalid() {
            return None;
        }
        let tid = GetWindowThreadProcessId(fg, None);
        let mut gui = GUITHREADINFO {
            cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
            ..Default::default()
        };
        GetGUIThreadInfo(tid, &mut gui).ok()?;
        if gui.hwndCaret.is_invalid() {
            return None;
        }
        let r = gui.rcCaret;
        if r.bottom <= r.top {
            return None;
        }
        let mut tl = POINT { x: r.left, y: r.top };
        let mut br = POINT { x: r.right, y: r.bottom };
        let _ = ClientToScreen(gui.hwndCaret, &mut tl);
        let _ = ClientToScreen(gui.hwndCaret, &mut br);
        Some(Anchor { x: tl.x, top: tl.y, bottom: br.y })
    }
}

pub fn cursor_anchor() -> Anchor {
    let (x, y) = cursor_pos();
    let scale = monitor_at(x, y).scale;
    Anchor {
        x,
        top: y - (6.0 * scale) as i32,
        bottom: y + (16.0 * scale) as i32,
    }
}

pub fn monitor_at(x: i32, y: i32) -> Monitor {
    unsafe {
        let hm = MonitorFromPoint(POINT { x, y }, MONITOR_DEFAULTTONEAREST);
        let mut mi = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        let _ = GetMonitorInfoW(hm, &mut mi);
        let (mut dx, mut dy) = (96u32, 96u32);
        let _ = GetDpiForMonitor(hm, MDT_EFFECTIVE_DPI, &mut dx, &mut dy);
        Monitor {
            work: mi.rcWork,
            scale: dx as f32 / 96.0,
        }
    }
}

pub fn foreground() -> Hwnd {
    Hwnd::from_raw(unsafe { GetForegroundWindow() })
}

pub fn focus(h: Hwnd) -> bool {
    unsafe { SetForegroundWindow(h.raw()).as_bool() }
}

pub fn find_window(title: &str) -> Option<Hwnd> {
    unsafe { FindWindowW(None, &HSTRING::from(title)).ok() }
        .filter(|h| !h.is_invalid())
        .map(Hwnd::from_raw)
}

pub fn window_rect(h: Hwnd) -> RECT {
    let mut r = RECT::default();
    unsafe {
        let _ = GetWindowRect(h.raw(), &mut r);
    }
    r
}

pub fn move_window(h: Hwnd, x: i32, y: i32) {
    unsafe {
        let _ = SetWindowPos(
            h.raw(),
            Some(HWND_TOPMOST),
            x,
            y,
            0,
            0,
            SWP_NOSIZE | SWP_NOACTIVATE,
        );
    }
}

pub fn show_no_activate(h: Hwnd) {
    unsafe {
        let _ = ShowWindow(h.raw(), SW_SHOWNOACTIVATE);
    }
}

pub fn show_and_focus(h: Hwnd) {
    unsafe {
        let _ = ShowWindow(h.raw(), SW_SHOW);
        let _ = SetForegroundWindow(h.raw());
    }
}

/// Where parked windows wait, off every monitor.
const PARK: i32 = -32000;

/// Keeps a window shown but off every screen (no taskbar button: tool
/// windows only). eframe repaints *hidden* windows at most every 100 ms,
/// so a hidden window adds up to 100 ms to every wake-up or first frame;
/// a parked window repaints at once and appears by just moving it.
pub fn park(h: Hwnd) {
    unsafe {
        let _ = SetWindowPos(
            h.raw(),
            None,
            PARK,
            PARK,
            0,
            0,
            SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | windows::Win32::UI::WindowsAndMessaging::SWP_SHOWWINDOW,
        );
    }
}

/// Parked (or hidden): not on any screen.
pub fn is_parked(h: Hwnd) -> bool {
    !is_visible(h) || window_rect(h).left <= PARK + 100
}

pub fn hide(h: Hwnd) {
    unsafe {
        let _ = ShowWindow(h.raw(), SW_HIDE);
    }
}

pub fn is_visible(h: Hwnd) -> bool {
    unsafe { IsWindowVisible(h.raw()).as_bool() }
}

fn update_ex_style(h: Hwnd, f: impl FnOnce(isize) -> isize) {
    unsafe {
        let ex = GetWindowLongPtrW(h.raw(), GWL_EXSTYLE);
        let new = f(ex);
        if new != ex {
            SetWindowLongPtrW(h.raw(), GWL_EXSTYLE, new);
        }
    }
}

/// Original window procedures of the windows we subclassed, by handle.
static ORIGINAL_PROCS: std::sync::Mutex<Vec<(isize, isize)>> = std::sync::Mutex::new(Vec::new());

/// egui-winit turns on winit's "undecorated shadow" for borderless windows,
/// which moves the client area 1px down and leaves the top row of the
/// window to the non-client frame: a white line across the transparent
/// margin. This subclass makes the client area the whole window again.
fn full_client_area(h: Hwnd) {
    let mut procs = ORIGINAL_PROCS.lock().unwrap();
    if procs.iter().any(|(w, _)| *w == h.0) {
        return;
    }
    let old = unsafe { SetWindowLongPtrW(h.raw(), GWLP_WNDPROC, nc_proc as *const () as isize) };
    if old != 0 {
        procs.push((h.0, old));
    }
}

unsafe extern "system" fn nc_proc(h: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    use windows::Win32::UI::WindowsAndMessaging::{WM_CREATE, WM_ERASEBKGND, WM_NCACTIVATE, WM_NCPAINT};
    match msg {
        // Keep the proposed rectangle: client area = window area, so there
        // is no caption or frame for Windows to paint.
        WM_NCCALCSIZE if wparam.0 != 0 => return LRESULT(0),
        // The GL surface paints everything; erasing with the class brush
        // would flash white before the first frame (and on resize/focus).
        WM_ERASEBKGND => return LRESULT(1),
        // Nothing non-client to draw; stops a classic caption repaint on
        // focus changes.
        WM_NCPAINT => return LRESULT(0),
        WM_NCACTIVATE => return LRESULT(1),
        WM_CREATE => dwm_frame_off(Hwnd::from_raw(h)),
        _ => {}
    }
    let old = ORIGINAL_PROCS
        .lock()
        .unwrap()
        .iter()
        .find(|(w, _)| *w == h.0 as isize)
        .map(|(_, p)| *p);
    match old {
        Some(p) => unsafe {
            let proc: WNDPROC = std::mem::transmute(p);
            CallWindowProcW(proc, h, msg, wparam, lparam)
        },
        None => unsafe { DefWindowProcW(h, msg, wparam, lparam) },
    }
}

/// Turns off what DWM draws around a window: non-client rendering, the
/// Windows 11 border, corner rounding and show/hide transitions.
fn dwm_frame_off(h: Hwnd) {
    use windows::Win32::Graphics::Dwm::{
        DWMNCRP_DISABLED, DWMWA_BORDER_COLOR, DWMWA_COLOR_NONE, DWMWA_NCRENDERING_POLICY,
        DWMWA_TRANSITIONS_FORCEDISABLED, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_DONOTROUND,
        DwmSetWindowAttribute,
    };
    let set = |attr, value: u32| unsafe {
        let _ = DwmSetWindowAttribute(h.raw(), attr, &value as *const u32 as *const _, 4);
    };
    set(DWMWA_NCRENDERING_POLICY, DWMNCRP_DISABLED.0 as u32);
    set(DWMWA_BORDER_COLOR, DWMWA_COLOR_NONE);
    set(DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_DONOTROUND.0 as u32);
    set(DWMWA_TRANSITIONS_FORCEDISABLED, 1);
}

/// Subclasses every window the UI thread creates from now on (all egui
/// viewports: settings, translator, Quick, Ultra, tray menu…) the moment it
/// is created, before it is ever shown. Without this, a new window shows
/// a white Windows caption and background for a frame or two before
/// [`strip_chrome`] runs — the white flicker at the top. Call once, on the
/// UI thread.
pub fn install_chrome_hook() {
    use windows::Win32::System::Threading::GetCurrentThreadId;
    use windows::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, HCBT_CREATEWND, SetWindowsHookExW, WH_CBT,
    };
    unsafe extern "system" fn cbt(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if code == HCBT_CREATEWND as i32 {
            let h = HWND(wparam.0 as *mut _);
            // winit's top-level windows only (not the tray's hidden window,
            // IME windows or child controls).
            let top = unsafe { GetWindowLongPtrW(h, GWL_STYLE) } & windows::Win32::UI::WindowsAndMessaging::WS_CHILD.0 as isize == 0;
            if top && class_name(h) == "Window Class" {
                full_client_area(Hwnd::from_raw(h));
            }
        }
        unsafe { CallNextHookEx(None, code, wparam, lparam) }
    }
    unsafe {
        let _ = SetWindowsHookExW(WH_CBT, Some(cbt), None, GetCurrentThreadId());
    }
}

/// Removes everything DWM draws around a borderless transparent window:
/// the frame styles winit keeps, the Windows 11 1px border (a white line
/// across the top of the transparent window margin) and rounded clipping.
/// The card inside draws its own border and corners.
pub fn strip_chrome(h: Hwnd) {
    full_client_area(h);
    unsafe {
        let style = GetWindowLongPtrW(h.raw(), GWL_STYLE);
        let strip = (WS_CAPTION.0 | WS_SYSMENU.0 | WS_THICKFRAME.0 | WS_MINIMIZEBOX.0 | WS_MAXIMIZEBOX.0) as isize;
        let new = (style & !strip) | WS_POPUP.0 as isize;
        if new != style {
            SetWindowLongPtrW(h.raw(), GWL_STYLE, new);
        }
        // Recompute the client area with the subclass in place.
        let _ = SetWindowPos(
            h.raw(),
            None,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
        );
    }
    dwm_frame_off(h);
}

/// Never take focus when shown or clicked, and stay out of Alt+Tab — the
/// user's selection in the source app must survive while the popup is open.
///
/// The window is also kept layered (opaque constant alpha; per-pixel
/// transparency still comes from the GL surface): the transparent popup
/// only composites reliably as a layered window, and `WS_EX_TRANSPARENT`
/// click-through requires it anyway.
pub fn make_tool_popup(h: Hwnd) {
    // winit keeps caption/system-menu styles on borderless windows; on a
    // layered window DWM would draw a ghost close button in the corner.
    strip_chrome(h);
    update_ex_style(h, |ex| {
        ex | (WS_EX_NOACTIVATE.0 | WS_EX_TOOLWINDOW.0 | WS_EX_LAYERED.0) as isize
    });
    unsafe {
        let _ = SetLayeredWindowAttributes(h.raw(), COLORREF(0), 255, LWA_ALPHA);
    }
}

/// Starts moving our window under the cursor with the system's move loop,
/// as if its title bar had been grabbed. Call while the left button is
/// down (on a drag start). egui's `StartDrag` does the same but only when
/// winit thinks the window has focus, which our windows often don't (a tray
/// app may not take focus when it opens one), so the drag was lost.
pub fn begin_window_drag() {
    use windows::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture;
    use windows::Win32::UI::WindowsAndMessaging::{GA_ROOT, GetAncestor, HTCAPTION, WM_NCLBUTTONDOWN, WindowFromPoint};
    let (x, y) = cursor_pos();
    unsafe {
        let root = GetAncestor(WindowFromPoint(POINT { x, y }), GA_ROOT);
        if root.is_invalid() || !is_own_window(root) {
            return;
        }
        let _ = ReleaseCapture();
        let lparam = ((y as u32 & 0xFFFF) << 16) | (x as u32 & 0xFFFF);
        let _ = PostMessageW(Some(root), WM_NCLBUTTONDOWN, WPARAM(HTCAPTION as usize), LPARAM(lparam as isize));
    }
}

/// Tells the window where the cursor is (screen coordinates).
pub fn post_mouse_move(h: Hwnd, x: i32, y: i32) {
    let mut p = POINT { x, y };
    unsafe {
        let _ = ScreenToClient(h.raw(), &mut p);
        let lparam = ((p.y as u32 & 0xFFFF) << 16) | (p.x as u32 & 0xFFFF);
        let _ = PostMessageW(Some(h.raw()), WM_MOUSEMOVE, WPARAM(0), LPARAM(lparam as isize));
    }
}

/// Let mouse input fall through the transparent parts of the popup window.
pub fn set_click_through(h: Hwnd, on: bool) {
    let bit = WS_EX_TRANSPARENT.0 as isize;
    update_ex_style(h, |ex| if on { ex | bit } else { ex & !bit });
}

pub fn is_own_window(h: HWND) -> bool {
    let mut pid = 0u32;
    unsafe {
        GetWindowThreadProcessId(h, Some(&mut pid));
        pid == GetCurrentProcessId()
    }
}

pub fn class_name(h: HWND) -> String {
    let mut buf = [0u16; 128];
    let n = unsafe { GetClassNameW(h, &mut buf) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

/// Console hosts treat Ctrl+C as an interrupt, so they get Ctrl+Insert.
pub fn is_terminal(h: Hwnd) -> bool {
    matches!(
        class_name(h.raw()).as_str(),
        "ConsoleWindowClass" | "CASCADIA_HOSTING_WINDOW_CLASS" | "mintty" | "VirtualConsoleClass"
    )
}

/// Opens a URL (or a folder) with its default handler, e.g. the browser.
/// Runs on its own thread so a slow shell never blocks the UI.
pub fn open_url(url: &str) {
    let target = HSTRING::from(url);
    std::thread::spawn(move || unsafe {
        use windows::Win32::UI::Shell::ShellExecuteW;
        ShellExecuteW(None, w!("open"), &target, None, None, SW_SHOW);
    });
}

const RUN_KEY: windows::core::PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
const RUN_VALUE: windows::core::PCWSTR = w!("HeliLingo");
/// Autostart value written by versions named Quick Translate.
const OLD_RUN_VALUE: windows::core::PCWSTR = w!("QuickTranslate");

pub fn autostart_enabled() -> bool {
    let has = |value| unsafe {
        RegGetValueW(HKEY_CURRENT_USER, RUN_KEY, value, RRF_RT_REG_SZ, None, None, None).is_ok()
    };
    if has(OLD_RUN_VALUE) {
        // Started with Windows under the old name: move the entry over
        // (it also points the entry at this executable, helilingo.exe).
        unsafe {
            let mut key = HKEY::default();
            if RegOpenKeyExW(HKEY_CURRENT_USER, RUN_KEY, None, KEY_SET_VALUE, &mut key).is_ok() {
                let _ = RegDeleteValueW(key, OLD_RUN_VALUE);
                let _ = RegCloseKey(key);
            }
        }
        return set_autostart(true);
    }
    has(RUN_VALUE)
}

pub fn set_autostart(on: bool) -> bool {
    unsafe {
        let mut key = HKEY::default();
        if RegOpenKeyExW(HKEY_CURRENT_USER, RUN_KEY, None, KEY_SET_VALUE, &mut key).is_err() {
            return false;
        }
        let ok = if on {
            let exe = std::env::current_exe().unwrap_or_default();
            let value: Vec<u16> = format!("\"{}\"", exe.display())
                .encode_utf16()
                .chain([0])
                .collect();
            let bytes = std::slice::from_raw_parts(value.as_ptr() as *const u8, value.len() * 2);
            RegSetValueExW(key, RUN_VALUE, None, REG_SZ, Some(bytes)).is_ok()
        } else {
            RegDeleteValueW(key, RUN_VALUE).is_ok()
        };
        let _ = RegCloseKey(key);
        ok
    }
}
