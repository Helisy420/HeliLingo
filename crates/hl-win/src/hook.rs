//! Low-level keyboard and mouse hooks, for what `RegisterHotKey` can't do:
//!
//! - double taps of a lone modifier ("Ctrl×2", "Shift×2", "Alt×2");
//! - mouse side buttons as shortcuts ("Mouse4", "Ctrl+Mouse5", "Mouse4×2");
//! - Ctrl+C pressed twice (translate what was just copied);
//! - while the popup is open (it never has focus): Esc, Ctrl+C, and clicks
//!   elsewhere, plus click-through over its transparent margin.
//!
//! Rules for the callbacks: they never wait on anything (events go out
//! through a channel); our own synthetic keys are ignored; and Windows
//! silently drops a hook it considers slow, so a watchdog compares the
//! system's last-input time with what the hooks saw and reinstalls them.

use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU32, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::SystemInformation::GetTickCount;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Accessibility::{HWINEVENTHOOK, SetWinEventHook};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, GetLastInputInfo, LASTINPUTINFO, VK_CONTROL, VK_ESCAPE, VK_LCONTROL, VK_LMENU, VK_LSHIFT, VK_LWIN,
    VK_MENU, VK_RCONTROL, VK_RMENU, VK_RSHIFT, VK_RWIN, VK_SHIFT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, EVENT_SYSTEM_FOREGROUND, GetMessageW, HC_ACTION, HHOOK, KBDLLHOOKSTRUCT,
    LLKHF_INJECTED, LLMHF_INJECTED, MSG, MSLLHOOKSTRUCT, PostThreadMessageW, SetWindowsHookExW, TranslateMessage,
    UnhookWindowsHookEx, WH_KEYBOARD_LL, WH_MOUSE_LL, WINEVENT_OUTOFCONTEXT, WM_APP, WM_KEYDOWN, WM_LBUTTONDOWN,
    WM_MBUTTONDOWN, WM_MOUSEMOVE, WM_RBUTTONDOWN, WM_SYSKEYDOWN, WM_XBUTTONDOWN, WM_XBUTTONUP,
};

use crate::window::{self, Hwnd};

/// What the hooks report.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HookEvent {
    /// A bound shortcut fired; the id is the caller's.
    Trigger(u32),
    /// Ctrl+C twice: the clipboard now holds what to translate.
    DoubleCopy,
    /// Esc while the popup is open.
    PopupEscape,
    /// Ctrl+C while the popup is open.
    PopupCopy,
    /// A click outside the popup while it is open.
    ClickAway,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Modifier {
    Ctrl,
    Shift,
    Alt,
}

/// A shortcut only the hook can see.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Combo {
    /// A lone modifier tapped twice.
    DoubleTap(Modifier),
    /// Mouse 4 (`button: 1`) or Mouse 5 (`2`), with modifiers, maybe ×2.
    Mouse { button: u8, ctrl: bool, shift: bool, alt: bool, double: bool },
}

impl Combo {
    /// Parses the settings format: "Ctrl×2", "Shift×2", "Alt×2", "Mouse4",
    /// "Ctrl+Mouse5", "Mouse4×2". `None` for plain chords (or garbage).
    pub fn parse(s: &str) -> Option<Combo> {
        let s = s.trim();
        let (body, double) = match s.strip_suffix("×2") {
            Some(b) => (b, true),
            None => (s, false),
        };
        let parts: Vec<&str> = body.split('+').map(str::trim).collect();
        let last = *parts.last()?;
        let has = |m: &str| parts[..parts.len() - 1].iter().any(|p| p.eq_ignore_ascii_case(m));
        let button = match last.to_ascii_lowercase().as_str() {
            "mouse4" => Some(1),
            "mouse5" => Some(2),
            _ => None,
        };
        if let Some(button) = button {
            return Some(Combo::Mouse { button, ctrl: has("ctrl"), shift: has("shift"), alt: has("alt"), double });
        }
        if double && parts.len() == 1 {
            return match last.to_ascii_lowercase().as_str() {
                "ctrl" => Some(Combo::DoubleTap(Modifier::Ctrl)),
                "shift" => Some(Combo::DoubleTap(Modifier::Shift)),
                "alt" => Some(Combo::DoubleTap(Modifier::Alt)),
                _ => None,
            };
        }
        None
    }
}

/// What the hooks work from.
#[derive(Clone, Debug, Default)]
pub struct Config {
    pub bindings: Vec<(u32, Combo)>,
    pub double_tap: Duration,
    pub double_copy: bool,
    /// Ctrl+C in the popup copies the translation (not "ignore").
    pub popup_ctrl_c: bool,
    /// Esc closes the popup (Settings: «Закрывать по Esc и клику вне бабла»).
    pub popup_esc: bool,
    /// Shortcuts and the hook do nothing (tray switch, pause).
    pub paused: bool,
    pub ignored_apps: Vec<String>,
    pub skip_fullscreen: bool,
}

static TX: OnceLock<Sender<HookEvent>> = OnceLock::new();
static CONFIG: Mutex<Option<Config>> = Mutex::new(None);
static STATE: Mutex<Matcher> = Mutex::new(Matcher::new());

/// The popup is shown (Esc, Ctrl+C and clicks are watched).
pub static POPUP_VISIBLE: AtomicBool = AtomicBool::new(false);
static POPUP_HWND: AtomicIsize = AtomicIsize::new(0);
/// The popup's card on screen (physical pixels: left, top, right, bottom).
static POPUP_CARD: Mutex<Option<(i32, i32, i32, i32)>> = Mutex::new(None);
static CLICK_THROUGH: AtomicBool = AtomicBool::new(false);

/// Foreground app, updated on every foreground change.
static FOREGROUND_EXE: Mutex<String> = Mutex::new(String::new());
static FOREGROUND_FULLSCREEN: AtomicBool = AtomicBool::new(false);

static LAST_HOOK_TICK: AtomicU32 = AtomicU32::new(0);
static HOOK_THREAD: AtomicU32 = AtomicU32::new(0);
const WM_REINSTALL: u32 = WM_APP + 1;
/// A tap is a press + release shorter than this, with nothing else pressed.
const TAP_MAX: Duration = Duration::from_millis(300);

pub fn configure(config: Config) {
    *CONFIG.lock().unwrap() = Some(config);
}

/// The popup window (for click-through) and its card rectangle while shown.
pub fn set_popup(hwnd: Hwnd, card: Option<(i32, i32, i32, i32)>) {
    POPUP_HWND.store(hwnd.0, Ordering::Relaxed);
    POPUP_VISIBLE.store(card.is_some(), Ordering::Relaxed);
    *POPUP_CARD.lock().unwrap() = card;
    if card.is_none() {
        CLICK_THROUGH.store(false, Ordering::Relaxed);
    }
}

fn card_hit(x: i32, y: i32) -> bool {
    POPUP_CARD.lock().unwrap().is_some_and(|(l, t, r, b)| x >= l && x < r && y >= t && y < b)
}

fn send(e: HookEvent) {
    if let Some(tx) = TX.get() {
        let _ = tx.send(e);
    }
}

/// Shortcuts don't fire in ignored apps or (optionally) fullscreen ones.
fn blocked(c: &Config) -> bool {
    if c.paused {
        return true;
    }
    if c.skip_fullscreen && FOREGROUND_FULLSCREEN.load(Ordering::Relaxed) {
        return true;
    }
    let exe = FOREGROUND_EXE.lock().unwrap();
    !exe.is_empty() && c.ignored_apps.iter().any(|a| a.eq_ignore_ascii_case(&exe))
}

fn down(vk: u16) -> bool {
    unsafe { GetAsyncKeyState(vk as i32) < 0 }
}

fn modifier_of(vk: u32) -> Option<Modifier> {
    let vk = vk as u16;
    if [VK_CONTROL.0, VK_LCONTROL.0, VK_RCONTROL.0].contains(&vk) {
        Some(Modifier::Ctrl)
    } else if [VK_SHIFT.0, VK_LSHIFT.0, VK_RSHIFT.0].contains(&vk) {
        Some(Modifier::Shift)
    } else if [VK_MENU.0, VK_LMENU.0, VK_RMENU.0].contains(&vk) {
        Some(Modifier::Alt)
    } else {
        None
    }
}

fn is_win(vk: u32) -> bool {
    vk as u16 == VK_LWIN.0 || vk as u16 == VK_RWIN.0
}

#[derive(Clone, Copy)]
struct Tap {
    held: bool,
    /// Another key or a click happened while it was held.
    dirty: bool,
    pressed_at: Option<Instant>,
    /// When the first complete tap was pressed.
    first: Option<Instant>,
}

impl Tap {
    const fn new() -> Self {
        Tap { held: false, dirty: false, pressed_at: None, first: None }
    }
}

struct Matcher {
    taps: [Tap; 3],
    last_copy: Option<Instant>,
    c_held: bool,
    last_mouse: Option<(u8, Instant)>,
    /// Side buttons whose press was swallowed: swallow the release too, or
    /// the browser goes Back/Forward on it.
    swallowed: [bool; 2],
}

impl Matcher {
    const fn new() -> Self {
        Matcher { taps: [Tap::new(); 3], last_copy: None, c_held: false, last_mouse: None, swallowed: [false; 2] }
    }

    fn index(m: Modifier) -> usize {
        match m {
            Modifier::Ctrl => 0,
            Modifier::Shift => 1,
            Modifier::Alt => 2,
        }
    }

    /// Something other than a lone modifier: no tap in progress counts.
    fn interrupt(&mut self) {
        for t in &mut self.taps {
            if t.held {
                t.dirty = true;
            }
            t.first = None;
        }
    }

    /// A modifier went down or up; returns it when that completes a double tap.
    fn modifier(&mut self, m: Modifier, key_down: bool, now: Instant, window: Duration) -> Option<Modifier> {
        let i = Self::index(m);
        if key_down {
            // Another modifier going down spoils the others' taps.
            for (j, t) in self.taps.iter_mut().enumerate() {
                if j != i {
                    if t.held {
                        t.dirty = true;
                    }
                    t.first = None;
                }
            }
            let t = &mut self.taps[i];
            if !t.held {
                t.held = true;
                t.dirty = false;
                t.pressed_at = Some(now);
            }
            return None;
        }
        let t = &mut self.taps[i];
        if !t.held {
            return None;
        }
        t.held = false;
        let pressed = t.pressed_at.take().unwrap_or(now);
        if t.dirty || now - pressed > TAP_MAX {
            t.first = None;
            return None;
        }
        match t.first {
            Some(first) if now - first <= window => {
                t.first = None;
                Some(m)
            }
            _ => {
                t.first = Some(pressed);
                None
            }
        }
    }
}

fn handle_key(vk: u32, key_down: bool) -> bool {
    let now = Instant::now();
    let guard = CONFIG.lock().unwrap();
    let Some(config) = guard.as_ref() else { return false };
    let mut m = STATE.lock().unwrap();
    let ctrl = down(VK_CONTROL.0);
    let alt = down(VK_MENU.0);
    let shift = down(VK_SHIFT.0);
    let win = down(VK_LWIN.0) || down(VK_RWIN.0);
    let plain = !(ctrl || alt || shift || win);

    // The popup has no focus: catch its keys here.
    if POPUP_VISIBLE.load(Ordering::Relaxed) {
        if vk as u16 == VK_ESCAPE.0 && plain && config.popup_esc {
            if key_down {
                send(HookEvent::PopupEscape);
            }
            return true;
        }
        if vk == 0x43 && key_down && ctrl && !alt && !shift && !win && config.popup_ctrl_c {
            m.interrupt();
            send(HookEvent::PopupCopy);
            return true;
        }
    }

    if let Some(md) = modifier_of(vk) {
        if let Some(hit) = m.modifier(md, key_down, now, config.double_tap)
            && !blocked(config)
            && let Some((id, _)) = config.bindings.iter().find(|(_, c)| *c == Combo::DoubleTap(hit))
        {
            send(HookEvent::Trigger(*id));
        }
        // Modifiers always reach the app.
        return false;
    }
    if is_win(vk) {
        m.interrupt();
        return false;
    }

    // Ctrl+C twice (auto-repeat doesn't count). The app copies as usual
    // both times; the second one also translates.
    if vk == 0x43 {
        if !key_down {
            m.c_held = false;
        } else if !std::mem::replace(&mut m.c_held, true) {
            if ctrl && !alt && !shift && !win {
                m.interrupt();
                let second = m.last_copy.is_some_and(|t| now - t <= config.double_tap);
                m.last_copy = if second { None } else { Some(now) };
                if second && config.double_copy && !blocked(config) {
                    send(HookEvent::DoubleCopy);
                }
                return false;
            }
            m.last_copy = None;
        }
    }
    if key_down {
        m.interrupt();
    }
    false
}

/// Mouse 4/5 (`button` 1/2). Returns true to swallow.
fn handle_side_button(button: u8, button_down: bool) -> bool {
    let i = (button - 1) as usize;
    let mut m = STATE.lock().unwrap();
    if !button_down {
        return std::mem::take(&mut m.swallowed[i]);
    }
    m.interrupt();
    let guard = CONFIG.lock().unwrap();
    let Some(config) = guard.as_ref() else { return false };
    let now = Instant::now();
    let (ctrl, shift, alt) = (down(VK_CONTROL.0), down(VK_SHIFT.0), down(VK_MENU.0));
    let single = Combo::Mouse { button, ctrl, shift, alt, double: false };
    let double = Combo::Mouse { button, ctrl, shift, alt, double: true };
    let blocked = blocked(config);
    let mut swallow = false;
    if let Some((id, _)) = config.bindings.iter().find(|(_, c)| *c == single) {
        if !blocked {
            send(HookEvent::Trigger(*id));
        }
        swallow = !blocked;
    } else if let Some((id, _)) = config.bindings.iter().find(|(_, c)| *c == double) {
        let second = m.last_mouse.is_some_and(|(b, t)| b == button && now - t <= config.double_tap);
        m.last_mouse = if second { None } else { Some((button, now)) };
        if second && !blocked {
            send(HookEvent::Trigger(*id));
        }
        swallow = !blocked;
    }
    m.swallowed[i] = swallow;
    swallow
}

unsafe extern "system" fn keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    LAST_HOOK_TICK.store(unsafe { GetTickCount() }, Ordering::Relaxed);
    if code == HC_ACTION as i32 {
        let k = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
        let key_down = matches!(wparam.0 as u32, WM_KEYDOWN | WM_SYSKEYDOWN);
        // Our own synthetic Ctrl+C / Ctrl+V must not count.
        if !k.flags.contains(LLKHF_INJECTED) && handle_key(k.vkCode, key_down) {
            return LRESULT(1);
        }
    }
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

unsafe extern "system" fn mouse_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    LAST_HOOK_TICK.store(unsafe { GetTickCount() }, Ordering::Relaxed);
    if code == HC_ACTION as i32 {
        let ms = unsafe { &*(lparam.0 as *const MSLLHOOKSTRUCT) };
        let msg = wparam.0 as u32;
        let injected = ms.flags & LLMHF_INJECTED != 0;
        if !injected && matches!(msg, WM_XBUTTONDOWN | WM_XBUTTONUP) {
            let button = (ms.mouseData >> 16) as u8;
            if (1..=2).contains(&button) && handle_side_button(button, msg == WM_XBUTTONDOWN) {
                return LRESULT(1);
            }
        }
        if POPUP_VISIBLE.load(Ordering::Relaxed) {
            if msg == WM_MOUSEMOVE {
                // Click-through over the popup's transparent margin, set
                // before Windows routes the next click.
                let through = !card_hit(ms.pt.x, ms.pt.y);
                if CLICK_THROUGH.swap(through, Ordering::Relaxed) != through {
                    window::set_click_through(Hwnd(POPUP_HWND.load(Ordering::Relaxed)), through);
                }
            } else if matches!(msg, WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN) && !card_hit(ms.pt.x, ms.pt.y) {
                send(HookEvent::ClickAway);
            }
        }
        if matches!(msg, WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN) {
            STATE.lock().unwrap().interrupt();
        }
    }
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

unsafe extern "system" fn foreground_proc(
    _hook: HWINEVENTHOOK,
    _event: u32,
    hwnd: HWND,
    _id_object: i32,
    _id_child: i32,
    _thread: u32,
    _time: u32,
) {
    let h = Hwnd::from_raw(hwnd);
    *FOREGROUND_EXE.lock().unwrap() = window::exe_name(h);
    FOREGROUND_FULLSCREEN.store(window::is_fullscreen(h), Ordering::Relaxed);
}

/// Starts the hook thread (once) and its watchdog. Events arrive on `tx`.
pub fn start(tx: Sender<HookEvent>) {
    if TX.set(tx).is_err() {
        return;
    }
    std::thread::Builder::new()
        .name("input-hook".into())
        .spawn(|| unsafe {
            let install = || {
                let instance = GetModuleHandleW(None).ok().map(Into::into);
                let kb = SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_proc), instance, 0).ok();
                let mouse = SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), instance, 0).ok();
                (kb, mouse)
            };
            let mut hooks: (Option<HHOOK>, Option<HHOOK>) = install();
            let _fg = SetWinEventHook(
                EVENT_SYSTEM_FOREGROUND,
                EVENT_SYSTEM_FOREGROUND,
                None,
                Some(foreground_proc),
                0,
                0,
                WINEVENT_OUTOFCONTEXT,
            );
            HOOK_THREAD.store(GetCurrentThreadId(), Ordering::Relaxed);
            LAST_HOOK_TICK.store(GetTickCount(), Ordering::Relaxed);
            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                if msg.message == WM_REINSTALL && msg.hwnd.is_invalid() {
                    // New hooks first, then drop the old ones: no key slips through.
                    let fresh = install();
                    for h in [hooks.0, hooks.1].into_iter().flatten() {
                        let _ = UnhookWindowsHookEx(h);
                    }
                    hooks = fresh;
                    LAST_HOOK_TICK.store(GetTickCount(), Ordering::Relaxed);
                    continue;
                }
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        })
        .expect("spawn hook thread");
    std::thread::Builder::new().name("input-hook-watchdog".into()).spawn(watchdog).expect("spawn hook watchdog");
}

/// Reinstalls the hooks when the system saw input that they didn't.
fn watchdog() {
    let mut last = Instant::now();
    loop {
        std::thread::sleep(Duration::from_secs(1));
        let mut info = LASTINPUTINFO { cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32, dwTime: 0 };
        if !unsafe { GetLastInputInfo(&mut info) }.as_bool() {
            continue;
        }
        let now = unsafe { GetTickCount() };
        let seen = LAST_HOOK_TICK.load(Ordering::Relaxed);
        let missed = info.dwTime.wrapping_sub(seen) as i32 > 200 && now.wrapping_sub(info.dwTime) > 300;
        let thread = HOOK_THREAD.load(Ordering::Relaxed);
        if missed && thread != 0 && last.elapsed() > Duration::from_secs(2) {
            last = Instant::now();
            unsafe {
                let _ = PostThreadMessageW(thread, WM_REINSTALL, WPARAM(0), LPARAM(0));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_settings_strings() {
        assert_eq!(Combo::parse("Ctrl×2"), Some(Combo::DoubleTap(Modifier::Ctrl)));
        assert_eq!(Combo::parse("Shift×2"), Some(Combo::DoubleTap(Modifier::Shift)));
        assert_eq!(
            Combo::parse("Ctrl+Mouse5"),
            Some(Combo::Mouse { button: 2, ctrl: true, shift: false, alt: false, double: false })
        );
        assert_eq!(
            Combo::parse("Mouse4×2"),
            Some(Combo::Mouse { button: 1, ctrl: false, shift: false, alt: false, double: true })
        );
        assert_eq!(
            Combo::parse("Ctrl+Alt+Mouse5"),
            Some(Combo::Mouse { button: 2, ctrl: true, shift: false, alt: true, double: false })
        );
        assert_eq!(
            Combo::parse("Mouse4"),
            Some(Combo::Mouse { button: 1, ctrl: false, shift: false, alt: false, double: false })
        );
        assert_eq!(Combo::parse("Ctrl+Shift+L"), None);
        assert_eq!(Combo::parse(""), None);
    }

    #[test]
    fn double_tap_timing() {
        let mut m = Matcher::new();
        let w = Duration::from_millis(400);
        let t0 = Instant::now();
        let ms = |n| t0 + Duration::from_millis(n);
        assert_eq!(m.modifier(Modifier::Ctrl, true, ms(0), w), None);
        assert_eq!(m.modifier(Modifier::Ctrl, false, ms(80), w), None);
        assert_eq!(m.modifier(Modifier::Ctrl, true, ms(200), w), None);
        assert_eq!(m.modifier(Modifier::Ctrl, false, ms(260), w), Some(Modifier::Ctrl));
        // Too slow: no double tap.
        assert_eq!(m.modifier(Modifier::Ctrl, true, ms(1000), w), None);
        assert_eq!(m.modifier(Modifier::Ctrl, false, ms(1050), w), None);
        assert_eq!(m.modifier(Modifier::Ctrl, true, ms(1600), w), None);
        assert_eq!(m.modifier(Modifier::Ctrl, false, ms(1650), w), None);
        // Ctrl+key in between: not a tap.
        assert_eq!(m.modifier(Modifier::Ctrl, true, ms(3000), w), None);
        m.interrupt();
        assert_eq!(m.modifier(Modifier::Ctrl, false, ms(3050), w), None);
        assert_eq!(m.modifier(Modifier::Ctrl, true, ms(3100), w), None);
        assert_eq!(m.modifier(Modifier::Ctrl, false, ms(3150), w), None);
    }
}
