//! Logs to `%LOCALAPPDATA%\HeliLingo\logs\helilingo.log` (and the console in
//! debug builds). Level from `HELILINGO_LOG` (default `info`). No translated
//! text and no keys are ever logged.

use std::fs::{File, OpenOptions};
use std::sync::Mutex;

use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt::writer::MakeWriterExt;

/// Start a fresh log once the old one passes this size.
const MAX_BYTES: u64 = 5 << 20;

pub fn init() {
    let filter = EnvFilter::try_from_env("HELILINGO_LOG").unwrap_or_else(|_| "info".into());
    let builder = tracing_subscriber::fmt().with_env_filter(filter).with_ansi(false);
    match open() {
        Some(file) if cfg!(debug_assertions) => builder.with_writer(Mutex::new(file).and(std::io::stderr)).init(),
        Some(file) => builder.with_writer(Mutex::new(file)).init(),
        None => builder.with_writer(std::io::stderr).init(),
    }
}

fn open() -> Option<File> {
    let dir = crate::settings::local_dir().join("logs");
    std::fs::create_dir_all(&dir).ok()?;
    let path = dir.join("helilingo.log");
    let big = std::fs::metadata(&path).is_ok_and(|m| m.len() > MAX_BYTES);
    OpenOptions::new().create(true).append(!big).write(true).truncate(big).open(path).ok()
}
