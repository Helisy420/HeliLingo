//! Request ids and cancel-previous: every window ("surface") runs at most
//! one translation; starting a new one aborts the old one, and so does
//! closing the window.

use std::collections::HashMap;
use std::future::Future;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};
use tokio::task::AbortHandle;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RequestId(pub u64);

/// Where a translation shows up.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Surface {
    Popup,
    Quick,
    Translator,
    Ultra,
    /// Translate-and-paste: no window.
    Paste,
}

#[derive(Default)]
pub struct Requests {
    next: AtomicU64,
    running: Mutex<HashMap<Surface, (RequestId, AbortHandle)>>,
}

impl Requests {
    pub fn new() -> Self {
        Self::default()
    }

    /// Spawns `make(id)` for `surface`, aborting what ran there before.
    /// Must be called inside a Tokio runtime.
    pub fn begin<F, Fut>(&self, surface: Surface, make: F) -> RequestId
    where
        F: FnOnce(RequestId) -> Fut,
        Fut: Future<Output = ()> + Send + 'static,
    {
        let id = RequestId(self.next.fetch_add(1, Ordering::Relaxed) + 1);
        let handle = tokio::spawn(make(id)).abort_handle();
        let old = self.running.lock().unwrap().insert(surface, (id, handle));
        if let Some((_, old)) = old {
            old.abort();
        }
        id
    }

    /// Aborts what runs on `surface`, if anything.
    pub fn cancel(&self, surface: Surface) {
        if let Some((_, h)) = self.running.lock().unwrap().remove(&surface) {
            h.abort();
        }
    }

    /// Whether `id` is still the current request of its surface.
    pub fn is_current(&self, id: RequestId) -> bool {
        self.running.lock().unwrap().values().any(|(r, h)| *r == id && !h.is_finished())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;
    use std::time::Duration;

    #[tokio::test(start_paused = true)]
    async fn new_request_aborts_the_old_one() {
        let req = Requests::new();
        let first_done = Arc::new(AtomicBool::new(false));
        let flag = first_done.clone();
        let a = req.begin(Surface::Popup, move |_| async move {
            tokio::time::sleep(Duration::from_secs(1)).await;
            flag.store(true, Ordering::SeqCst);
        });
        let b = req.begin(Surface::Popup, |_| async {});
        assert_ne!(a, b);
        tokio::time::sleep(Duration::from_secs(2)).await;
        assert!(!first_done.load(Ordering::SeqCst));
    }
}
