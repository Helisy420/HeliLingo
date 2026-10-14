//! Ultra mode: a selection mixing languages is split into fragments (lines,
//! then sentences); each fragment's language is detected locally, fragments
//! already in the target language are left alone (if the setting says so),
//! and the rest are translated two at a time. Each fragment's result goes to
//! the window as soon as it's ready.

use std::sync::Arc;

use hl_core::lang::AUTO;
use hl_core::{RequestId, text};
use hl_pipeline::Request;
use serde::Serialize;
use tauri::ipc::Channel;
use tokio::sync::Semaphore;

use crate::state::AppState;

#[derive(Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum UltraEvent {
    /// The fragments, in order, before any translation.
    Split { fragments: Vec<Fragment>, target: String },
    Translated { index: usize, text: String, engine: String },
    Unchanged { index: usize },
    Failed { index: usize, error: hl_core::EngineError },
    Done,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Fragment {
    pub text: String,
    /// Whitespace after it in the selection (kept when rebuilding).
    pub sep: String,
    pub lang: Option<String>,
}

/// Translates `selection` fragment by fragment. Runs until done; the
/// caller spawns it (and aborts it to cancel).
pub async fn run(app: Arc<AppState>, selection: String, channel: Channel<UltraEvent>) {
    let (target, detect_each, skip_target, second) = {
        let s = app.settings.read().unwrap();
        (s.target_lang.clone(), s.ultra_detect_each, s.ultra_skip_target, s.second_lang.clone())
    };
    let segs = text::segments(&text::normalize(&selection));
    let fragments: Vec<Fragment> = segs
        .iter()
        .map(|s| Fragment {
            text: s.text.clone(),
            sep: s.sep.clone(),
            lang: if detect_each { hl_detect::detect(&s.text).map(str::to_owned) } else { None },
        })
        .collect();
    let _ = channel.send(UltraEvent::Split { fragments: fragments.clone(), target: target.clone() });

    let limit = Arc::new(Semaphore::new(2));
    let mut tasks = Vec::new();
    for (index, f) in fragments.into_iter().enumerate() {
        let same = f.lang.as_deref().is_some_and(|l| l.split('-').next() == target.split('-').next());
        if same && skip_target {
            let _ = channel.send(UltraEvent::Unchanged { index });
            continue;
        }
        let (app, channel, limit, target, second) = (app.clone(), channel.clone(), limit.clone(), target.clone(), second.clone());
        tasks.push(tokio::spawn(async move {
            let _permit = limit.acquire().await;
            let req = Request {
                text: f.text,
                source: f.lang.unwrap_or_else(|| AUTO.to_owned()),
                target,
                second_lang: second,
                engine: String::new(),
                programmer_mode: false,
                context: Vec::new(),
            };
            let router = app.router(None);
            match app.pipeline.run(RequestId(index as u64), req, router, app.online(), |_| {}).await {
                Ok(d) if d.text.trim() == d.source_text.trim() => {
                    let _ = channel.send(UltraEvent::Unchanged { index });
                }
                Ok(d) => {
                    let _ = channel.send(UltraEvent::Translated { index, text: d.text, engine: d.engine });
                }
                Err(error) => {
                    let _ = channel.send(UltraEvent::Failed { index, error });
                }
            }
        }));
    }
    for t in tasks {
        let _ = t.await;
    }
    let _ = channel.send(UltraEvent::Done);
}
