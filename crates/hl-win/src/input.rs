//! Synthetic keyboard input: the paste chord.

use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBD_EVENT_FLAGS, KEYBDINPUT, KEYEVENTF_EXTENDEDKEY,
    KEYEVENTF_KEYUP, SendInput, VIRTUAL_KEY, VK_CONTROL, VK_INSERT, VK_LCONTROL, VK_LMENU, VK_LSHIFT, VK_LWIN,
    VK_C, VK_RCONTROL, VK_RMENU, VK_RSHIFT, VK_RWIN, VK_SHIFT, VK_V,
};

/// Unassigned virtual key, injected so a lone Alt/Win release doesn't open
/// the menu bar or the Start menu.
const VK_DUMMY: VIRTUAL_KEY = VIRTUAL_KEY(0xE8);

fn key(vk: VIRTUAL_KEY, up: bool) -> INPUT {
    let mut flags = if up { KEYEVENTF_KEYUP } else { KEYBD_EVENT_FLAGS(0) };
    if vk == VK_INSERT {
        flags |= KEYEVENTF_EXTENDEDKEY;
    }
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 { ki: KEYBDINPUT { wVk: vk, wScan: 0, dwFlags: flags, time: 0, dwExtraInfo: 0 } },
    }
}

fn send(inputs: &[INPUT]) {
    if !inputs.is_empty() {
        unsafe {
            SendInput(inputs, std::mem::size_of::<INPUT>() as i32);
        }
    }
}

fn is_down(vk: VIRTUAL_KEY) -> bool {
    unsafe { GetAsyncKeyState(vk.0 as i32) < 0 }
}

/// Releases modifiers the user may still hold (from the shortcut), so our
/// Ctrl+V doesn't become Ctrl+Shift+V.
pub fn release_modifiers() {
    let mut inputs = Vec::new();
    if [VK_LMENU, VK_RMENU, VK_LWIN, VK_RWIN].into_iter().any(is_down) {
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

/// Ctrl+V, or Shift+Insert in a terminal.
pub fn send_paste(terminal: bool) {
    let (modifier, k) = if terminal { (VK_SHIFT, VK_INSERT) } else { (VK_CONTROL, VK_V) };
    send(&[key(modifier, false), key(k, false), key(k, true), key(modifier, true)]);
}

/// Ctrl+C, or Ctrl+Insert in a terminal (Ctrl+C would interrupt there).
pub fn send_copy(terminal: bool) {
    let (modifier, k) = if terminal { (VK_CONTROL, VK_INSERT) } else { (VK_CONTROL, VK_C) };
    send(&[key(modifier, false), key(k, false), key(k, true), key(modifier, true)]);
}
