// Release builds are GUI-only: no console window behind the app.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! HeliLingo — select text in any app, press a shortcut, read the
//! translation as it streams in.

mod actions;
mod api;
mod apply;
mod capture;
mod commands;
mod files;
mod hotkeys;
mod logging;
mod selection;
mod settings;
mod state;
mod stream;
mod tray;
mod ultra;
mod update;
mod windows;

use std::sync::Arc;

use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};

fn main() {
    // An offline model worker (a copy of this exe, see hl_offline::worker):
    // serve translations on stdin/stdout, nothing else.
    let args: Vec<String> = std::env::args().collect();
    if let Some(i) = args.iter().position(|a| a == hl_offline::worker::FLAG) {
        hl_offline::worker::serve(args.get(i + 1).map_or("", String::as_str));
        return;
    }

    logging::init();
    let settings = settings::Settings::load();
    tracing::info!(version = env!("CARGO_PKG_VERSION"), "start");

    // Weak or remote PCs: let WebView2 draw in software.
    if !settings.hardware_acceleration {
        // SAFETY: before any other thread exists.
        unsafe { std::env::set_var("WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS", "--disable-gpu") };
    }
    hl_detect::warm_up();

    tauri::Builder::default()
        // A second start opens the Quick window of the running one;
        // `helilingo --quick "text"` opens it with that text, and
        // `helilingo --capture [window]` starts the screen-area overlay (a
        // game launcher's shortcut, a stream deck button).
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            if let Some(i) = args.iter().position(|a| a == "--capture") {
                let mode = if args.get(i + 1).is_some_and(|m| m == "window") { capture::Mode::Window } else { capture::Mode::Area };
                // Off the main thread: the first start creates a webview.
                let app = app.clone();
                std::thread::spawn(move || capture::start(&app, mode));
                return;
            }
            let text = args.iter().position(|a| a == "--quick").and_then(|i| args.get(i + 1).cloned());
            windows::show_quick(app, text);
        }))
        .plugin(tauri_plugin_global_shortcut::Builder::new().with_handler(hotkeys::handle).build())
        .plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, None))
        .manage(Arc::new(state::AppState::new(settings)))
        .invoke_handler(tauri::generate_handler![
            commands::translate,
            commands::cancel,
            commands::engines,
            commands::replace_selection,
            commands::get_settings,
            commands::update_settings,
            commands::set_key,
            commands::ready,
            commands::run_action,
            commands::open_settings,
            commands::quit,
            commands::history_list,
            commands::history_star,
            commands::history_delete,
            commands::history_clear,
            commands::stats_summary,
            commands::stats_reset,
            commands::cache_clear,
            commands::power,
            commands::set_enabled,
            commands::pause,
            commands::check_engines,
            commands::glossary_count,
            commands::openrouter_models,
            commands::open_glossary,
            commands::open_url,
            commands::wiki_lookup,
            commands::net_status,
            capture::capture_area,
            capture::capture_cancel,
            capture::capture_live,
            capture::capture_state,
            update::update_check,
            update::update_install,
            capture::capture_hits,
            capture::capture_unpin,
            commands::clipboard_has_image,
            commands::clipboard_image,
            commands::image_file,
            commands::ocr_image,
            commands::ultra,
            commands::open_in_translator,
            commands::popup_layout,
            commands::popup_hide,
            commands::popup_pin,
            commands::popup_pinned,
            commands::popup_drag,
            commands::translate_quiet,
            commands::copy_text,
            commands::offline_status,
            commands::offline_install,
            commands::offline_cancel,
            commands::offline_delete,
            commands::offline_dir,
            commands::offline_pick_dir,
            commands::offline_open_dir,
            commands::files_dir,
            files::translate_batch,
            files::files_info,
            files::files_choose,
            files::files_from_clipboard,
            files::file_translate,
            files::file_cancel,
            files::file_save,
            files::files_save_all,
            files::files_open_dir,
            commands::files_pick_dir,
        ])
        .setup(|app| {
            let handle = app.handle();
            tray::create(handle)?;
            windows::create_startup(handle)?;
            selection::start(handle);
            update::start(handle);
            // The pinned bubble remembers where it was dragged.
            if let Some(popup) = handle.get_webview_window(windows::POPUP) {
                let h = handle.clone();
                popup.on_window_event(move |e| {
                    if let tauri::WindowEvent::Moved(p) = e {
                        selection::moved(&h, p.x, p.y);
                    }
                });
            }
            let settings = handle.state::<Arc<state::AppState>>().settings.read().unwrap().clone();
            apply::side_effects(handle, None, &settings);
            selection::restore_pin(&settings);
            if !settings.welcomed {
                windows::show(handle, windows::WELCOME);
            }
            if cfg!(debug_assertions) && std::env::var_os("HELILINGO_GALLERY").is_some() {
                // Dev: the component sheet, to compare with Figma.
                WebviewWindowBuilder::new(app, "gallery", WebviewUrl::App("gallery.html".into()))
                    .title("HeliLingo — компоненты")
                    .inner_size(1480.0, 1150.0)
                    .build()?;
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("failed to start HeliLingo")
        .run(|_app, event| match event {
            // A tray app keeps running with every window hidden.
            tauri::RunEvent::ExitRequested { api, code: None, .. } => api.prevent_exit(),
            tauri::RunEvent::Exit => {
                hl_offline::shutdown();
                api::shutdown();
            }
            _ => {}
        });
}
