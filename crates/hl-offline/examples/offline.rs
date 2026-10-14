//! Offline models from a console, without the app:
//!
//! ```text
//! cargo run -p hl-offline --example offline -- install opus [models-dir]
//! cargo run -p hl-offline --example offline -- translate opus "Hello, world" ru [models-dir]
//! ```
//!
//! The example is also its own model worker (like the app's exe).

use std::sync::Arc;
use std::time::Instant;

use hl_core::{Engine, Job, Sink};
use hl_offline::{Config, Model, manager};

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    if let Some(i) = args.iter().position(|a| a == hl_offline::worker::FLAG) {
        hl_offline::worker::serve(args.get(i + 1).map_or("", String::as_str));
        return;
    }
    let usage = "usage: offline install <model> [dir] | translate <model> <text> <target> [dir]";
    let (Some(cmd), Some(model)) = (args.get(1), args.get(2).and_then(|m| Model::from_id(m))) else {
        eprintln!("{usage}");
        return;
    };
    let dir_arg = |i: usize| args.get(i).map(std::path::PathBuf::from).unwrap_or_else(hl_offline::default_model_dir);
    match cmd.as_str() {
        "install" => {
            let dir = dir_arg(3);
            let t0 = Instant::now();
            manager::install(dir.clone(), model);
            loop {
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                match manager::status(&dir, model) {
                    manager::Status::Downloading { done, total } => {
                        eprint!("\r{:>5.1}% of {} MB   ", done as f64 * 100.0 / total.max(1) as f64, total >> 20)
                    }
                    s => {
                        eprintln!("\n{s:?} in {:.1} s", t0.elapsed().as_secs_f64());
                        break;
                    }
                }
            }
        }
        "translate" => {
            let (Some(text), Some(target)) = (args.get(3), args.get(4)) else {
                eprintln!("{usage}");
                return;
            };
            let config = Config { model_dir: dir_arg(5), ..Config::default() };
            let engine = Arc::new(hl_offline::engine::OfflineEngine::new(model, config));
            for round in 0..2 {
                let job = Arc::new(Job { text: text.clone(), source: "auto".into(), target: target.clone(), ..Job::default() });
                let t0 = Instant::now();
                let sink = Sink::new(|t| eprint!("{t}"));
                match engine.clone().translate(job, sink).await {
                    Ok(o) => println!("\n[{round}] {} ({:.0} ms)", o.text, t0.elapsed().as_secs_f64() * 1000.0),
                    Err(e) => println!("\n[{round}] error: {e}"),
                }
            }
            hl_offline::shutdown();
        }
        _ => eprintln!("{usage}"),
    }
}
