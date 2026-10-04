//! Settings → Offline & acceleration: the offline model manager
//! (Figma 329:369): where models run, the six tiers with download
//! progress, and the model folder.
//!
//! Statuses are read from the model folder through a small cache (folder
//! sizes are walked at most once a second); downloads run in
//! `offline::manager` and the page repaints while one is running.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use egui::Ui;

use super::*;
use crate::offline::manager::{self, Status};
use crate::offline::{self, Tier, catalog, sys};
use crate::settings::{OfflineDevice, OfflinePrecision};

/// How long an armed "Delete" waits for the second click.
const DELETE_CONFIRM: Duration = Duration::from_secs(4);
/// How often installed sizes and free space are re-read.
const REFRESH: Duration = Duration::from_secs(1);

#[derive(Default)]
struct Cache {
    at: Option<Instant>,
    dir: PathBuf,
    status: HashMap<Tier, Status>,
    free: Option<u64>,
    /// "Delete" clicked once on this tier.
    armed: Option<(Tier, Instant)>,
}

thread_local! {
    static CACHE: RefCell<Cache> = RefCell::new(Cache::default());
}

/// A folder chosen in the picker (it runs on its own thread).
static PICKED: Mutex<Option<PathBuf>> = Mutex::new(None);

fn refresh(dir: &PathBuf, force: bool) {
    CACHE.with_borrow_mut(|c| {
        let stale = c.at.is_none_or(|t| t.elapsed() >= REFRESH) || c.dir != *dir;
        if !(force || stale || manager::any_running()) {
            return;
        }
        c.status = Tier::ALL.into_iter().map(|t| (t, manager::status(dir, t))).collect();
        c.free = sys::free_disk(dir);
        c.dir = dir.clone();
        c.at = Some(Instant::now());
    });
}

fn status(tier: Tier) -> Status {
    CACHE.with_borrow(|c| c.status.get(&tier).cloned().unwrap_or(Status::NotInstalled { partial: false }))
}

/// Installed, as the Providers page shows it (cached like this page).
pub(super) fn installed(s: &Settings, kind: ProviderKind) -> bool {
    let Some(tier) = kind.tier() else { return true };
    refresh(&offline::model_dir(&s.model_dir), false);
    matches!(status(tier), Status::Installed { .. })
}

/// "1.2 GB", "352 MB".
pub(super) fn size_text(bytes: u64) -> String {
    let mb = bytes as f64 / 1e6;
    if mb >= 1000.0 {
        format!("{:.1} {}", mb / 1000.0, tr("GB"))
    } else {
        format!("{:.0} {}", mb.max(1.0), tr("MB"))
    }
}

fn describe(tier: Tier) -> &'static str {
    match tier {
        Tier::SuperMegaFast => tr("English ↔ Russian · the fastest, starts instantly"),
        Tier::SuperFast => tr("Several languages through English · fast"),
        Tier::Fast => tr("200 languages · good quality"),
        Tier::Normal => tr("200 languages · better quality, slower"),
        Tier::Medium => tr("55 languages · uses context and the glossary · graphics card recommended"),
        Tier::Heavy => tr("55 languages · the best quality · needs about 8 GB of video memory"),
    }
}

pub fn show(ui: &mut Ui, s: &mut Settings, _state: &mut SettingsState) -> Option<SettingsAction> {
    let mut action = None;
    let dir = offline::model_dir(&s.model_dir);
    refresh(&dir, false);
    if manager::any_running() {
        ui.ctx().request_repaint_after(Duration::from_millis(250));
    }
    if let Some(p) = PICKED.lock().unwrap().take() {
        s.model_dir = if p == offline::default_model_dir() { String::new() } else { p.display().to_string() };
        action = Some(SettingsAction::Changed);
    }

    page_title(
        ui,
        tr("Offline & acceleration"),
        Some(tr("Local models work without the internet and run on the processor or graphics card.")),
    );

    let gpu = sys::gpu();
    section(ui, tr("Device"), |ui| {
        let sub = match &gpu {
            Some(g) => format!("{} · {:.0} {}", g.name, g.vram_gb(), tr("GB")),
            None => tr("No suitable graphics card found").to_owned(),
        };
        const DEVICES: [OfflineDevice; 3] = [OfflineDevice::Auto, OfflineDevice::Cpu, OfflineDevice::Gpu];
        row(ui, tr("Run models on"), Some(&sub), 250.0, false, |ui| {
            let active = DEVICES.iter().position(|d| *d == s.offline_device).unwrap_or(0);
            let opts = [tr("Auto"), tr("Processor"), tr("Graphics card")];
            if let Some(i) = segmented_sm(ui, &opts, active, &[true, true, gpu.is_some()], tr("No suitable graphics card found")) {
                s.offline_device = DEVICES[i];
                action = Some(SettingsAction::Changed);
            }
        });

        let max = sys::logical_cores().max(1) as u32;
        let auto = sys::physical_cores();
        let value = if s.offline_threads == 0 {
            trf("Auto ({n})", &[("n", &auto.to_string())])
        } else {
            s.offline_threads.to_string()
        };
        row(ui, tr("Processor threads"), Some(tr("For models on the processor; Auto uses the physical cores")), 200.0, false, |ui| {
            ui.add_sized(vec2(64.0, 20.0), egui::Label::new(rt(&value, 11.5, Weight::Medium, TEXT_2)).selectable(false));
            let resp = slider(ui, &mut s.offline_threads, 0..=max, 1, 120.0);
            if resp.drag_stopped() || resp.clicked() {
                action = Some(SettingsAction::Changed);
            }
        });

        const PRECISIONS: [OfflinePrecision; 4] =
            [OfflinePrecision::Auto, OfflinePrecision::Int8, OfflinePrecision::Int8Float16, OfflinePrecision::Float32];
        row(ui, tr("Precision"), Some(tr("OPUS-MT, Argos and NLLB. Gemma models use their own")), 280.0, true, |ui| {
            let active = PRECISIONS.iter().position(|p| *p == s.offline_precision).unwrap_or(0);
            let opts = [tr("Auto"), "int8", "int8 + fp16", "float32"];
            if let Some(i) = segmented_sm(ui, &opts, active, &[true, true, gpu.is_some(), true], tr("Graphics card only")) {
                s.offline_precision = PRECISIONS[i];
                action = Some(SettingsAction::Changed);
            }
        });
    });

    section(ui, tr("Models"), |ui| {
        for (i, tier) in Tier::ALL.into_iter().enumerate() {
            if let Some(a) = model_row(ui, &dir, tier, i + 1 == Tier::ALL.len()) {
                action = Some(a);
            }
        }
    });

    section(ui, tr("Storage"), |ui| {
        let free = CACHE.with_borrow(|c| c.free);
        let dir_text = dir.display().to_string();
        let sub = match free {
            Some(f) => format!("{dir_text}\n{}", trf("{size} free on this drive", &[("size", &size_text(f))])),
            None => dir_text,
        };
        row(ui, tr("Model folder"), Some(&sub), 190.0, true, |ui| {
            if link_button(ui, None, tr("Open")).clicked() {
                let _ = std::fs::create_dir_all(&dir);
                crate::win::open_url(&dir.display().to_string());
            }
            if outline_button(ui, tr("Change…"), 26.0).clicked() {
                let start = dir.clone();
                let title = tr("Folder for offline models");
                std::thread::spawn(move || {
                    if let Some(p) = sys::pick_folder(title, &start) {
                        *PICKED.lock().unwrap() = Some(p);
                    }
                });
            }
        });
    });
    footnote(
        ui,
        tr("Models download only when you click Download. Gemma models also download the llama.cpp engine once. Already downloaded models stay in the old folder when you change it."),
    );
    action
}

fn model_row(ui: &mut Ui, dir: &PathBuf, tier: Tier, last: bool) -> Option<SettingsAction> {
    let mut action = None;
    let st = status(tier);
    let title = format!("{} · {}", tier.kind().short_name(), tier.label());
    let size = catalog::download_size(dir, tier);
    let sub = match &st {
        Status::Installed { size } => match catalog::runtime_info(dir).filter(|_| tier.runtime() == offline::Runtime::Llama) {
            Some(rt) => format!("{} · llama.cpp {} · {}", size_text(*size), rt.backend.label(), describe(tier)),
            None => format!("{} · {}", size_text(*size), describe(tier)),
        },
        Status::Downloading { done, total } => {
            trf("Downloading {done} of {total}", &[("done", &size_text(*done)), ("total", &size_text((*total).max(*done)))])
        }
        Status::Failed(e) => crate::i18n::tr_text(e).to_owned(),
        Status::NotInstalled { .. } => format!("{} · {}", size_text(size), describe(tier)),
    };
    let armed = CACHE.with_borrow(|c| c.armed.filter(|(t, at)| *t == tier && at.elapsed() < DELETE_CONFIRM).is_some());
    row(ui, &title, Some(&sub), 230.0, last, |ui| match &st {
        Status::NotInstalled { partial } => {
            let label = if *partial { tr("Resume") } else { tr("Download") };
            if outline_button(ui, label, 26.0).clicked() {
                manager::install(dir.clone(), tier);
                refresh(dir, true);
            }
        }
        Status::Downloading { done, total } => {
            if link_button(ui, None, tr("Cancel")).clicked() {
                manager::cancel(tier);
            }
            let p = if *total > 0 { (*done as f32 / *total as f32).clamp(0.0, 1.0) } else { 0.0 };
            ui.add_sized(vec2(36.0, 20.0), egui::Label::new(rt(format!("{:.0}%", p * 100.0), 11.0, Weight::Medium, TEXT_2)).selectable(false));
            progress(ui, p, 110.0);
        }
        Status::Installed { .. } => {
            let label = if armed { tr("Click again to delete") } else { tr("Delete") };
            let loaded = offline::loaded(tier);
            if link_button(ui, None, label).clicked() {
                if armed {
                    // A file still in use stays; the row then shows what is left.
                    let _ = manager::delete(dir, tier);
                    CACHE.with_borrow_mut(|c| c.armed = None);
                    refresh(dir, true);
                    action = Some(SettingsAction::Changed);
                } else {
                    CACHE.with_borrow_mut(|c| c.armed = Some((tier, Instant::now())));
                    ui.ctx().request_repaint_after(DELETE_CONFIRM);
                }
            }
            if loaded {
                // In memory now: it loaded when it was used; free it here.
                if link_button(ui, None, tr("Unload")).on_hover_text(tr("Free the memory it uses; it loads again when used")).clicked() {
                    offline::unload(tier);
                }
                badge(ui, tr("Loaded"), OK, OK_BG, Some(icons::DOT_OK));
            } else {
                badge(ui, tr("Installed"), OK, OK_BG, Some(icons::DOT_OK));
            }
        }
        Status::Failed(_) => {
            if outline_button(ui, tr("Retry"), 26.0).clicked() {
                manager::dismiss(tier);
                manager::install(dir.clone(), tier);
                refresh(dir, true);
            }
            badge(ui, tr("Error"), WARN, WARN_BG, Some(icons::DOT_WARN));
        }
    });
    action
}

/// A thin progress bar (track and accent fill, radius 2).
fn progress(ui: &mut Ui, p: f32, width: f32) {
    let (rect, _) = ui.allocate_exact_size(vec2(width, 6.0), Sense::hover());
    ui.painter().rect_filled(rect, 3, SWITCH_OFF_TRACK);
    let fill = Rect::from_min_size(rect.min, vec2(rect.width() * p, rect.height()));
    ui.painter().rect_filled(fill, 3, ACCENT);
}
