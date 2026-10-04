//! Global low-level keyboard/mouse hooks: detects the shortcuts, Esc /
//! Ctrl+C / click-away while the popup is open, and records new shortcuts
//! for Settings → Hotkeys.
//!
//! A binding is a [`Combo`]: a chord (Ctrl + Alt + A), a chord whose last
//! key is tapped twice (Ctrl + Alt + A ×2), or a double tap of a lone
//! modifier (Ctrl ×2, Alt ×2, Shift ×2). The mouse side buttons (Mouse 4,
//! Mouse 5) work like keys, alone or with modifiers. Ctrl+C pressed twice
//! (DeepL style) is a fixed extra trigger, switched in the settings.
//!
//! Shortcuts are ignored while an app from the ignore list, or (optionally)
//! any fullscreen app, is in the foreground; the keys then pass through.
//!
//! Windows silently removes a low-level hook whose callback is slow (and,
//! in practice, sometimes for no visible reason), after which no shortcut
//! works. So the callbacks never wait on the UI (events go through a
//! channel and a separate waker thread asks for a repaint), and a watchdog
//! compares the system's last-input time with the last event the hooks
//! saw: input the hooks missed means they were dropped, and they are
//! installed again within a second.

use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU32, Ordering};
use std::sync::mpsc::{Sender, channel};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Accessibility::{HWINEVENTHOOK, SetWinEventHook};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, VK_BACK, VK_CONTROL, VK_DELETE, VK_ESCAPE, VK_LCONTROL, VK_LMENU, VK_LSHIFT,
    VK_LWIN, VK_MENU, VK_RCONTROL, VK_RMENU, VK_RSHIFT, VK_RWIN, VK_SHIFT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, EVENT_SYSTEM_FOREGROUND, GetMessageW, HC_ACTION, HHOOK,
    KBDLLHOOKSTRUCT, LLKHF_INJECTED, LLMHF_INJECTED, MSG, MSLLHOOKSTRUCT, PostThreadMessageW,
    SetWindowsHookExW,
    TranslateMessage, UnhookWindowsHookEx, WH_KEYBOARD_LL, WH_MOUSE_LL, WINEVENT_OUTOFCONTEXT,
    WM_APP, WM_KEYDOWN, WM_LBUTTONDOWN, WM_MBUTTONDOWN, WM_RBUTTONDOWN, WM_SYSKEYDOWN, WM_XBUTTONDOWN,
    WM_XBUTTONUP,
};
use windows::Win32::System::SystemInformation::GetTickCount;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};

use super::{Hwnd, apps};
use crate::settings::{self, Combo, HotkeySlot, PopupCtrlC, Settings};

#[derive(Debug)]
pub enum HookEvent {
    /// The user pressed one of the shortcuts.
    Trigger(HotkeySlot),
    /// Esc while the popup, Quick or Ultra window is open.
    Escape,
    /// Ctrl+C while the popup is visible (unless set to Ignore).
    PopupCopy,
    /// A mouse button went down somewhere (physical screen coordinates).
    MouseDown { x: i32, y: i32 },
    /// Result of recording a shortcut.
    Captured(Capture),
}

/// How a shortcut recording ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Capture {
    /// Esc: keep the old binding.
    Cancel,
    /// Backspace / Delete: turn the shortcut off.
    Clear,
    Set(Combo),
}

struct Shared {
    tx: Sender<HookEvent>,
    /// Wakes the UI. Asking egui for a repaint takes its lock, which the UI
    /// may hold for a while; the hook callback must never wait on it.
    wake: Sender<()>,
}

/// The shortcut settings the hook works from.
struct Config {
    bindings: Vec<(HotkeySlot, Combo)>,
    double_tap: Duration,
    ignored_apps: Vec<String>,
    ignore_fullscreen: bool,
    popup_ctrl_c: PopupCtrlC,
    copy_twice: bool,
}

static SHARED: OnceLock<Shared> = OnceLock::new();
static CONFIG: Mutex<Option<Config>> = Mutex::new(None);
/// Shortcut matching state (taps and first presses of ×2 chords).
static MATCHER: Mutex<Matcher> = Mutex::new(Matcher::new());
/// Set while Settings records a shortcut.
static RECORDER: Mutex<Option<Recorder>> = Mutex::new(None);
/// Executable of the current foreground window, updated on every
/// foreground change so the shortcut path doesn't open processes.
static FOREGROUND_EXE: Mutex<String> = Mutex::new(String::new());

pub static POPUP_VISIBLE: AtomicBool = AtomicBool::new(false);
/// One of the popup, Quick or Ultra windows is open: Esc closes it from
/// anywhere (it may not have the keyboard focus). Never set otherwise, so
/// Esc is left alone when none of them is open.
pub static ESC_CLOSES: AtomicBool = AtomicBool::new(false);
pub static PAUSED: AtomicBool = AtomicBool::new(false);
/// Last foreground window that isn't ours or the taskbar, so "Translate
/// selection" from the tray can go back to it.
pub static LAST_APP: AtomicIsize = AtomicIsize::new(0);

/// A tap is a press+release shorter than this, with nothing else pressed.
const TAP_MAX: Duration = Duration::from_millis(280);
/// Tick (GetTickCount) of the last event the hooks saw.
static LAST_HOOK_TICK: AtomicU32 = AtomicU32::new(0);
/// The hook thread, for the watchdog's "install again" message.
static HOOK_THREAD: AtomicU32 = AtomicU32::new(0);
/// Thread message: install the hooks again.
const WM_REINSTALL: u32 = WM_APP + 1;

/// Applies the shortcut-related settings.
pub fn configure(s: &Settings) {
    let mut bindings = vec![(HotkeySlot::Translate, s.shortcut.as_combo())];
    for slot in [
        HotkeySlot::Quick,
        HotkeySlot::Ultra,
        HotkeySlot::ScreenArea,
        HotkeySlot::WholeScreen,
        HotkeySlot::MainWindow,
        HotkeySlot::TranslatePaste,
    ] {
        if let Some(c) = s.hotkeys.get(slot) {
            bindings.push((slot, c));
        }
    }
    *CONFIG.lock().unwrap() = Some(Config {
        bindings,
        double_tap: Duration::from_millis(s.double_tap_ms as u64),
        ignored_apps: s.ignored_apps.clone(),
        ignore_fullscreen: s.ignore_fullscreen,
        popup_ctrl_c: s.popup_ctrl_c,
        copy_twice: s.copy_twice,
    });
}

/// Starts recording a shortcut (Settings → Hotkeys); the result arrives
/// as [`HookEvent::Captured`]. `double_tap` is the ×2 interval.
pub fn start_capture(double_tap: Duration) {
    *RECORDER.lock().unwrap() = Some(Recorder::new(double_tap));
}

pub fn stop_capture() {
    *RECORDER.lock().unwrap() = None;
}

/// A key the Settings window saw itself while recording (backup for when
/// the hook doesn't get the keys): same state machine as the hook.
pub fn capture_key(vk: u32, mods: Combo) {
    let result = {
        let mut rec = RECORDER.lock().unwrap();
        let Some(r) = rec.as_mut() else { return };
        let out = r.key(vk, true, mods, Instant::now()).0;
        if out.is_some() {
            *rec = None;
        }
        out
    };
    after_capture_step(result);
}

/// Shortcuts are off in the foreground app: it is on the ignore list, or
/// it is fullscreen and the user asked to ignore fullscreen apps.
fn blocked(config: &Config) -> bool {
    let exe = FOREGROUND_EXE.lock().unwrap();
    let norm = |s: &str| s.trim().to_lowercase().trim_end_matches(".exe").to_owned();
    let fg = norm(&exe);
    if !fg.is_empty() && config.ignored_apps.iter().any(|a| norm(a) == fg) {
        return true;
    }
    config.ignore_fullscreen && apps::is_fullscreen(super::foreground())
}

fn send(ev: HookEvent) {
    if let Some(s) = SHARED.get() {
        let _ = s.tx.send(ev);
        let _ = s.wake.send(());
    }
}

/// A recording step either finished (send it) or is waiting for a possible
/// second tap of the same chord: check again once the interval has passed.
fn after_capture_step(result: Option<Capture>) {
    match result {
        Some(c) => send(HookEvent::Captured(c)),
        None => {
            let wait = RECORDER.lock().unwrap().as_ref().and_then(Recorder::pending_deadline);
            if let Some(deadline) = wait {
                std::thread::spawn(move || {
                    std::thread::sleep(deadline.saturating_duration_since(Instant::now()) + Duration::from_millis(15));
                    let done = {
                        let mut rec = RECORDER.lock().unwrap();
                        let out = rec.as_mut().and_then(|r| r.timeout(Instant::now()));
                        if out.is_some() {
                            *rec = None;
                        }
                        out
                    };
                    if let Some(c) = done {
                        send(HookEvent::Captured(c));
                    }
                });
            }
        }
    }
}

pub fn start(tx: Sender<HookEvent>, ctx: egui::Context) {
    let (wake, woken) = channel::<()>();
    if SHARED.set(Shared { tx, wake }).is_err() {
        return;
    }
    std::thread::Builder::new()
        .name("input-hook-wake".into())
        .spawn(move || {
            while woken.recv().is_ok() {
                while woken.try_recv().is_ok() {}
                ctx.request_repaint();
            }
        })
        .expect("spawn waker thread");
    std::thread::Builder::new()
        .name("input-hook".into())
        .spawn(|| unsafe {
            let install = || {
                let module = GetModuleHandleW(None).ok();
                let instance = module.map(|m| m.into());
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
                    // Install fresh hooks first, then drop the old ones, so
                    // no key slips through in between.
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
    std::thread::Builder::new()
        .name("input-hook-watchdog".into())
        .spawn(watchdog)
        .expect("spawn hook watchdog");
}

/// Re-installs the hooks when the system saw input that they didn't.
fn watchdog() {
    let mut last_reinstall = Instant::now();
    loop {
        std::thread::sleep(Duration::from_millis(1000));
        let mut info = LASTINPUTINFO { cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32, dwTime: 0 };
        if !unsafe { GetLastInputInfo(&mut info) }.as_bool() {
            continue;
        }
        let now = unsafe { GetTickCount() };
        let seen = LAST_HOOK_TICK.load(Ordering::Relaxed);
        // Input at least 300 ms old (time to be delivered) that is newer
        // than anything the hooks saw.
        let missed = info.dwTime.wrapping_sub(seen) as i32 > 200 && now.wrapping_sub(info.dwTime) > 300;
        let thread = HOOK_THREAD.load(Ordering::Relaxed);
        if missed && thread != 0 && last_reinstall.elapsed() > Duration::from_secs(2) {
            if debug() {
                eprintln!("hook: input the hooks missed; installing them again");
            }
            last_reinstall = Instant::now();
            unsafe {
                let _ = PostThreadMessageW(thread, WM_REINSTALL, WPARAM(0), LPARAM(0));
            }
        }
    }
}

fn is_ctrl(vk: u32) -> bool {
    [VK_CONTROL, VK_LCONTROL, VK_RCONTROL].iter().any(|k| k.0 as u32 == vk)
}

fn is_alt(vk: u32) -> bool {
    [VK_MENU, VK_LMENU, VK_RMENU].iter().any(|k| k.0 as u32 == vk)
}

fn is_shift(vk: u32) -> bool {
    [VK_SHIFT, VK_LSHIFT, VK_RSHIFT].iter().any(|k| k.0 as u32 == vk)
}

fn is_modifier(vk: u32) -> bool {
    is_ctrl(vk) || is_alt(vk) || is_shift(vk) || [VK_LWIN, VK_RWIN].iter().any(|k| k.0 as u32 == vk)
}

/// The generic code of a modifier that can be double-tapped (Win can't).
fn tap_kind(vk: u32) -> Option<u32> {
    if is_ctrl(vk) {
        Some(settings::VK_CONTROL)
    } else if is_alt(vk) {
        Some(settings::VK_MENU)
    } else if is_shift(vk) {
        Some(settings::VK_SHIFT)
    } else {
        None
    }
}

fn down(vk: windows::Win32::UI::Input::KeyboardAndMouse::VIRTUAL_KEY) -> bool {
    unsafe { GetAsyncKeyState(vk.0 as i32) < 0 }
}

/// The modifiers held right now, with `vk` as the key. AltGr (right Alt on
/// some layouts) arrives as a fake left Ctrl plus right Alt, so it reads as
/// Ctrl + Alt, which is what it is.
fn current_mods(vk: u32) -> Combo {
    Combo {
        ctrl: down(VK_CONTROL),
        shift: down(VK_SHIFT),
        alt: down(VK_MENU),
        win: down(VK_LWIN) || down(VK_RWIN),
        vk,
        double: false,
    }
}

/// A chord that can be a shortcut: at least Ctrl, Alt or Win, or a
/// function key or mouse side button (alone or with Shift).
fn bindable(c: &Combo) -> bool {
    c.ctrl || c.alt || c.win || (0x70..=0x87).contains(&c.vk) || settings::is_mouse_button(c.vk)
}

/// Double taps of lone modifiers: one tracker per modifier kind.
#[derive(Clone, Copy, Default)]
struct Tap {
    held: bool,
    /// Another key/button was used while the modifier was held.
    dirty: bool,
    pressed_at: Option<Instant>,
    /// Press time of the first completed tap.
    first_tap: Option<Instant>,
}

struct Taps([Tap; 3]);

impl Taps {
    const fn new() -> Self {
        let t = Tap { held: false, dirty: false, pressed_at: None, first_tap: None };
        Taps([t; 3])
    }

    fn index(kind: u32) -> usize {
        match kind {
            settings::VK_CONTROL => 0,
            settings::VK_MENU => 1,
            _ => 2,
        }
    }

    /// Something other than a lone modifier happened: no tap in progress counts.
    fn interrupt(&mut self) {
        for t in &mut self.0 {
            if t.held {
                t.dirty = true;
            }
            t.first_tap = None;
        }
    }

    /// A modifier event; returns the kind when it completes a double tap
    /// (two taps within `window`).
    fn event(&mut self, vk: u32, key_down: bool, now: Instant, window: Duration) -> Option<u32> {
        let kind = tap_kind(vk)?;
        // Pressing one modifier interrupts taps of the others.
        for (i, t) in self.0.iter_mut().enumerate() {
            if i != Self::index(kind) && key_down {
                if t.held {
                    t.dirty = true;
                }
                t.first_tap = None;
            }
        }
        let t = &mut self.0[Self::index(kind)];
        if key_down {
            if !t.held {
                t.held = true;
                t.dirty = false;
                t.pressed_at = Some(now);
            }
            return None;
        }
        if !t.held {
            return None;
        }
        t.held = false;
        let pressed = t.pressed_at.take().unwrap_or(now);
        if t.dirty || now - pressed > TAP_MAX {
            t.first_tap = None;
            return None;
        }
        match t.first_tap {
            Some(first) if now - first <= window => {
                t.first_tap = None;
                Some(kind)
            }
            _ => {
                t.first_tap = Some(pressed);
                None
            }
        }
    }
}

/// Live shortcut matching.
struct Matcher {
    taps: Taps,
    /// First press of a chord that is bound with ×2.
    first_press: Option<(Combo, Instant)>,
    /// Ctrl+C ×2: time of the last Ctrl+C, and whether C is still held
    /// (holding it auto-repeats; that is not a second press).
    last_copy: Option<Instant>,
    c_held: bool,
    /// Mouse side buttons whose press was swallowed: their release must be
    /// swallowed too (Windows turns the release into Back / Forward).
    swallowed_buttons: [bool; 2],
}

impl Matcher {
    const fn new() -> Self {
        Matcher { taps: Taps::new(), first_press: None, last_copy: None, c_held: false, swallowed_buttons: [false; 2] }
    }

    /// Ctrl+C (exactly; auto-repeat doesn't count). True when it is the
    /// second one within `window`.
    fn copy(&mut self, vk: u32, key_down: bool, held: Combo, now: Instant, window: Duration) -> bool {
        const C: u32 = 0x43;
        if vk != C {
            return false;
        }
        if !key_down {
            self.c_held = false;
            return false;
        }
        if std::mem::replace(&mut self.c_held, true) {
            return false;
        }
        if held != (Combo { ctrl: true, vk: C, ..Default::default() }) {
            self.last_copy = None;
            return false;
        }
        match self.last_copy {
            Some(t) if now - t <= window => {
                self.last_copy = None;
                true
            }
            _ => {
                self.last_copy = Some(now);
                false
            }
        }
    }

    /// One key event. Returns the shortcut it completes (if any) and
    /// whether to swallow the key.
    fn key(&mut self, vk: u32, key_down: bool, held: Combo, now: Instant, bindings: &[(HotkeySlot, Combo)], window: Duration) -> (Option<HotkeySlot>, bool) {
        if is_modifier(vk) {
            let hit = self.taps.event(vk, key_down, now, window).and_then(|kind| {
                let tap = Combo::modifier_tap(kind);
                bindings.iter().find(|(_, c)| *c == tap).map(|(s, _)| *s)
            });
            return (hit, false);
        }
        if !key_down {
            return (None, false);
        }
        self.taps.interrupt();
        let chord = Combo { double: false, ..held };
        if let Some((slot, _)) = bindings.iter().find(|(_, c)| *c == chord) {
            self.first_press = None;
            return (Some(*slot), true);
        }
        let twice = Combo { double: true, ..chord };
        if let Some((slot, _)) = bindings.iter().find(|(_, c)| *c == twice) {
            return match self.first_press {
                Some((c, t)) if c == chord && now - t <= window => {
                    self.first_press = None;
                    (Some(*slot), true)
                }
                // The first of two presses: keep it from the app underneath.
                _ => {
                    self.first_press = Some((chord, now));
                    (None, true)
                }
            };
        }
        self.first_press = None;
        (None, false)
    }

    /// A mouse button: interrupts taps (Ctrl+click is not a tap).
    fn mouse(&mut self) {
        self.taps.interrupt();
        self.first_press = None;
    }
}

/// Recording a new shortcut. A chord is kept pending for the double-press
/// interval: pressing it again makes it a ×2 binding, otherwise it is
/// taken as is ([`Recorder::timeout`]). Tapping a lone Ctrl, Alt or Shift
/// twice records that double tap. Esc cancels, Backspace/Delete clears.
pub struct Recorder {
    window: Duration,
    pending: Option<(Combo, Instant)>,
    taps: Taps,
}

impl Recorder {
    pub fn new(window: Duration) -> Self {
        Recorder { window, pending: None, taps: Taps::new() }
    }

    /// When the pending chord becomes final if not pressed again.
    fn pending_deadline(&self) -> Option<Instant> {
        self.pending.map(|(_, t)| t + self.window)
    }

    /// One key event while recording: the result (when finished) and
    /// whether to swallow the key.
    pub fn key(&mut self, vk: u32, key_down: bool, held: Combo, now: Instant) -> (Option<Capture>, bool) {
        if is_modifier(vk) {
            let tap = self.taps.event(vk, key_down, now, self.window);
            return match tap {
                Some(kind) if self.pending.is_none() => (Some(Capture::Set(Combo::modifier_tap(kind))), false),
                _ => (None, false),
            };
        }
        if !key_down {
            // Swallow the releases too, so the app doesn't see half a key.
            return (None, true);
        }
        self.taps.interrupt();
        let plain = !(held.ctrl || held.alt || held.win || held.shift);
        if vk == VK_ESCAPE.0 as u32 && plain {
            return (Some(Capture::Cancel), true);
        }
        if (vk == VK_BACK.0 as u32 || vk == VK_DELETE.0 as u32) && plain {
            return (Some(Capture::Clear), true);
        }
        let chord = Combo { vk, double: false, ..held };
        if !bindable(&chord) {
            return (None, true);
        }
        match self.pending {
            Some((c, t)) if c == chord && now - t <= self.window => {
                self.pending = None;
                (Some(Capture::Set(Combo { double: true, ..chord })), true)
            }
            _ => {
                self.pending = Some((chord, now));
                (None, true)
            }
        }
    }

    /// The pending chord, once the interval for a second press is over.
    pub fn timeout(&mut self, now: Instant) -> Option<Capture> {
        match self.pending {
            Some((c, t)) if now - t > self.window => {
                self.pending = None;
                Some(Capture::Set(c))
            }
            _ => None,
        }
    }
}

/// `HELILINGO_ALLOW_INJECTED=1` (testing only): treat synthetic keys like
/// real ones, so the shortcut and recording paths can be driven with
/// SendInput. Normally injected keys are ignored, so the app's own
/// synthetic Ctrl+C / Ctrl+V can never trigger it.
fn allow_injected() -> bool {
    static ALLOW: OnceLock<bool> = OnceLock::new();
    *ALLOW.get_or_init(|| std::env::var("HELILINGO_ALLOW_INJECTED").is_ok_and(|v| v == "1"))
}

/// `HELILINGO_DEBUG_HOOK=1`: print what the hook sees and decides to
/// stderr (for diagnosing shortcuts that don't fire).
fn debug() -> bool {
    static DEBUG: OnceLock<bool> = OnceLock::new();
    *DEBUG.get_or_init(|| std::env::var("HELILINGO_DEBUG_HOOK").is_ok_and(|v| v == "1"))
}

/// Returns true to swallow the key.
fn handle_key(vk: u32, key_down: bool) -> bool {
    let now = Instant::now();
    let held = current_mods(vk);

    // Recording a shortcut takes every key.
    {
        let mut rec = RECORDER.lock().unwrap();
        if let Some(r) = rec.as_mut() {
            let (result, swallow) = r.key(vk, key_down, held, now);
            if debug() && key_down {
                eprintln!("hook: recording key {vk:#x} held {} -> {result:?}", held.label());
            }
            if result.is_some() {
                *rec = None;
            }
            drop(rec);
            after_capture_step(result);
            return swallow;
        }
    }

    let plain = !(held.ctrl || held.alt || held.shift || held.win);
    if vk == VK_ESCAPE.0 as u32 && plain && ESC_CLOSES.load(Ordering::Relaxed) {
        if key_down {
            send(HookEvent::Escape);
        }
        return true;
    }

    let guard = CONFIG.lock().unwrap();
    let Some(config) = guard.as_ref() else { return false };
    let mut matcher = MATCHER.lock().unwrap();

    // Ctrl+C while the popup is open copies the translation instead of the
    // selection underneath (Settings → "Ctrl+C in the popup").
    let c_key = 0x43;
    if key_down
        && vk == c_key
        && POPUP_VISIBLE.load(Ordering::Relaxed)
        && config.popup_ctrl_c != PopupCtrlC::Ignore
        && held == (Combo { ctrl: true, vk, ..Default::default() })
    {
        // Ctrl+C is not a Ctrl tap.
        matcher.taps.interrupt();
        send(HookEvent::PopupCopy);
        return true;
    }

    // Ctrl+C ×2: the app copies as usual both times (never swallowed);
    // the second one translates what was copied.
    if config.copy_twice
        && matcher.copy(vk, key_down, held, now, config.double_tap)
        && !PAUSED.load(Ordering::Relaxed)
        && !blocked(config)
    {
        matcher.taps.interrupt();
        send(HookEvent::Trigger(HotkeySlot::CopyTwice));
        return false;
    }

    let (hit, swallow) = matcher.key(vk, key_down, held, now, &config.bindings, config.double_tap);
    if debug() && key_down {
        eprintln!(
            "hook: key {vk:#x} held {} -> {hit:?} (paused {}, blocked {}, foreground {:?})",
            held.label(),
            PAUSED.load(Ordering::Relaxed),
            blocked(config),
            FOREGROUND_EXE.lock().unwrap(),
        );
    }
    match hit {
        Some(slot) if !PAUSED.load(Ordering::Relaxed) && !blocked(config) => {
            crate::timing(&format!("hook: trigger {slot:?}"));
            send(HookEvent::Trigger(slot));
            // A modifier double tap is never swallowed (the modifier keys
            // must reach the app); a chord is, so the app doesn't act on it.
            swallow
        }
        Some(_) => false,
        None => swallow && !PAUSED.load(Ordering::Relaxed) && !blocked(config),
    }
}

unsafe extern "system" fn keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    LAST_HOOK_TICK.store(unsafe { GetTickCount() }, Ordering::Relaxed);
    if code == HC_ACTION as i32 {
        let k = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
        let injected = k.flags.contains(LLKHF_INJECTED) && !allow_injected();
        let key_down = matches!(wparam.0 as u32, WM_KEYDOWN | WM_SYSKEYDOWN);
        if !injected && handle_key(k.vkCode, key_down) {
            return LRESULT(1);
        }
    }
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

/// A mouse side button, run through the same path as keys. Returns true
/// to swallow it (bound, or being recorded).
fn handle_side_button(vk: u32, down: bool) -> bool {
    let i = (vk - settings::VK_XBUTTON1) as usize;
    if !down {
        let swallowed = std::mem::take(&mut MATCHER.lock().unwrap().swallowed_buttons[i]);
        // During recording the recorder swallows releases itself.
        return handle_key(vk, false) || swallowed;
    }
    let swallow = handle_key(vk, true);
    MATCHER.lock().unwrap().swallowed_buttons[i] = swallow;
    swallow
}

unsafe extern "system" fn mouse_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    LAST_HOOK_TICK.store(unsafe { GetTickCount() }, Ordering::Relaxed);
    if code == HC_ACTION as i32 && matches!(wparam.0 as u32, WM_XBUTTONDOWN | WM_XBUTTONUP) {
        let m = unsafe { &*(lparam.0 as *const MSLLHOOKSTRUCT) };
        let injected = m.flags & LLMHF_INJECTED != 0 && !allow_injected();
        // HIWORD(mouseData): 1 = XBUTTON1 (Mouse 4), 2 = XBUTTON2 (Mouse 5).
        let vk = match m.mouseData >> 16 {
            1 => Some(settings::VK_XBUTTON1),
            2 => Some(settings::VK_XBUTTON2),
            _ => None,
        };
        if let Some(vk) = vk.filter(|_| !injected) {
            if handle_side_button(vk, wparam.0 as u32 == WM_XBUTTONDOWN) {
                return LRESULT(1);
            }
        }
    }
    if code == HC_ACTION as i32
        && matches!(wparam.0 as u32, WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN)
    {
        MATCHER.lock().unwrap().mouse();
        if POPUP_VISIBLE.load(Ordering::Relaxed) {
            let m = unsafe { &*(lparam.0 as *const MSLLHOOKSTRUCT) };
            send(HookEvent::MouseDown { x: m.pt.x, y: m.pt.y });
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
    if hwnd.is_invalid() || super::is_own_window(hwnd) {
        return;
    }
    let class = super::class_name(hwnd);
    let shell = [
        "Shell_TrayWnd",
        "Shell_SecondaryTrayWnd",
        "NotifyIconOverflowWindow",
        "TopLevelWindowForOverflowXamlIsland",
        "Windows.UI.Core.CoreWindow",
        "XamlExplorerHostIslandWindow",
    ];
    if !shell.contains(&class.as_str()) {
        LAST_APP.store(hwnd.0 as isize, Ordering::Relaxed);
    }
    *FOREGROUND_EXE.lock().unwrap() = apps::exe_of_window(Hwnd::from_raw(hwnd)).unwrap_or_default();
}

#[cfg(test)]
mod tests {
    use super::*;

    const CTRL: u32 = 0xA2;
    const ALT: u32 = 0xA4;
    const SHIFT: u32 = 0xA0;
    const A: u32 = 0x41;
    const W: Duration = Duration::from_millis(400);

    fn ms(t0: Instant, n: u64) -> Instant {
        t0 + Duration::from_millis(n)
    }

    fn ca(vk: u32) -> Combo {
        Combo { ctrl: true, alt: true, vk, ..Default::default() }
    }

    #[test]
    fn records_plain_chord_after_the_interval() {
        let t0 = Instant::now();
        let mut r = Recorder::new(W);
        assert_eq!(r.key(CTRL, true, Combo::default(), t0), (None, false));
        assert_eq!(r.key(ALT, true, ca(ALT), ms(t0, 10)), (None, false));
        assert_eq!(r.key(A, true, ca(A), ms(t0, 20)), (None, true));
        assert_eq!(r.key(A, false, ca(A), ms(t0, 60)), (None, true));
        assert_eq!(r.timeout(ms(t0, 300)), None);
        assert_eq!(r.timeout(ms(t0, 450)), Some(Capture::Set(ca(A))));
    }

    #[test]
    fn records_chord_with_double_tapped_key() {
        let t0 = Instant::now();
        let mut r = Recorder::new(W);
        r.key(A, true, ca(A), t0);
        r.key(A, false, ca(A), ms(t0, 50));
        let (out, swallow) = r.key(A, true, ca(A), ms(t0, 200));
        assert!(swallow);
        assert_eq!(out, Some(Capture::Set(Combo { double: true, ..ca(A) })));
    }

    #[test]
    fn records_modifier_double_taps() {
        let t0 = Instant::now();
        for (vk, kind) in [(SHIFT, settings::VK_SHIFT), (CTRL, settings::VK_CONTROL), (ALT, settings::VK_MENU)] {
            let mut r = Recorder::new(W);
            r.key(vk, true, Combo::default(), t0);
            r.key(vk, false, Combo::default(), ms(t0, 80));
            r.key(vk, true, Combo::default(), ms(t0, 200));
            let (out, swallow) = r.key(vk, false, Combo::default(), ms(t0, 260));
            assert!(!swallow);
            assert_eq!(out, Some(Capture::Set(Combo::modifier_tap(kind))));
        }
    }

    #[test]
    fn esc_cancels_backspace_clears_letters_ignored() {
        let t0 = Instant::now();
        let mut r = Recorder::new(W);
        assert_eq!(r.key(A, true, Combo { vk: A, ..Default::default() }, t0), (None, true));
        assert_eq!(r.key(0x1B, true, Combo { vk: 0x1B, ..Default::default() }, t0).0, Some(Capture::Cancel));
        let mut r = Recorder::new(W);
        assert_eq!(r.key(0x08, true, Combo { vk: 0x08, ..Default::default() }, t0).0, Some(Capture::Clear));
        // F-keys are allowed alone.
        let mut r = Recorder::new(W);
        r.key(0x74, true, Combo { vk: 0x74, ..Default::default() }, t0);
        assert_eq!(r.timeout(ms(t0, 500)), Some(Capture::Set(Combo { vk: 0x74, ..Default::default() })));
    }

    #[test]
    fn mouse_side_buttons_bind_alone_and_match() {
        let t0 = Instant::now();
        let m4 = Combo { vk: settings::VK_XBUTTON1, ..Default::default() };
        let mut r = Recorder::new(W);
        assert_eq!(r.key(settings::VK_XBUTTON1, true, m4, t0), (None, true));
        assert_eq!(r.timeout(ms(t0, 500)), Some(Capture::Set(m4)));
        let bindings = [(HotkeySlot::TranslatePaste, m4)];
        let mut m = Matcher::new();
        assert_eq!(m.key(settings::VK_XBUTTON1, true, m4, t0, &bindings, W), (Some(HotkeySlot::TranslatePaste), true));
        assert_eq!(m4.label(), "Mouse 4");
    }

    #[test]
    fn ctrl_c_twice() {
        let t0 = Instant::now();
        let cc = Combo { ctrl: true, vk: 0x43, ..Default::default() };
        let mut m = Matcher::new();
        assert!(!m.copy(0x43, true, cc, t0, W));
        // Held C auto-repeats: not a second press.
        assert!(!m.copy(0x43, true, cc, ms(t0, 40), W));
        assert!(!m.copy(0x43, false, cc, ms(t0, 80), W));
        assert!(m.copy(0x43, true, cc, ms(t0, 200), W));
        // Too slow, or a plain C, doesn't count.
        let mut m = Matcher::new();
        m.copy(0x43, true, cc, t0, W);
        m.copy(0x43, false, cc, ms(t0, 50), W);
        assert!(!m.copy(0x43, true, cc, ms(t0, 900), W));
        let mut m = Matcher::new();
        m.copy(0x43, true, cc, t0, W);
        m.copy(0x43, false, cc, ms(t0, 50), W);
        assert!(!m.copy(0x43, true, Combo { vk: 0x43, ..Default::default() }, ms(t0, 150), W));
    }

    #[test]
    fn altgr_records_as_ctrl_alt() {
        // Right Alt on AltGr layouts: fake LCtrl + RAlt held.
        let t0 = Instant::now();
        let mut r = Recorder::new(W);
        r.key(A, true, ca(A), t0);
        assert_eq!(r.timeout(ms(t0, 500)), Some(Capture::Set(ca(A))));
    }

    #[test]
    fn matches_chords_double_chords_and_taps() {
        let t0 = Instant::now();
        let bindings = [
            (HotkeySlot::Quick, ca(0x54)),
            (HotkeySlot::Ultra, Combo { double: true, ..ca(A) }),
            (HotkeySlot::Translate, Combo::modifier_tap(settings::VK_CONTROL)),
        ];
        let mut m = Matcher::new();
        assert_eq!(m.key(0x54, true, ca(0x54), t0, &bindings, W), (Some(HotkeySlot::Quick), true));
        // ×2: first press swallowed, second fires.
        assert_eq!(m.key(A, true, ca(A), ms(t0, 10), &bindings, W), (None, true));
        assert_eq!(m.key(A, true, ca(A), ms(t0, 200), &bindings, W), (Some(HotkeySlot::Ultra), true));
        // Too slow: two first presses.
        assert_eq!(m.key(A, true, ca(A), ms(t0, 1000), &bindings, W), (None, true));
        assert_eq!(m.key(A, true, ca(A), ms(t0, 1600), &bindings, W), (None, true));
        // Ctrl ×2.
        let mut m = Matcher::new();
        let none = Combo::default();
        m.key(CTRL, true, none, t0, &bindings, W);
        m.key(CTRL, false, none, ms(t0, 50), &bindings, W);
        m.key(CTRL, true, none, ms(t0, 150), &bindings, W);
        assert_eq!(m.key(CTRL, false, none, ms(t0, 200), &bindings, W), (Some(HotkeySlot::Translate), false));
        // Ctrl+C in between is not a tap.
        let mut m = Matcher::new();
        m.key(CTRL, true, none, t0, &bindings, W);
        m.key(0x43, true, Combo { ctrl: true, vk: 0x43, ..Default::default() }, ms(t0, 20), &bindings, W);
        m.key(CTRL, false, none, ms(t0, 60), &bindings, W);
        m.key(CTRL, true, none, ms(t0, 150), &bindings, W);
        assert_eq!(m.key(CTRL, false, none, ms(t0, 200), &bindings, W).0, None);
    }
}
