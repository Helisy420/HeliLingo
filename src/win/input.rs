//! Synthetic keyboard input (copy/paste chords) and key naming.

use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, GetKeyNameTextW, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBD_EVENT_FLAGS,
    KEYBDINPUT, KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, MAPVK_VK_TO_VSC, MapVirtualKeyW,
    SendInput, VIRTUAL_KEY, VK_C, VK_CONTROL, VK_INSERT, VK_LCONTROL, VK_LMENU, VK_LSHIFT,
    VK_LWIN, VK_RCONTROL, VK_RMENU, VK_RSHIFT, VK_RWIN, VK_SHIFT, VK_V,
};

/// Unassigned virtual key, injected to stop a lone Alt/Win release from
/// opening the menu bar / Start menu.
const VK_DUMMY: VIRTUAL_KEY = VIRTUAL_KEY(0xE8);

fn key(vk: VIRTUAL_KEY, up: bool) -> INPUT {
    let mut flags = if up { KEYEVENTF_KEYUP } else { KEYBD_EVENT_FLAGS(0) };
    if vk == VK_INSERT {
        flags |= KEYEVENTF_EXTENDEDKEY;
    }
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

fn send(inputs: &[INPUT]) {
    if inputs.is_empty() {
        return;
    }
    unsafe {
        SendInput(inputs, std::mem::size_of::<INPUT>() as i32);
    }
}

fn is_down(vk: VIRTUAL_KEY) -> bool {
    unsafe { GetAsyncKeyState(vk.0 as i32) < 0 }
}

/// Releases modifiers the user may still be holding (e.g. from
/// Ctrl+Shift+T), so our Ctrl+C doesn't turn into Ctrl+Shift+C.
pub fn release_modifiers() {
    let mut inputs = Vec::new();
    let alt_or_win = [VK_LMENU, VK_RMENU, VK_LWIN, VK_RWIN].into_iter().any(is_down);
    if alt_or_win {
        inputs.push(key(VK_DUMMY, false));
        inputs.push(key(VK_DUMMY, true));
    }
    for vk in [VK_LSHIFT, VK_RSHIFT, VK_LMENU, VK_RMENU, VK_LWIN, VK_RWIN, VK_LCONTROL, VK_RCONTROL] {
        if is_down(vk) {
            inputs.push(key(vk, true));
        }
    }
    send(&inputs);
}

fn chord(modifier: VIRTUAL_KEY, k: VIRTUAL_KEY) {
    send(&[key(modifier, false), key(k, false), key(k, true), key(modifier, true)]);
}

pub fn send_copy(terminal: bool) {
    if terminal {
        chord(VK_CONTROL, VK_INSERT);
    } else {
        chord(VK_CONTROL, VK_C);
    }
}

pub fn send_paste(terminal: bool) {
    if terminal {
        chord(VK_SHIFT, VK_INSERT);
    } else {
        chord(VK_CONTROL, VK_V);
    }
}

/// Human-readable key name for a virtual-key code ("T", "F5", "Space").
pub fn vk_name(vk: u32) -> String {
    match vk {
        0x04 => return "Mouse 3".into(),
        0x05 => return "Mouse 4".into(),
        0x06 => return "Mouse 5".into(),
        _ => {}
    }
    match vk {
        0x30..=0x39 | 0x41..=0x5A => return char::from_u32(vk).unwrap_or('?').to_string(),
        0x70..=0x87 => return format!("F{}", vk - 0x6F),
        _ => {}
    }
    unsafe {
        let scan = MapVirtualKeyW(vk, MAPVK_VK_TO_VSC);
        let mut lparam = (scan as i32) << 16;
        // Navigation keys live on the extended scan-code page.
        if matches!(vk, 0x21..=0x2E | 0x5B | 0x5C | 0x6F | 0x90) {
            lparam |= 1 << 24;
        }
        let mut buf = [0u16; 64];
        let n = GetKeyNameTextW(lparam, &mut buf);
        if n > 0 {
            return String::from_utf16_lossy(&buf[..n as usize]);
        }
    }
    format!("0x{vk:02X}")
}

/// The virtual-key code of an egui key, for recording shortcuts from the
/// Settings window's own key events (layout independent: callers pass the
/// physical key). Modifier keys and keys without a stable code give `None`.
pub fn egui_key_vk(key: egui::Key) -> Option<u32> {
    use egui::Key as K;
    let name = key.name();
    if name.len() == 1 && name.as_bytes()[0].is_ascii_uppercase() {
        return Some(name.as_bytes()[0] as u32);
    }
    Some(match key {
        K::Num0 | K::Num1 | K::Num2 | K::Num3 | K::Num4 | K::Num5 | K::Num6 | K::Num7 | K::Num8 | K::Num9 => {
            0x30 + name.trim_start_matches("Num").parse::<u32>().ok().unwrap_or(0)
        }
        K::Escape => 0x1B,
        K::Tab => 0x09,
        K::Backspace => 0x08,
        K::Enter => 0x0D,
        K::Space => 0x20,
        K::Insert => 0x2D,
        K::Delete => 0x2E,
        K::Home => 0x24,
        K::End => 0x23,
        K::PageUp => 0x21,
        K::PageDown => 0x22,
        K::ArrowLeft => 0x25,
        K::ArrowUp => 0x26,
        K::ArrowRight => 0x27,
        K::ArrowDown => 0x28,
        K::Minus => 0xBD,
        K::Equals | K::Plus => 0xBB,
        K::Comma => 0xBC,
        K::Period => 0xBE,
        K::Slash => 0xBF,
        K::Semicolon => 0xBA,
        K::Backtick => 0xC0,
        K::OpenBracket => 0xDB,
        K::Backslash => 0xDC,
        K::CloseBracket => 0xDD,
        K::Quote => 0xDE,
        _ => {
            let n = name.strip_prefix('F')?.parse::<u32>().ok().filter(|n| (1..=24).contains(n))?;
            0x6F + n
        }
    })
}
