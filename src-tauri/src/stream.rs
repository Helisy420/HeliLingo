//! Translation events to a window: deltas are merged so the window gets at
//! most one message per frame, whatever the model's token rate.

use std::time::Duration;

use hl_core::TranslateEvent;
use tauri::ipc::Channel;
use tokio::sync::mpsc;

/// One 60 Hz frame.
const FRAME: Duration = Duration::from_millis(16);

/// Returns an `emit` function for the router. Events go through a small
/// task that merges consecutive `Delta`s arriving within one frame.
pub fn to_window(channel: Channel<TranslateEvent>) -> impl FnMut(TranslateEvent) + Send + 'static {
    let (tx, mut rx) = mpsc::unbounded_channel::<TranslateEvent>();
    tauri::async_runtime::spawn(async move {
        while let Some(ev) = rx.recv().await {
            let TranslateEvent::Delta { id, mut text } = ev else {
                send(&channel, ev);
                continue;
            };
            // Collect more deltas for the rest of this frame.
            let deadline = tokio::time::Instant::now() + FRAME;
            let mut after = None;
            while let Ok(Some(next)) = tokio::time::timeout_at(deadline, rx.recv()).await {
                match next {
                    TranslateEvent::Delta { text: more, .. } => text.push_str(&more),
                    other => {
                        after = Some(other);
                        break;
                    }
                }
            }
            send(&channel, TranslateEvent::Delta { id, text });
            if let Some(ev) = after {
                send(&channel, ev);
            }
        }
    });
    move |ev| {
        let _ = tx.send(ev);
    }
}

fn send(channel: &Channel<TranslateEvent>, ev: TranslateEvent) {
    if let Err(e) = channel.send(ev) {
        tracing::debug!("window channel closed: {e}");
    }
}
