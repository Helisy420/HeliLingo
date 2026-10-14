//! The selected text through UI Automation: no synthetic Ctrl+C, the
//! clipboard isn't touched. Works where the focused control exposes the
//! Text pattern (Notepad, Word, Windows Terminal, most browsers' pages);
//! elsewhere the caller falls back to copying.
//!
//! UI Automation calls go to the other app and can hang when it does, so
//! they run on one long-lived thread (COM initialised once, the automation
//! object reused) and the caller waits a short time for the answer.

use std::sync::mpsc::{Sender, channel};
use std::sync::{LazyLock, Mutex};
use std::time::Duration;

use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx};
use windows::Win32::UI::Accessibility::{CUIAutomation, IUIAutomation, IUIAutomationTextPattern, UIA_TextPatternId};

type Reply = Sender<Option<String>>;

static WORKER: LazyLock<Mutex<Option<Sender<Reply>>>> = LazyLock::new(|| Mutex::new(spawn()));

fn spawn() -> Option<Sender<Reply>> {
    let (tx, rx) = channel::<Reply>();
    std::thread::Builder::new()
        .name("uia".into())
        .spawn(move || {
            // SAFETY: this thread's own COM apartment.
            let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
            let automation: Option<IUIAutomation> = unsafe { CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER).ok() };
            for reply in rx {
                let text = automation.as_ref().and_then(selection);
                let _ = reply.send(text);
            }
        })
        .ok()?;
    Some(tx)
}

fn selection(automation: &IUIAutomation) -> Option<String> {
    unsafe {
        let focused = automation.GetFocusedElement().ok()?;
        let pattern: IUIAutomationTextPattern = focused.GetCurrentPatternAs(UIA_TextPatternId).ok()?;
        let ranges = pattern.GetSelection().ok()?;
        let mut parts = Vec::new();
        for i in 0..ranges.Length().ok()? {
            let text = ranges.GetElement(i).ok()?.GetText(100_000).ok()?.to_string();
            if !text.is_empty() {
                parts.push(text);
            }
        }
        let text = parts.join("\n");
        (!text.trim().is_empty()).then_some(text)
    }
}

/// The focused control's selected text, or `None` (no Text pattern, empty
/// selection, or the app didn't answer within `timeout`).
pub fn selected_text(timeout: Duration) -> Option<String> {
    let (tx, rx) = channel();
    {
        let mut worker = WORKER.lock().unwrap();
        let sent = worker.as_ref().is_some_and(|w| w.send(tx.clone()).is_ok());
        if !sent {
            // The thread died (or never started): try a fresh one next time.
            *worker = spawn();
            return None;
        }
    }
    match rx.recv_timeout(timeout) {
        Ok(text) => text,
        Err(_) => {
            // Stuck on a hung app: leave it, start a new thread for later calls.
            *WORKER.lock().unwrap() = spawn();
            None
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn answers_without_panicking() {
        // Whatever has focus in the test session; only the call path matters.
        let _ = super::selected_text(std::time::Duration::from_millis(500));
    }
}
