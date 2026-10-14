//! Translates from the command line through the real pipeline, streaming to
//! stdout, with stage timings on stderr:
//!
//! ```text
//! cargo run -p hl-pipeline --example translate --release -- "Where is the nearest pharmacy?" ru
//! ```
//!
//! The second run of the same text is a cache hit (a temp-dir cache file).

use std::io::Write;
use std::sync::Arc;
use std::time::Instant;

use hl_cache::{Cache, Store};
use hl_core::{Engine, HedgePolicy, RequestId, Router, TranslateEvent};
use hl_pipeline::{Pipeline, Request};
use hl_providers::Google;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_env("HELILINGO_LOG").unwrap_or_else(|_| "hl_pipeline=debug".into()))
        .with_writer(std::io::stderr)
        .init();
    let mut args = std::env::args().skip(1);
    let text = args.next().unwrap_or_else(|| "Where is the nearest pharmacy?".into());
    let target = args.next().unwrap_or_else(|| "ru".into());

    let t0 = Instant::now();
    hl_detect::warm_up();
    let store = Store::open(&std::env::temp_dir().join("hl-cli-cache.redb")).expect("cache file");
    let pipeline = Pipeline { cache: Some(Cache::new(store)) };
    let engines: Vec<Arc<dyn Engine>> = vec![Arc::new(Google::new(hl_providers::client(), None))];
    let router = Router { engines, policy: HedgePolicy::default(), bench: Arc::default() };
    let req = Request {
        text,
        source: "auto".into(),
        target,
        second_lang: "en".into(),
        engine: String::new(),
        programmer_mode: true,
        context: Vec::new(),
    };

    let mut first: Option<f64> = None;
    let result = pipeline
        .run(RequestId(1), req, router, true, |e| match e {
            TranslateEvent::Planned { source_lang, target, .. } => {
                eprintln!("[{:>7.1} ms] plan: {} → {target}", ms(t0), source_lang.as_deref().unwrap_or("?"));
            }
            TranslateEvent::Started { engine, .. } => eprintln!("[{:>7.1} ms] engine: {engine}", ms(t0)),
            TranslateEvent::Delta { text, .. } => {
                first.get_or_insert(ms(t0));
                print!("{text}");
                let _ = std::io::stdout().flush();
            }
            TranslateEvent::Replace { text, .. } => print!("\n↺ {text}"),
            _ => {}
        })
        .await;
    println!();
    match result {
        Ok(d) => eprintln!(
            "[{:>7.1} ms] done ({}), first text at {:.1} ms",
            ms(t0),
            if d.cached { "cache" } else { d.engine.as_str() },
            first.unwrap_or_default()
        ),
        Err(e) => eprintln!("[{:>7.1} ms] failed: {e}", ms(t0)),
    }
}

fn ms(t0: Instant) -> f64 {
    t0.elapsed().as_secs_f64() * 1000.0
}
