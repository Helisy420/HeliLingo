//! «Перевод файлов» (Figma 46:178 / 46:252 / 46:313) and the batch
//! translation the screen-area results use too.
//!
//! A file is read into units (`hl_docs`; a PDF is rendered page by page
//! and recognised with Windows OCR), the units are translated in batches by
//! the user's engines — the same hedged chain as everywhere, offline models
//! included — and the file is rebuilt into a work folder. «Скачать» saves a
//! copy where the user says; «Скачать всё» copies every finished one into
//! «Папка для результатов».

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use futures_util::StreamExt;
use hl_core::{EngineError, RequestId};
use hl_docs::{Doc, Kind, Output, Unit, WriteOptions};
use hl_pipeline::Request;
use serde::Serialize;
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, State};

use crate::settings::FileFormat;
use crate::state::AppState;

/// Figma: «до 20 МБ».
pub const MAX_BYTES: u64 = 20 * 1024 * 1024;
/// A batch: this many characters or units, whichever comes first.
const BATCH_CHARS: usize = 1800;
const BATCH_UNITS: usize = 30;
/// Batches in flight at once (keyless endpoints dislike more).
const PARALLEL: usize = 2;

// ── Batch translation ─────────────────────────────────────────────────

/// Translations of `texts` (one line each), in order, and who did them.
pub struct Batch {
    pub texts: Vec<String>,
    pub engine: String,
    pub source_lang: Option<String>,
}

async fn translate_one(
    app: &AppState,
    text: String,
    source: &str,
    target: &str,
    engine: Option<&str>,
) -> Result<hl_pipeline::Done, EngineError> {
    let req = Request {
        text,
        source: source.to_owned(),
        target: target.to_owned(),
        // Already in the target language: leave it, don't flip to the second one.
        second_lang: target.to_owned(),
        engine: String::new(),
        programmer_mode: false,
        context: Vec::new(),
    };
    app.pipeline.run(RequestId(0), req, app.router(engine), app.online(), |_| {}).await
}

/// Translates `texts` in batches joined by line breaks (engines keep
/// lines). A batch that comes back with a different number of lines is
/// redone line by line. `prefer`: an engine to try first. `progress(done)`
/// after each batch.
pub async fn translate_texts(
    app: &AppState,
    texts: &[String],
    source: &str,
    target: &str,
    prefer: Option<&str>,
    progress: impl Fn(usize) + Send + Sync,
) -> Result<Batch, EngineError> {
    let mut batches: Vec<std::ops::Range<usize>> = Vec::new();
    let (mut start, mut chars) = (0, 0);
    for (i, t) in texts.iter().enumerate() {
        if i > start && (chars + t.len() > BATCH_CHARS || i - start >= BATCH_UNITS) {
            batches.push(start..i);
            start = i;
            chars = 0;
        }
        chars += t.len() + 1;
    }
    if start < texts.len() {
        batches.push(start..texts.len());
    }

    let mut out = vec![String::new(); texts.len()];
    let mut engine = String::new();
    let mut source_lang = None;
    let mut done = 0;
    let mut results = futures_util::stream::iter(batches.into_iter().map(|range| async move {
        let lines: Vec<&str> = texts[range.clone()].iter().map(String::as_str).collect();
        let joined = lines.join("\n");
        let first = translate_one(app, joined, source, target, prefer).await?;
        let parts: Vec<String> = first.text.split('\n').map(|s| s.trim().to_owned()).collect();
        if parts.len() == lines.len() {
            return Ok::<_, EngineError>((range, parts, first.engine, first.source_lang));
        }
        // Lines merged or split: one by one.
        let mut parts = Vec::with_capacity(lines.len());
        for l in &lines {
            parts.push(translate_one(app, (*l).to_owned(), source, target, prefer).await?.text.replace('\n', " "));
        }
        Ok((range, parts, first.engine, first.source_lang))
    }))
    .buffered(PARALLEL);
    while let Some(r) = results.next().await {
        let (range, parts, e, lang) = r?;
        done += range.len();
        for (slot, p) in out[range].iter_mut().zip(parts) {
            *slot = p;
        }
        if engine.is_empty() {
            engine = e;
        }
        if source_lang.is_none() {
            source_lang = lang;
        }
        progress(done);
    }
    Ok(Batch { texts: out, engine, source_lang })
}

/// For the screen-area results: the blocks' translations in one go.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchResult {
    pub texts: Vec<String>,
    pub engine: String,
    pub source_lang: Option<String>,
}

#[tauri::command]
pub async fn translate_batch(
    state: State<'_, Arc<AppState>>,
    texts: Vec<String>,
    source: String,
    target: String,
    engine: Option<String>,
) -> Result<BatchResult, EngineError> {
    let texts: Vec<String> = texts.iter().map(|t| t.replace(['\n', '\r'], " ")).collect();
    let b = translate_texts(&state, &texts, &source, &target, engine.as_deref(), |_| {}).await?;
    Ok(BatchResult { texts: b.texts, engine: b.engine, source_lang: b.source_lang })
}

// ── Files ─────────────────────────────────────────────────────────────

/// A file as the queue shows it before translating.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileInfo {
    pub path: String,
    pub name: String,
    pub size: u64,
    /// "DOCX", "PDF"…; `None` = not a format we translate.
    pub kind: Option<&'static str>,
    pub too_big: bool,
}

fn info(path: &Path) -> FileInfo {
    let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    FileInfo {
        path: path.to_string_lossy().into_owned(),
        name: path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
        size,
        kind: Kind::from_path(path).map(Kind::label),
        too_big: size > MAX_BYTES,
    }
}

#[tauri::command]
pub async fn files_info(paths: Vec<String>) -> Result<Vec<FileInfo>, ()> {
    Ok(paths.iter().map(|p| info(Path::new(p))).collect())
}

/// «Выбрать файлы».
#[tauri::command]
pub async fn files_choose(state: State<'_, Arc<AppState>>) -> Result<Vec<FileInfo>, ()> {
    let en = state.settings.read().unwrap().ui_language == crate::settings::UiLanguage::En;
    let (title, filter) = if en { ("Files to translate", "Documents") } else { ("Файлы для перевода", "Документы") };
    let paths = tauri::async_runtime::spawn_blocking(move || {
        hl_win::dialog::open_files(title, filter, "*.docx;*.pdf;*.pptx;*.xlsx;*.txt;*.md;*.srt")
    })
    .await
    .unwrap_or_default();
    Ok(paths.iter().map(|p| info(p)).collect())
}

/// Ctrl+V: files copied in Explorer.
#[tauri::command]
pub async fn files_from_clipboard() -> Result<Vec<FileInfo>, ()> {
    let paths = tauri::async_runtime::spawn_blocking(hl_win::clipboard::files).await.unwrap_or_default();
    Ok(paths.iter().map(|p| info(p)).collect())
}

/// What the queue row hears while a file is translated.
#[derive(Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum FileEvent {
    /// Read: how much there is. `groups` = slides or pages (0 = none).
    Read { units: usize, pages: Option<u32>, groups: u32 },
    /// Recognising a PDF page (before any translating).
    Reading { page: u32, pages: u32 },
    /// Units translated so far; `group` = the slide or page being done.
    Progress { done: usize, total: usize, group: u32 },
    /// The translated file, in the work folder, and the name to save it as.
    Done { out: String, name: String, source_lang: Option<String> },
    /// `kind`: "broken", "empty", "tooBig", "unreadablePdf", "noText", or an
    /// engine error's kind.
    Error { kind: String },
}

/// Running translations by the page's id, to cancel them.
static RUNNING: Mutex<Option<HashMap<u32, tauri::async_runtime::JoinHandle<()>>>> = Mutex::new(None);

fn work_dir() -> PathBuf {
    crate::settings::local_dir().join("files")
}

/// «Договор.docx» → «Договор (RU).docx».
fn out_name(path: &Path, target: &str, ext: &str) -> String {
    let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "document".into());
    format!("{stem} ({}).{ext}", target.split('-').next().unwrap_or(target).to_uppercase())
}

#[tauri::command]
pub async fn file_translate(
    app: AppHandle,
    id: u32,
    path: String,
    source: String,
    target: String,
    on_event: Channel<FileEvent>,
) -> Result<(), ()> {
    let task = tauri::async_runtime::spawn(async move {
        let state = app.state::<Arc<AppState>>().inner().clone();
        let send = |e: FileEvent| {
            let _ = on_event.send(e);
        };
        match run(&state, Path::new(&path), &source, &target, &send).await {
            Ok(ev) => send(ev),
            Err(kind) => send(FileEvent::Error { kind }),
        }
        if let Some(map) = RUNNING.lock().unwrap().as_mut() {
            map.remove(&id);
        }
    });
    RUNNING.lock().unwrap().get_or_insert_with(HashMap::new).insert(id, task);
    Ok(())
}

#[tauri::command]
pub async fn file_cancel(id: u32) -> Result<(), ()> {
    if let Some(task) = RUNNING.lock().unwrap().as_mut().and_then(|m| m.remove(&id)) {
        task.abort();
    }
    Ok(())
}

async fn run(app: &AppState, path: &Path, source: &str, target: &str, send: &(impl Fn(FileEvent) + Sync)) -> Result<FileEvent, String> {
    let kind = Kind::from_path(path).ok_or("broken")?;
    let size = std::fs::metadata(path).map_err(|_| "broken")?.len();
    if size > MAX_BYTES {
        return Err("tooBig".into());
    }
    let bytes = tokio::fs::read(path).await.map_err(|_| "broken")?;
    let doc = if kind == Kind::Pdf {
        read_pdf(bytes, source, send).await?
    } else {
        tauri::async_runtime::spawn_blocking(move || Doc::read(kind, &bytes)).await.map_err(|_| "broken")?.map_err(|_| "broken")?
    };
    if doc.units.is_empty() {
        return Err("noText".into());
    }
    let groups = doc.units.iter().map(|u| u.group).max().unwrap_or(0);
    send(FileEvent::Read { units: doc.units.len(), pages: doc.pages, groups });

    let texts: Vec<String> = doc.units.iter().map(|u| u.text.clone()).collect();
    let total = texts.len();
    let group_of = |done: usize| doc.units.get(done.min(total - 1)).map_or(0, |u: &Unit| u.group);
    let batch = translate_texts(app, &texts, source, target, None, |done| {
        send(FileEvent::Progress { done, total, group: group_of(done) });
    })
    .await
    .map_err(|e| serde_json::to_value(&e).ok().and_then(|v| v["kind"].as_str().map(str::to_owned)).unwrap_or_else(|| "other".into()))?;

    let opts = {
        let s = app.settings.read().unwrap();
        WriteOptions {
            output: match s.files.format {
                FileFormat::Same => Output::Same,
                FileFormat::Docx => Output::Docx,
                FileFormat::Txt => Output::Txt,
            },
            keep_formatting: s.files.keep_formatting,
            bilingual: s.files.bilingual,
        }
    };
    let (out, ext) = tauri::async_runtime::spawn_blocking(move || doc.write(&batch.texts, &opts).map(|r| (r, batch.source_lang)))
        .await
        .map_err(|_| "broken")?
        .map(|((bytes, ext), lang)| ((bytes, lang), ext))
        .map_err(|_| "broken")?;
    let (bytes, source_lang) = out;
    let name = out_name(path, target, ext);
    let dir = work_dir().join(format!("{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos())));
    std::fs::create_dir_all(&dir).map_err(|_| "broken")?;
    let file = dir.join(&name);
    std::fs::write(&file, bytes).map_err(|_| "broken")?;
    Ok(FileEvent::Done { out: file.to_string_lossy().into_owned(), name, source_lang })
}

/// A PDF: every page rendered (≈ 200 dpi for A4) and recognised; each
/// recognised block is a unit of that page.
async fn read_pdf(bytes: Vec<u8>, source: &str, send: &(impl Fn(FileEvent) + Sync)) -> Result<Doc, String> {
    let lang = (source != "auto").then(|| source.to_owned());
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<(u32, u32)>();
    let reading = tauri::async_runtime::spawn_blocking(move || -> Result<Doc, String> {
        let pdf = hl_win::pdf::Pdf::open(&bytes).map_err(|_| "unreadablePdf")?;
        let pages = pdf.pages();
        let mut units = Vec::new();
        for i in 0..pages {
            let _ = tx.send((i + 1, pages));
            let png = pdf.page_png(i, 1700).map_err(|_| "unreadablePdf")?;
            let r = hl_win::ocr::recognize(&png, lang.as_deref()).map_err(|e| match e {
                hl_win::ocr::OcrError::NoLanguage(_) => "noOcrLanguage",
                _ => "unreadablePdf",
            })?;
            for b in r.blocks {
                let text = b.text.split_whitespace().collect::<Vec<_>>().join(" ");
                if text.chars().any(char::is_alphabetic) {
                    units.push(Unit { text, group: i + 1 });
                }
            }
        }
        Ok(Doc::from_units(Kind::Pdf, units, Some(pages)))
    });
    let mut reading = std::pin::pin!(reading);
    loop {
        tokio::select! {
            Some((page, pages)) = rx.recv() => send(FileEvent::Reading { page, pages }),
            r = &mut reading => return r.map_err(|_| "unreadablePdf".to_owned())?,
        }
    }
}

// ── Saving ────────────────────────────────────────────────────────────

fn results_dir(app: &AppState) -> PathBuf {
    crate::settings::files_dir(&app.settings.read().unwrap().files.out_dir)
}

/// A free name in `dir`: «Договор (RU).docx», then «Договор (RU) 2.docx».
fn free_path(dir: &Path, name: &str) -> PathBuf {
    let first = dir.join(name);
    if !first.exists() {
        return first;
    }
    let p = Path::new(name);
    let stem = p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let ext = p.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
    (2..).map(|n| dir.join(format!("{stem} {n}{ext}"))).find(|p| !p.exists()).unwrap_or(first)
}

/// «Скачать»: the save dialog, starting in «Папка для результатов».
/// Returns where it went (`None` = cancelled).
#[tauri::command]
pub async fn file_save(state: State<'_, Arc<AppState>>, out: String, name: String) -> Result<Option<String>, String> {
    let dir = results_dir(&state);
    let en = state.settings.read().unwrap().ui_language == crate::settings::UiLanguage::En;
    let title = if en { "Save the translation" } else { "Сохранить перевод" };
    let _ = std::fs::create_dir_all(&dir);
    let picked = tauri::async_runtime::spawn_blocking(move || hl_win::dialog::save_file(title, &dir, &name))
        .await
        .ok()
        .flatten();
    let Some(dest) = picked else { return Ok(None) };
    std::fs::copy(&out, &dest).map_err(|e| e.to_string())?;
    hl_win::dialog::reveal(&dest);
    Ok(Some(dest.to_string_lossy().into_owned()))
}

/// «Скачать всё»: every finished file into «Папка для результатов», then
/// the folder opens.
#[tauri::command]
pub async fn files_save_all(state: State<'_, Arc<AppState>>, items: Vec<(String, String)>) -> Result<(), String> {
    let dir = results_dir(&state);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let mut last = None;
    for (out, name) in items {
        let dest = free_path(&dir, &name);
        std::fs::copy(&out, &dest).map_err(|e| e.to_string())?;
        last = Some(dest);
    }
    match last {
        Some(p) => hl_win::dialog::reveal(&p),
        None => hl_win::open(&dir.to_string_lossy()),
    }
    Ok(())
}

/// «Открыть папку».
#[tauri::command]
pub async fn files_open_dir(state: State<'_, Arc<AppState>>) -> Result<(), ()> {
    let dir = results_dir(&state);
    let _ = std::fs::create_dir_all(&dir);
    hl_win::open(&dir.to_string_lossy());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        assert_eq!(out_name(Path::new("C:/x/Договор поставки.docx"), "ru", "docx"), "Договор поставки (RU).docx");
        assert_eq!(out_name(Path::new("scan.pdf"), "zh-CN", "docx"), "scan (ZH).docx");
    }

    #[test]
    fn free_names_count_up() {
        let dir = std::env::temp_dir().join(format!("hl-free-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a (RU).docx"), b"x").unwrap();
        assert_eq!(free_path(&dir, "a (RU).docx"), dir.join("a (RU) 2.docx"));
        assert_eq!(free_path(&dir, "b.txt"), dir.join("b.txt"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
