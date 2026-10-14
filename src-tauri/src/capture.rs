//! «Перевод области экрана» (Figma 48:297, 49:192 / 49:244 / 49:300): an
//! overlay over the monitor under the cursor. First the user picks what to
//! read — a dragged area with handles, a window under the pointer, or the
//! whole screen — then the rectangle is copied and read, and the page draws
//! the translation over the real screen («Поверх», «Плашка», «Рядом»,
//! «Субтитры»).
//!
//! The overlay is left out of screen captures (`exclude_from_capture`), so
//! it never reads its own translation and needn't hide for a copy. Pinned
//! («Закрепить»), the area stays live: it's watched a few times a second
//! and read again when its picture changes and settles — a game's dialogue
//! box, subtitles. Then the overlay lets clicks through everywhere except
//! its own controls, and the game keeps focus.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::ipc::Channel;
use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

use crate::state::AppState;
use crate::windows;

pub const CAPTURE: &str = "capture";

/// What the overlay starts with («Весь экран» is picked on the page).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Mode {
    Area,
    Window,
}

/// A rectangle in the overlay's CSS pixels.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Start {
    mode: Mode,
    /// Other apps' windows on this monitor, front to back, for «Окно».
    windows: Vec<Rect>,
}

/// The app that was in front when the overlay opened (a game): it gets
/// focus back when the area is pinned.
static TARGET: Mutex<hl_win::Hwnd> = Mutex::new(hl_win::Hwnd(0));
/// The overlay is left out of captures (else it must hide for a copy).
static EXCLUDED: AtomicBool = AtomicBool::new(false);
/// The pinned area's watcher.
static LIVE: Mutex<Option<tauri::async_runtime::JoinHandle<()>>> = Mutex::new(None);
/// Pinned: where the overlay takes clicks (its controls), in screen pixels.
static HITS: Mutex<Vec<(i32, i32, i32, i32)>> = Mutex::new(Vec::new());
static PINNED: AtomicBool = AtomicBool::new(false);
/// The latest start, for a page that wasn't listening yet (the first
/// start loads the page while this is being sent).
static LAST_START: Mutex<Option<Start>> = Mutex::new(None);

/// Opens the overlay on the monitor under the cursor.
pub fn start(app: &AppHandle, mode: Mode) {
    let Some(monitor) = windows::cursor_monitor(app) else { return };
    stop_live(app);
    let fg = hl_win::window::foreground();
    if hl_win::window::process_id(fg) != std::process::id() {
        *TARGET.lock().unwrap() = fg;
    }
    // Before the overlay shows: it would be the top window itself.
    let (origin, size, scale) = (*monitor.position(), *monitor.size(), monitor.scale_factor());
    let to_css = |l: i32, t: i32, r: i32, b: i32| -> Option<Rect> {
        let (ml, mt) = (origin.x, origin.y);
        let (mr, mb) = (ml + size.width as i32, mt + size.height as i32);
        let (l, t, r, b) = (l.max(ml), t.max(mt), r.min(mr), b.min(mb));
        (r - l > 40 && b - t > 30).then(|| Rect {
            x: (l - ml) as f64 / scale,
            y: (t - mt) as f64 / scale,
            w: (r - l) as f64 / scale,
            h: (b - t) as f64 / scale,
        })
    };
    let list: Vec<Rect> =
        hl_win::window::visible_windows().into_iter().filter_map(|(_, (l, t, r, b))| to_css(l, t, r, b)).collect();

    let (w, created) = match app.get_webview_window(CAPTURE) {
        Some(w) => (w, false),
        None => match WebviewWindowBuilder::new(app, CAPTURE, WebviewUrl::App("capture.html".into()))
            .title("HeliLingo")
            .decorations(false)
            .transparent(true)
            .shadow(false)
            .always_on_top(true)
            .skip_taskbar(true)
            .resizable(false)
            .visible(false)
            .build()
        {
            Ok(w) => {
                if let Ok(h) = w.hwnd() {
                    let ok = hl_win::window::exclude_from_capture(hl_win::Hwnd(h.0 as isize));
                    EXCLUDED.store(ok, Ordering::Relaxed);
                }
                (w, true)
            }
            Err(e) => {
                tracing::error!("capture overlay: {e}");
                return;
            }
        },
    };
    set_interactive(&w, true);
    place(&w, origin, size);
    let start = Start { mode, windows: list };
    *LAST_START.lock().unwrap() = Some(start.clone());
    // A new page asks for it (`capture_state`) once it's listening; an
    // open one hears it now.
    if !created {
        let _ = w.emit_to(CAPTURE, "capture:start", start);
        let _ = w.show();
        windows::focus(&w);
    }
}

/// What the overlay should show now (its page asks on load).
#[tauri::command]
pub async fn capture_state() -> Result<Option<Start>, ()> {
    let last = LAST_START.lock().unwrap().clone();
    tracing::info!(windows = last.as_ref().map(|s| s.windows.len()), "capture page loaded");
    Ok(last)
}

/// Covers the whole monitor, taskbar included. Moving onto a monitor with
/// another scale makes Windows resize the window for the new DPI, so the
/// size is set again once that has happened.
fn place(w: &WebviewWindow, origin: tauri::PhysicalPosition<i32>, size: tauri::PhysicalSize<u32>) {
    let _ = w.set_position(origin);
    let _ = w.set_size(size);
    let w = w.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(80));
        let _ = w.set_position(origin);
        let _ = w.set_size(size);
    });
}

/// Screen pixels of a CSS rectangle of the overlay.
fn to_screen(window: &WebviewWindow, r: &Rect) -> Result<(i32, i32, i32, i32), String> {
    let scale = window.scale_factor().map_err(|e| e.to_string())?;
    let origin = window.outer_position().map_err(|e| e.to_string())?;
    Ok((
        origin.x + (r.x * scale).round() as i32,
        origin.y + (r.y * scale).round() as i32,
        (r.w * scale).round() as i32,
        (r.h * scale).round() as i32,
    ))
}

/// The user picked a rectangle (CSS pixels of the overlay): it's copied
/// and its text read. Without capture exclusion the overlay steps aside
/// for the copy and comes back (the page is already in its result view).
#[tauri::command]
pub async fn capture_area(
    state: State<'_, Arc<AppState>>,
    window: WebviewWindow,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    lang: String,
) -> Result<hl_win::ocr::Recognized, hl_win::ocr::OcrError> {
    let failed = |e: String| hl_win::ocr::OcrError::Failed(e);
    let rect = to_screen(&window, &Rect { x, y, w, h }).map_err(failed)?;
    let excluded = EXCLUDED.load(Ordering::Relaxed);
    if !excluded {
        let _ = window.hide();
    }
    let png = tauri::async_runtime::spawn_blocking(move || {
        if !excluded {
            // Let the compositor take the overlay off the screen first.
            std::thread::sleep(Duration::from_millis(120));
        }
        hl_win::capture::capture_png(rect.0, rect.1, rect.2, rect.3)
    })
    .await
    .map_err(|e| failed(e.to_string()))?;
    if !excluded {
        let _ = window.show();
        windows::focus(&window);
    }
    let png = png.ok_or_else(|| failed("the screen couldn't be copied".into()))?;
    crate::commands::recognize(&state, png, &lang).await
}

/// What the pinned area's page hears.
#[derive(Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum LiveEvent {
    /// The area's picture changed and settled: its text now.
    Text { ocr: hl_win::ocr::Recognized },
    Error { message: String },
}

/// How often the pinned area is looked at.
const LIVE_TICK: Duration = Duration::from_millis(330);
/// This much of the area changed since the last reading: read again…
const LIVE_CHANGED: f32 = 0.004;
/// …once two looks in a row differ by less than this (text done typing).
const LIVE_STILL: f32 = 0.002;

/// «Закрепить»: keeps reading the area while it's pinned. The overlay
/// lets clicks through (but over its controls, see `capture_hits`) and
/// the app underneath gets focus back.
#[tauri::command]
pub async fn capture_live(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    window: WebviewWindow,
    area: Rect,
    lang: String,
    on_event: Channel<LiveEvent>,
) -> Result<(), String> {
    if !EXCLUDED.load(Ordering::Relaxed) {
        return Err("unsupported".into());
    }
    let (x, y, w, h) = to_screen(&window, &area)?;
    stop_live(&app);
    PINNED.store(true, Ordering::Relaxed);
    set_interactive(&window, false);
    let target = *TARGET.lock().unwrap();
    if !target.is_null() {
        hl_win::window::force_foreground(target);
    }
    let state = state.inner().clone();
    let task = tauri::async_runtime::spawn(async move {
        let mut last_read: Option<Vec<u8>> = None;
        let mut prev: Option<Vec<u8>> = None;
        loop {
            tokio::time::sleep(LIVE_TICK).await;
            let Ok(Some(px)) = tauri::async_runtime::spawn_blocking(move || hl_win::capture::capture_rect(x, y, w, h)).await
            else {
                continue;
            };
            let (uw, uh) = (w as usize, h as usize);
            let moved = last_read.as_ref().is_none_or(|l| hl_win::capture::difference(l, &px, uw, uh) > LIVE_CHANGED);
            let still = prev.as_ref().is_some_and(|p| hl_win::capture::difference(p, &px, uw, uh) < LIVE_STILL);
            prev = Some(px.clone());
            if !(moved && still) {
                continue;
            }
            last_read = Some(px.clone());
            let Some(png) = tauri::async_runtime::spawn_blocking(move || hl_win::capture::encode_png(w as u32, h as u32, &px))
                .await
                .ok()
                .flatten()
            else {
                continue;
            };
            let event = match crate::commands::recognize(&state, png, &lang).await {
                Ok(ocr) => LiveEvent::Text { ocr },
                Err(e) => LiveEvent::Error { message: format!("{e:?}") },
            };
            if on_event.send(event).is_err() {
                break;
            }
        }
    });
    *LIVE.lock().unwrap() = Some(task);
    start_hit_watch(app);
    Ok(())
}

/// The pinned overlay's controls (CSS pixels): clicks there reach the page.
#[tauri::command]
pub async fn capture_hits(window: WebviewWindow, rects: Vec<Rect>) -> Result<(), ()> {
    let list = rects
        .iter()
        .filter_map(|r| to_screen(&window, r).ok())
        .map(|(x, y, w, h)| (x, y, x + w, y + h))
        .collect();
    *HITS.lock().unwrap() = list;
    Ok(())
}

/// While pinned: the overlay takes clicks only over its controls, so the
/// game underneath gets every other click.
fn start_hit_watch(app: AppHandle) {
    static RUNNING: AtomicBool = AtomicBool::new(false);
    if RUNNING.swap(true, Ordering::Relaxed) {
        return;
    }
    std::thread::Builder::new()
        .name("capture-hits".into())
        .spawn(move || {
            let mut through = None;
            while PINNED.load(Ordering::Relaxed) {
                let (cx, cy) = hl_win::window::cursor_pos();
                let over = HITS.lock().unwrap().iter().any(|&(l, t, r, b)| cx >= l && cx < r && cy >= t && cy < b);
                if through != Some(!over)
                    && let Some(w) = app.get_webview_window(CAPTURE)
                    && let Ok(h) = w.hwnd()
                {
                    hl_win::window::set_click_through(hl_win::Hwnd(h.0 as isize), !over);
                    through = Some(!over);
                }
                std::thread::sleep(Duration::from_millis(40));
            }
            RUNNING.store(false, Ordering::Relaxed);
        })
        .ok();
}

/// Picking needs clicks and keys; pinned, the overlay never takes focus.
fn set_interactive(w: &WebviewWindow, on: bool) {
    if let Ok(h) = w.hwnd() {
        let h = hl_win::Hwnd(h.0 as isize);
        hl_win::window::set_no_activate(h, !on);
        if on {
            hl_win::window::set_click_through(h, false);
        }
    }
}

fn stop_live(app: &AppHandle) {
    PINNED.store(false, Ordering::Relaxed);
    HITS.lock().unwrap().clear();
    if let Some(task) = LIVE.lock().unwrap().take() {
        task.abort();
    }
    if let Some(w) = app.get_webview_window(CAPTURE) {
        set_interactive(&w, true);
    }
}

/// «Открепить»: back to a still result (the overlay takes clicks again).
#[tauri::command]
pub async fn capture_unpin(app: AppHandle) -> Result<(), ()> {
    stop_live(&app);
    Ok(())
}

#[tauri::command]
pub async fn capture_cancel(app: AppHandle, window: WebviewWindow) -> Result<(), ()> {
    stop_live(&app);
    let _ = window.hide();
    Ok(())
}
