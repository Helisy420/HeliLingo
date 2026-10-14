//! Commands the windows call (typed wrappers live in `ui/src/lib/ipc.ts`).

use std::sync::Arc;

use hl_cache::history::Item;
use hl_cache::stats::Summary;
use hl_core::{RequestId, Surface, TranslateEvent};
use hl_pipeline::{Done, Request};
use serde::Deserialize;
use tauri::{AppHandle, Emitter, State, WebviewWindow};
use tauri::ipc::Channel;

use crate::actions::Action;
use crate::settings::Settings;
use crate::state::{AppState, EngineInfo};
use crate::stream;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslateArgs {
    pub text: String,
    pub source: String,
    pub target: String,
    pub surface: Surface,
    /// Engine picked in this window; it goes first in the chain.
    #[serde(default)]
    pub engine: Option<String>,
}

/// Starts a translation for `surface` (cancelling the one running there) and
/// streams its events to `on_event`. Returns at once with the request id.
/// Async so it runs on the Tokio runtime, where the request task is spawned.
#[tauri::command]
pub async fn translate(
    state: State<'_, Arc<AppState>>,
    args: TranslateArgs,
    on_event: Channel<TranslateEvent>,
) -> Result<RequestId, ()> {
    let app = state.inner().clone();
    let surface = args.surface;
    Ok(state.requests.begin(surface, move |id| async move {
        let emit = stream::to_window(on_event);
        let router = app.router(args.engine.as_deref());
        let (second_lang, programmer_mode) = {
            let s = app.settings.read().unwrap();
            (s.second_lang.clone(), s.programmer_mode)
        };
        let req = Request {
            text: args.text,
            source: args.source,
            target: args.target,
            second_lang,
            engine: args.engine.unwrap_or_default(),
            programmer_mode,
            context: app.context_for(surface),
        };
        if let Ok(done) = app.pipeline.run(id, req, router, app.online(), emit).await {
            app.remember(surface, &done.source_text, &done.text);
            let app = app.clone();
            tauri::async_runtime::spawn_blocking(move || record(&app, surface, &done));
        }
    }))
}

/// History and statistics for a finished translation (blocking: disk).
fn record(app: &AppState, surface: Surface, done: &Done) {
    let (save_history, keep_stats) = {
        let s = app.settings.read().unwrap();
        (s.save_history, s.keep_stats)
    };
    let surface = surface_name(surface);
    if save_history && let Some(h) = &app.history {
        let item = Item {
            id: 0,
            source_text: done.source_text.clone(),
            text: done.text.clone(),
            source_lang: done.source_lang.clone(),
            target_lang: done.target.clone(),
            engine: done.engine.clone(),
            surface: surface.to_owned(),
            starred: false,
            created: 0,
        };
        if let Err(e) = h.add(item) {
            tracing::warn!("history: {e}");
        }
    }
    if keep_stats && let Some(st) = &app.stats {
        let event = hl_cache::stats::Event {
            day: hl_win::local_day(),
            feature: surface,
            source_text: &done.source_text,
            text: &done.text,
            source_lang: done.source_lang.as_deref().unwrap_or("auto"),
            target_lang: &done.target,
        };
        if let Err(e) = st.record(&event) {
            tracing::warn!("stats: {e}");
        }
    }
}

fn surface_name(s: Surface) -> &'static str {
    match s {
        Surface::Popup => "popup",
        Surface::Quick => "quick",
        Surface::Translator => "translator",
        Surface::Ultra => "ultra",
        Surface::Paste => "paste",
    }
}

/// Stops what runs on `surface` (the window closed or the text was cleared).
#[tauri::command]
pub async fn cancel(state: State<'_, Arc<AppState>>, surface: Surface) -> Result<(), ()> {
    state.requests.cancel(surface);
    Ok(())
}

/// Engines for the pickers.
#[tauri::command]
pub async fn engines(state: State<'_, Arc<AppState>>) -> Result<Vec<EngineInfo>, ()> {
    Ok(state.engine_list())
}

/// Replaces the selection in the app that was in front before this window
/// opened: hides the window, pastes `text` there, restores the clipboard.
#[tauri::command]
pub async fn replace_selection(state: State<'_, Arc<AppState>>, window: WebviewWindow, text: String) -> Result<(), ()> {
    let target = *state.target.lock().unwrap();
    let _ = window.hide();
    tauri::async_runtime::spawn_blocking(move || hl_win::paste_into(target, &text));
    Ok(())
}

/// All settings, for a window that just opened.
#[tauri::command]
pub async fn get_settings(state: State<'_, Arc<AppState>>) -> Result<Settings, ()> {
    Ok(state.settings.read().unwrap().clone())
}

/// Applies the changed fields (`patch` is a partial settings object), saves
/// them, and tells every window. Returns the full new settings.
#[tauri::command]
pub async fn update_settings(app: AppHandle, patch: serde_json::Value) -> Result<Settings, String> {
    crate::apply::update(&app, patch)
}

/// Stores (or, with an empty string, removes) an engine's API key.
#[tauri::command]
pub async fn set_key(app: AppHandle, state: State<'_, Arc<AppState>>, engine: String, key: String) -> Result<(), String> {
    if !crate::settings::CLOUD_ENGINES.contains(&engine.as_str()) {
        return Err("unknown engine".into());
    }
    if !hl_win::secret::set(&engine, &key) {
        return Err("Windows Credential Manager refused the key".into());
    }
    state.rebuild_engines();
    let _ = app.emit("engines", ());
    Ok(())
}

/// A page finished its first render: show its window (new windows start
/// hidden so they never flash empty).
#[tauri::command]
pub async fn ready(window: WebviewWindow) -> Result<(), ()> {
    if window.label() != crate::windows::QUICK
        && window.label() != crate::windows::POPUP
        && window.label() != crate::windows::TRAY
    {
        let _ = window.show();
        crate::windows::focus(&window);
        if let Some(p) = crate::windows::take_pending(window.label()) {
            let _ = window.emit_to(window.label(), "window:shown", Some(p));
        }
    }
    Ok(())
}

/// Runs an action from a window (tray menu items, "open settings", …).
#[tauri::command]
pub async fn run_action(app: AppHandle, action: Action) -> Result<(), ()> {
    crate::actions::run(&app, action);
    Ok(())
}

/// Opens Settings on a page ("bubbles", "offline", …).
#[tauri::command]
pub async fn open_settings(app: AppHandle, page: String) -> Result<(), ()> {
    crate::windows::show_with(&app, crate::windows::SETTINGS, Some(page));
    Ok(())
}

/// Leaves the app (tray menu «Выход»).
#[tauri::command]
pub async fn quit(app: AppHandle) -> Result<(), ()> {
    app.exit(0);
    Ok(())
}

fn data<T>(x: &Option<T>) -> Result<&T, String> {
    x.as_ref().ok_or_else(|| "data file unavailable".to_owned())
}

/// History, newest first (`query` filters, `starred` keeps starred only).
#[tauri::command]
pub async fn history_list(state: State<'_, Arc<AppState>>, query: String, starred: bool) -> Result<Vec<Item>, String> {
    data(&state.history)?.list(&query, starred, 500)
}

#[tauri::command]
pub async fn history_star(state: State<'_, Arc<AppState>>, id: u64, starred: bool) -> Result<(), String> {
    data(&state.history)?.set_star(id, starred)
}

#[tauri::command]
pub async fn history_delete(state: State<'_, Arc<AppState>>, id: u64) -> Result<(), String> {
    data(&state.history)?.delete(id)
}

/// Clears history (starred entries stay).
#[tauri::command]
pub async fn history_clear(state: State<'_, Arc<AppState>>) -> Result<(), String> {
    data(&state.history)?.clear()
}

/// Statistics for "week", "month" or "all".
#[tauri::command]
pub async fn stats_summary(state: State<'_, Arc<AppState>>, period: String) -> Result<Summary, String> {
    let days = match period.as_str() {
        "week" => 7,
        "month" => 30,
        _ => 0,
    };
    data(&state.stats)?.summary(hl_win::local_day(), days)
}

#[tauri::command]
pub async fn stats_reset(state: State<'_, Arc<AppState>>) -> Result<(), String> {
    data(&state.stats)?.reset()
}

/// Empties the translation cache («Очистить кэш»).
#[tauri::command]
pub async fn cache_clear(state: State<'_, Arc<AppState>>) -> Result<(), String> {
    match &state.pipeline.cache {
        Some(c) => c.clear().await,
        None => Ok(()),
    }
}

/// Tray switch state.
#[tauri::command]
pub async fn power(state: State<'_, Arc<AppState>>) -> Result<crate::state::PowerInfo, ()> {
    Ok(state.power_info())
}

/// Tray «Включён» switch: off = shortcuts do nothing.
#[tauri::command]
pub async fn set_enabled(app: AppHandle, state: State<'_, Arc<AppState>>, enabled: bool) -> Result<crate::state::PowerInfo, ()> {
    {
        let mut p = state.power.lock().unwrap();
        p.off = !enabled;
        p.paused_until = None;
    }
    let info = state.power_info();
    let _ = app.emit("power", &info);
    Ok(info)
}

/// «Пауза на N минут» (0 = resume now).
#[tauri::command]
pub async fn pause(app: AppHandle, state: State<'_, Arc<AppState>>, minutes: u64) -> Result<crate::state::PowerInfo, ()> {
    state.power.lock().unwrap().paused_until =
        (minutes > 0).then(|| std::time::Instant::now() + std::time::Duration::from_secs(minutes * 60));
    let info = state.power_info();
    let _ = app.emit("power", &info);
    Ok(info)
}

/// One engine's answer to «Проверить подключение».
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Check {
    pub id: String,
    pub ok: bool,
    pub error: Option<hl_core::EngineError>,
    pub ms: u64,
}

/// Sends "Hello" to every enabled engine, each on its own (no fallback).
#[tauri::command]
pub async fn check_engines(state: State<'_, Arc<AppState>>) -> Result<Vec<Check>, ()> {
    let engines = state.router(None).engines;
    let mut tasks = Vec::new();
    for engine in engines {
        tasks.push(tauri::async_runtime::spawn(async move {
            let id = engine.id().to_string();
            let job = Arc::new(hl_core::Job { text: "Hello, world".into(), source: "en".into(), target: "ru".into(), ..Default::default() });
            let t0 = std::time::Instant::now();
            let r = tokio::time::timeout(std::time::Duration::from_secs(10), engine.translate(job, hl_core::Sink::discard())).await;
            let ms = t0.elapsed().as_millis() as u64;
            match r {
                Ok(Ok(_)) => Check { id, ok: true, error: None, ms },
                Ok(Err(e)) => Check { id, ok: false, error: Some(e), ms },
                Err(_) => Check { id, ok: false, error: Some(hl_core::EngineError::Timeout), ms },
            }
        }));
    }
    let mut out = Vec::new();
    for t in tasks {
        if let Ok(c) = t.await {
            out.push(c);
        }
    }
    Ok(out)
}

fn glossary_path() -> std::path::PathBuf {
    crate::settings::data_dir().join("glossary.tsv")
}

/// Number of terms in the glossary file.
#[tauri::command]
pub async fn glossary_count() -> Result<usize, ()> {
    let text = std::fs::read_to_string(glossary_path()).unwrap_or_default();
    Ok(hl_core::prompt::parse_glossary(&text).len())
}

/// Opens glossary.tsv in the default editor (created with an example first).
#[tauri::command]
pub async fn open_glossary() -> Result<(), String> {
    let path = glossary_path();
    if !path.exists() {
        std::fs::create_dir_all(path.parent().expect("has parent")).map_err(|e| e.to_string())?;
        std::fs::write(&path, "# term<TAB>translation, one per line\n# build\tсборка\n").map_err(|e| e.to_string())?;
    }
    hl_win::open(&path.to_string_lossy());
    Ok(())
}

/// OpenRouter's model list for the picker: kept in a file for a day
/// (`refresh` fetches it again now).
#[tauri::command]
pub async fn openrouter_models(refresh: bool) -> Result<Vec<hl_providers::openrouter::ModelInfo>, hl_core::EngineError> {
    let path = crate::settings::local_dir().join("openrouter-models.json");
    let read = || std::fs::read(&path).ok().and_then(|b| serde_json::from_slice::<Vec<_>>(&b).ok());
    let fresh = std::fs::metadata(&path)
        .and_then(|m| m.modified())
        .is_ok_and(|t| t.elapsed().is_ok_and(|age| age < std::time::Duration::from_secs(24 * 3600)));
    if fresh && !refresh && let Some(list) = read() {
        return Ok(list);
    }
    match hl_providers::openrouter::models(&hl_providers::client()).await {
        Ok(list) => {
            let _ = std::fs::create_dir_all(crate::settings::local_dir());
            let _ = std::fs::write(&path, serde_json::to_vec(&list).unwrap_or_default());
            Ok(list)
        }
        // Offline: an old list beats none.
        Err(e) => read().ok_or(e),
    }
}

/// The popup's «W» card: a Wikipedia summary for the translation (in the
/// target language) or the original.
#[tauri::command]
pub async fn wiki_lookup(
    state: State<'_, Arc<AppState>>,
    source: String,
    source_lang: Option<String>,
    translation: Option<String>,
    target: String,
) -> Result<hl_providers::wiki::Outcome, ()> {
    use crate::settings::{UiLanguage, WikiLang, WikiSize};
    use hl_providers::wiki::Prefer;
    let (lang, size, ui) = {
        let s = state.settings.read().unwrap();
        (s.bubble.wiki_lang, s.bubble.wiki_size, s.ui_language)
    };
    let prefer = match lang {
        WikiLang::Word => Prefer::Source,
        WikiLang::Translation => Prefer::Target,
        // The interface language: whichever side is written in it.
        WikiLang::Interface => {
            let ui = if ui == UiLanguage::En { "en" } else { "ru" };
            if target.starts_with(ui) { Prefer::Target } else { Prefer::Source }
        }
    };
    let c = hl_providers::wiki::candidates_by(prefer, &source, source_lang.as_deref(), translation.as_deref(), &target);
    Ok(hl_providers::wiki::lookup(c, size == WikiSize::Long).await)
}

/// The translator's status line: online, and offline models ready.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NetStatus {
    pub online: bool,
    /// Offline is on and at least one model that's switched on is installed.
    pub offline_ready: bool,
}

#[tauri::command]
pub async fn net_status(state: State<'_, Arc<AppState>>) -> Result<NetStatus, ()> {
    let online = tauri::async_runtime::spawn_blocking(hl_win::net::online).await.unwrap_or(true);
    let s = state.settings.read().unwrap().offline.clone();
    let cfg = s.config();
    let offline_ready = s.enabled
        && s.tiers.iter().filter_map(|t| hl_offline::Model::from_id(t)).any(|m| m.translates() && cfg.installed(m));
    Ok(NetStatus { online, offline_ready })
}

/// Opens an http(s) link in the browser.
#[tauri::command]
pub async fn open_url(url: String) -> Result<(), String> {
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err("not a web link".into());
    }
    hl_win::open(&url);
    Ok(())
}

/// Recognizes text in an image. The body is the encoded image (raw bytes);
/// the `lang` header is the source language ("auto" = the user's languages).
#[tauri::command]
pub async fn ocr_image(
    state: State<'_, Arc<AppState>>,
    request: tauri::ipc::Request<'_>,
) -> Result<hl_win::ocr::Recognized, hl_win::ocr::OcrError> {
    let tauri::ipc::InvokeBody::Raw(bytes) = request.body() else {
        return Err(hl_win::ocr::OcrError::BadImage);
    };
    let bytes = bytes.clone();
    let lang = request.headers().get("lang").and_then(|v| v.to_str().ok()).unwrap_or("auto").to_owned();
    recognize(&state, bytes, &lang).await
}

/// Reads the text in an image: Granite Vision when it's chosen for the
/// screen and installed, else Windows OCR in `lang` ("auto" = the user's
/// languages).
pub async fn recognize(
    state: &AppState,
    bytes: Vec<u8>,
    lang: &str,
) -> Result<hl_win::ocr::Recognized, hl_win::ocr::OcrError> {
    let granite = {
        let s = state.settings.read().unwrap();
        let cfg = s.offline.config();
        (s.offline.enabled && s.offline.screen_ocr == crate::settings::ScreenOcr::Granite && cfg.installed(hl_offline::Model::Granite))
            .then_some(cfg)
    };
    if let Some(cfg) = granite {
        return granite_ocr(&cfg, &bytes).await;
    }
    let lang = (lang != "auto").then(|| lang.to_owned());
    tauri::async_runtime::spawn_blocking(move || hl_win::ocr::recognize(&bytes, lang.as_deref()))
        .await
        .map_err(|e| hl_win::ocr::OcrError::Failed(e.to_string()))?
}

/// Granite Vision reads the image: one block per paragraph, stacked over
/// the image (it gives no positions).
async fn granite_ocr(cfg: &hl_offline::Config, bytes: &[u8]) -> Result<hl_win::ocr::Recognized, hl_win::ocr::OcrError> {
    use hl_win::ocr::{Block, Bounds, OcrError, Recognized};
    let (width, height) = hl_offline::vision::image_size(bytes).unwrap_or((1000, 1000));
    let text = hl_offline::vision::read_text(cfg, bytes).await.map_err(|e| OcrError::Failed(e.to_string()))?;
    let paragraphs: Vec<&str> = text.split("\n\n").map(str::trim).filter(|p| !p.is_empty()).collect();
    let n = paragraphs.len().max(1) as f32;
    let blocks = paragraphs
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let h = height as f32 / n;
            let lines = p.lines().count().max(1);
            Block {
                text: p.to_string(),
                bounds: Bounds { x: 0.0, y: h * i as f32, w: width as f32, h },
                lines,
                line_height: (h / lines as f32).min(24.0),
            }
        })
        .collect();
    Ok(Recognized { blocks, language: "granite".into(), width, height })
}

/// What Settings → Офлайн shows: the graphics card, the llama build, and
/// every model's state.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OfflineStatus {
    pub gpu: Option<hl_offline::sys::Gpu>,
    pub backend: &'static str,
    pub cores: usize,
    pub models: Vec<OfflineModel>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OfflineModel {
    pub id: &'static str,
    pub name: &'static str,
    pub status: hl_offline::manager::Status,
    /// Switched on (used for translation / screen reading).
    pub enabled: bool,
    /// In memory right now.
    pub loaded: bool,
}

#[tauri::command]
pub async fn offline_status(state: State<'_, Arc<AppState>>) -> Result<OfflineStatus, ()> {
    let s = state.settings.read().unwrap().offline.clone();
    let cfg = s.config();
    let backend = hl_offline::catalog::runtime_info(&cfg.model_dir)
        .map(|i| i.backend)
        .unwrap_or_else(hl_offline::catalog::LlamaBackend::detect)
        .label();
    let models = hl_offline::Model::ALL
        .into_iter()
        .map(|m| OfflineModel {
            id: m.id(),
            name: m.name(),
            status: hl_offline::manager::status(&cfg.model_dir, m),
            enabled: if m.translates() {
                s.tiers.iter().any(|t| t == m.id())
            } else {
                s.screen_ocr == crate::settings::ScreenOcr::Granite
            },
            loaded: hl_offline::loaded(m),
        })
        .collect();
    Ok(OfflineStatus { gpu: hl_offline::sys::gpu(), backend, cores: hl_offline::sys::physical_cores(), models })
}

/// The model folder: where it is now and whether that's the default.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelDir {
    pub path: String,
    pub is_default: bool,
    /// Free space on its drive, bytes.
    pub free: Option<u64>,
}

#[tauri::command]
pub async fn offline_dir(state: State<'_, Arc<AppState>>) -> Result<ModelDir, ()> {
    let setting = state.settings.read().unwrap().offline.model_dir.clone();
    let path = hl_offline::model_dir(&setting);
    Ok(ModelDir {
        free: hl_offline::sys::free_disk(&path),
        path: path.to_string_lossy().into_owned(),
        is_default: setting.trim().is_empty(),
    })
}

/// Asks for a new model folder (Windows' folder picker). `None` = cancelled.
/// Models already downloaded stay where they are; the page says so.
#[tauri::command]
pub async fn offline_pick_dir(state: State<'_, Arc<AppState>>) -> Result<Option<String>, ()> {
    let current = hl_offline::model_dir(&state.settings.read().unwrap().offline.model_dir);
    let title = if state.settings.read().unwrap().ui_language == crate::settings::UiLanguage::En {
        "Folder for offline models"
    } else {
        "Папка для офлайн-моделей"
    };
    let picked = tauri::async_runtime::spawn_blocking(move || hl_offline::sys::pick_folder(title, &current))
        .await
        .ok()
        .flatten();
    Ok(picked.map(|p| p.to_string_lossy().into_owned()))
}

/// «Папка для результатов» as shown: the path, and whether it's the default.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FilesDir {
    pub path: String,
    pub is_default: bool,
}

#[tauri::command]
pub async fn files_dir(state: State<'_, Arc<AppState>>) -> Result<FilesDir, ()> {
    let setting = state.settings.read().unwrap().files.out_dir.clone();
    Ok(FilesDir {
        path: crate::settings::files_dir(&setting).to_string_lossy().into_owned(),
        is_default: setting.trim().is_empty(),
    })
}

/// Asks for the folder translated documents go to. `None` = cancelled.
#[tauri::command]
pub async fn files_pick_dir(state: State<'_, Arc<AppState>>) -> Result<Option<String>, ()> {
    let (current, en) = {
        let s = state.settings.read().unwrap();
        (crate::settings::files_dir(&s.files.out_dir), s.ui_language == crate::settings::UiLanguage::En)
    };
    let title = if en { "Folder for translated documents" } else { "Папка для переведённых документов" };
    let picked = tauri::async_runtime::spawn_blocking(move || hl_offline::sys::pick_folder(title, &current))
        .await
        .ok()
        .flatten();
    Ok(picked.map(|p| p.to_string_lossy().into_owned()))
}

/// Opens the model folder in Explorer.
#[tauri::command]
pub async fn offline_open_dir(state: State<'_, Arc<AppState>>) -> Result<(), ()> {
    let path = hl_offline::model_dir(&state.settings.read().unwrap().offline.model_dir);
    let _ = std::fs::create_dir_all(&path);
    hl_win::open(&path.to_string_lossy());
    Ok(())
}

fn offline_model(id: &str) -> Result<hl_offline::Model, String> {
    hl_offline::Model::from_id(id).ok_or_else(|| "unknown model".to_owned())
}

/// Starts downloading a model (only while offline is switched on).
#[tauri::command]
pub async fn offline_install(state: State<'_, Arc<AppState>>, id: String) -> Result<(), String> {
    let m = offline_model(&id)?;
    let s = state.settings.read().unwrap().offline.clone();
    if !s.enabled {
        return Err("offline is off".into());
    }
    hl_offline::manager::dismiss(m);
    hl_offline::manager::install(s.config().model_dir, m);
    Ok(())
}

#[tauri::command]
pub async fn offline_cancel(id: String) -> Result<(), String> {
    let m = offline_model(&id)?;
    hl_offline::manager::cancel(m);
    hl_offline::manager::dismiss(m);
    Ok(())
}

/// Deletes a model's files and switches it off.
#[tauri::command]
pub async fn offline_delete(app: AppHandle, state: State<'_, Arc<AppState>>, id: String) -> Result<(), String> {
    let m = offline_model(&id)?;
    let dir = state.settings.read().unwrap().offline.config().model_dir;
    hl_offline::manager::delete(&dir, m).await.map_err(|e| e.to_string())?;
    state.rebuild_engines();
    let _ = app.emit("engines", ());
    Ok(())
}

/// Ultra mode on `text`: fragments and their translations stream to
/// `on_event` (cancels the previous Ultra run).
#[tauri::command]
pub async fn ultra(
    state: State<'_, Arc<AppState>>,
    text: String,
    on_event: Channel<crate::ultra::UltraEvent>,
) -> Result<RequestId, ()> {
    let app = state.inner().clone();
    Ok(state.requests.begin(Surface::Ultra, move |_| crate::ultra::run(app, text, on_event)))
}

/// Opens the translator with `text` (History → «Открыть в переводчике»).
#[tauri::command]
pub async fn open_in_translator(app: AppHandle, text: String) -> Result<(), ()> {
    crate::windows::show_with(&app, crate::windows::TRANSLATOR, Some(text));
    Ok(())
}

/// The popup page laid itself out: place and show it (see `selection::layout`).
#[tauri::command]
pub async fn popup_layout(app: AppHandle, layout: crate::selection::Layout) -> Result<(), ()> {
    crate::selection::layout(&app, layout);
    Ok(())
}

/// The bubble's pin button.
#[tauri::command]
pub async fn popup_pin(app: AppHandle, pinned: bool) -> Result<(), ()> {
    crate::selection::pin(&app, pinned);
    Ok(())
}

/// Whether the bubble is pinned (the page asks when it shows).
#[tauri::command]
pub async fn popup_pinned() -> Result<bool, ()> {
    Ok(crate::selection::PINNED.load(std::sync::atomic::Ordering::Relaxed))
}

/// Drags the bubble by its grip (it never takes focus, so the page can't
/// use the title-bar drag region).
#[tauri::command]
pub async fn popup_drag(window: WebviewWindow) -> Result<(), ()> {
    let _ = window.start_dragging();
    Ok(())
}

/// Translates without a window and without history (the dictionary
/// bubble's example sentences). Not cancelled by other requests.
#[tauri::command]
pub async fn translate_quiet(
    state: State<'_, Arc<AppState>>,
    text: String,
    source: String,
    target: String,
) -> Result<String, hl_core::EngineError> {
    let req = Request {
        text,
        source,
        target: target.clone(),
        second_lang: target,
        engine: String::new(),
        programmer_mode: false,
        context: Vec::new(),
    };
    let router = state.router(None);
    state.pipeline.run(RequestId(0), req, router, state.online(), |_| {}).await.map(|d| d.text)
}

/// The popup's own close button.
#[tauri::command]
pub async fn popup_hide(app: AppHandle) -> Result<(), ()> {
    crate::selection::hide_popup(&app);
    Ok(())
}

/// Puts text on the clipboard (works from windows without focus, unlike
/// the browser clipboard API).
#[tauri::command]
pub async fn copy_text(text: String) -> Result<(), ()> {
    tauri::async_runtime::spawn_blocking(move || hl_win::clipboard::set_text(&text, false)).await.map_err(|_| ())?;
    Ok(())
}

/// An image is on the clipboard (the Images tab's banner).
#[tauri::command]
pub async fn clipboard_has_image() -> Result<bool, ()> {
    Ok(hl_win::clipboard::has_image())
}

/// The clipboard's image as PNG.
#[tauri::command]
pub async fn clipboard_image() -> Result<tauri::ipc::Response, ()> {
    let png = tauri::async_runtime::spawn_blocking(hl_win::clipboard::image_png).await.ok().flatten().ok_or(())?;
    Ok(tauri::ipc::Response::new(png))
}

/// A dropped picture's bytes (the Images tab; up to 10 MB, images only).
#[tauri::command]
pub async fn image_file(path: String) -> Result<tauri::ipc::Response, ()> {
    let p = std::path::Path::new(&path);
    let ext = p.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).unwrap_or_default();
    if !matches!(ext.as_str(), "png" | "jpg" | "jpeg" | "webp" | "bmp" | "gif") {
        return Err(());
    }
    if std::fs::metadata(p).map_err(|_| ())?.len() > 10 * 1024 * 1024 {
        return Err(());
    }
    Ok(tauri::ipc::Response::new(tokio::fs::read(p).await.map_err(|_| ())?))
}
