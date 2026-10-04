//! Screen capture for "Screen area", "Window" and "Entire screen": a
//! frozen copy of the whole virtual screen (every monitor, physical
//! pixels) taken before the selection overlay appears, one monitor, and the
//! top-level windows the overlay can pick from.

use windows::Win32::Foundation::{HWND, LPARAM, POINT, RECT};
use windows::Win32::Graphics::Dwm::{DWMWA_CLOAKED, DWMWA_EXTENDED_FRAME_BOUNDS, DwmGetWindowAttribute};
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BitBlt, CAPTUREBLT, CreateCompatibleBitmap,
    CreateCompatibleDC, DIB_RGB_COLORS, DeleteDC, DeleteObject, GetDC, GetDIBits, GetMonitorInfoW,
    MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromPoint, ReleaseDC, SRCCOPY, SelectObject,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GWL_EXSTYLE, GetSystemMetrics, GetWindowLongPtrW, GetWindowRect, IsIconic,
    IsWindowVisible, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
    WS_EX_TRANSPARENT,
};
use windows::core::BOOL;

use super::image::RgbaImage;

/// The virtual screen in physical pixels: left, top (negative when a
/// monitor sits left of / above the primary one), width, height.
pub fn virtual_screen() -> (i32, i32, i32, i32) {
    unsafe {
        (
            GetSystemMetrics(SM_XVIRTUALSCREEN),
            GetSystemMetrics(SM_YVIRTUALSCREEN),
            GetSystemMetrics(SM_CXVIRTUALSCREEN),
            GetSystemMetrics(SM_CYVIRTUALSCREEN),
        )
    }
}

pub struct Screenshot {
    pub image: RgbaImage,
    /// Screen position of the image's top-left pixel.
    pub origin: (i32, i32),
}

/// Copies every monitor into one picture (GDI BitBlt).
pub fn capture_screen() -> Option<Screenshot> {
    let (x, y, w, h) = virtual_screen();
    capture_rect(x, y, w, h)
}

/// The whole monitor (not just its work area) under a screen point:
/// left, top, width, height in physical pixels.
pub fn monitor_rect_at(x: i32, y: i32) -> (i32, i32, i32, i32) {
    unsafe {
        let hm = MonitorFromPoint(POINT { x, y }, MONITOR_DEFAULTTONEAREST);
        let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
        let _ = GetMonitorInfoW(hm, &mut mi);
        let r = mi.rcMonitor;
        (r.left, r.top, r.right - r.left, r.bottom - r.top)
    }
}

/// Copies a screen rectangle (physical pixels) into a picture.
pub fn capture_rect(x: i32, y: i32, w: i32, h: i32) -> Option<Screenshot> {
    if w <= 0 || h <= 0 {
        return None;
    }
    unsafe {
        let screen = GetDC(Some(HWND::default()));
        if screen.is_invalid() {
            return None;
        }
        let mem = CreateCompatibleDC(Some(screen));
        let bitmap = CreateCompatibleBitmap(screen, w, h);
        let old = SelectObject(mem, bitmap.into());
        let copied = BitBlt(mem, 0, 0, w, h, Some(screen), x, y, SRCCOPY | CAPTUREBLT).is_ok();
        let mut info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w,
                biHeight: -h, // top-down
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut px = vec![0u8; (w * h * 4) as usize];
        SelectObject(mem, old);
        let rows = if copied {
            GetDIBits(mem, bitmap, 0, h as u32, Some(px.as_mut_ptr() as *mut _), &mut info, DIB_RGB_COLORS)
        } else {
            0
        };
        let _ = DeleteObject(bitmap.into());
        let _ = DeleteDC(mem);
        ReleaseDC(Some(HWND::default()), screen);
        if rows != h {
            return None;
        }
        for p in px.as_chunks_mut::<4>().0 {
            p.swap(0, 2);
            p[3] = 255;
        }
        Some(Screenshot {
            image: RgbaImage { width: w as u32, height: h as u32, rgba: px },
            origin: (x, y),
        })
    }
}

/// Shell windows that cover the desktop; picking them means nothing.
const SKIPPED_CLASSES: [&str; 3] = ["Progman", "WorkerW", "Shell_TrayWnd"];

/// Visible top-level windows of other processes, topmost first, as
/// physical screen rectangles (DWM's frame bounds, without the invisible
/// resize border). Cloaked windows (other virtual desktops, suspended UWP
/// apps), minimised ones, click-through overlays and our own windows are
/// left out.
pub fn window_rects() -> Vec<RECT> {
    unsafe extern "system" fn each(h: HWND, data: LPARAM) -> BOOL {
        let out = unsafe { &mut *(data.0 as *mut Vec<RECT>) };
        if let Some(r) = pickable_rect(h) {
            out.push(r);
        }
        BOOL(1)
    }
    let mut out: Vec<RECT> = Vec::new();
    unsafe {
        let _ = EnumWindows(Some(each), LPARAM(&mut out as *mut Vec<RECT> as isize));
    }
    out
}

fn pickable_rect(h: HWND) -> Option<RECT> {
    unsafe {
        if !IsWindowVisible(h).as_bool() || IsIconic(h).as_bool() || super::is_own_window(h) {
            return None;
        }
        let ex = GetWindowLongPtrW(h, GWL_EXSTYLE);
        if ex & WS_EX_TRANSPARENT.0 as isize != 0 {
            return None;
        }
        let mut cloaked = 0u32;
        let ok = DwmGetWindowAttribute(h, DWMWA_CLOAKED, &mut cloaked as *mut u32 as *mut _, 4).is_ok();
        if ok && cloaked != 0 {
            return None;
        }
        if SKIPPED_CLASSES.contains(&super::class_name(h).as_str()) {
            return None;
        }
        let mut r = RECT::default();
        let size = std::mem::size_of::<RECT>() as u32;
        if DwmGetWindowAttribute(h, DWMWA_EXTENDED_FRAME_BOUNDS, &mut r as *mut RECT as *mut _, size).is_err() {
            GetWindowRect(h, &mut r).ok()?;
        }
        (r.right - r.left >= 8 && r.bottom - r.top >= 8).then_some(r)
    }
}
