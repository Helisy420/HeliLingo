// GUI subsystem in every build: a debug build must not open a console
// window behind the app. `--translate` attaches to the parent console.
#![windows_subsystem = "windows"]

//! HeliLingo — select text in any app, double-tap Ctrl, read the
//! translation in a popup next to it.

/// `HELILINGO_DEBUG_TIMING=1`: log milestones of opening windows (ms since
/// start) to stderr, for finding latency.
pub fn timing(what: &str) {
    use std::sync::OnceLock;
    use std::time::Instant;
    static ON: OnceLock<Option<Instant>> = OnceLock::new();
    if let Some(t0) = ON.get_or_init(|| std::env::var("HELILINGO_DEBUG_TIMING").is_ok_and(|v| v == "1").then(Instant::now)) {
        eprintln!("timing {:>8.1} ms  {what}", t0.elapsed().as_secs_f64() * 1000.0);
    }
}

/// Product name: window titles, tray tooltip, About page.
pub const APP_NAME: &str = "HeliLingo";

mod app;
mod code;
mod history;
mod offline;
mod i18n;
mod icons;
mod settings;
mod stats;
mod theme;
mod translate;
mod ultra;
mod ui;
mod web;
mod wiki;
mod win;

fn main() -> eframe::Result {
    let args: Vec<String> = std::env::args().collect();

    // A worker process hosting one offline model (see `offline::worker`).
    if let Some(i) = args.iter().position(|a| a == "--offline-worker") {
        offline::worker::serve(args.get(i + 1).map_or("", String::as_str));
        return Ok(());
    }

    // `helilingo --translate <text> [target] [--provider google|yandex|bing|deepl]`
    // and `helilingo --check <provider>`: engine checks from a console.
    // `helilingo --install <tier>` (opus, argos, nllb600, nllb13, gemma4,
    // gemma12 or 1–6): download an offline model from a console.
    if let Some(i) = args.iter().position(|a| a == "--install") {
        win::attach_parent_console();
        let Some(tier) = args.get(i + 1).and_then(|t| offline::Tier::from_id(t)) else {
            println!("usage: --install opus|argos|nllb600|nllb13|gemma4|gemma12");
            return Ok(());
        };
        let models = offline::model_dir(&settings::Settings::load().model_dir);
        println!("installing {} into {}", tier.id(), models.display());
        let start = std::time::Instant::now();
        let mut last = std::time::Instant::now();
        let result = offline::manager::install_blocking(&models, tier, |done, total| {
            if last.elapsed() > std::time::Duration::from_secs(2) {
                last = std::time::Instant::now();
                println!("  {:.1} / {:.1} MB", done as f64 / 1e6, total as f64 / 1e6);
            }
        });
        match result {
            Ok(()) => println!("installed in {:.1} s", start.elapsed().as_secs_f64()),
            Err(e) => println!("failed: {e}"),
        }
        return Ok(());
    }
    if let Some(i) = args.iter().position(|a| a == "--translate" || a == "--check") {
        win::attach_parent_console();
        cli_translate(&args[i..]);
        return Ok(());
    }
    let preview = args
        .iter()
        .position(|a| a == "--preview")
        .and_then(|i| args.get(i + 1).cloned());

    if preview.is_none() && !win::single_instance() {
        return Ok(());
    }
    if preview.is_some() {
        settings::set_read_only();
    }
    let settings = settings::Settings::load();
    i18n::set_lang(&settings.ui_lang);
    history::set_enabled(settings.save_history);
    stats::set_enabled(settings.keep_stats);
    wiki::set_enabled(settings.show_wiki);
    // Settings → General → Hardware acceleration (applies on restart).
    // Off is a lighter mode: no vsync wait and no animations. A real
    // software renderer isn't possible: Windows' built-in software OpenGL
    // is version 1.1 and egui needs 2.0+ (the app wouldn't start).
    let light = !settings.hardware_acceleration;
    let mut options = eframe::NativeOptions {
        viewport: app::popup_viewport(),
        ..Default::default()
    };
    options.glow_options.vsync = !light;
    eframe::run_native(
        APP_NAME,
        options,
        Box::new(move |cc| {
            let mut app = app::App::new(cc, settings);
            if let Some(what) = preview.as_deref() {
                app.preview(what);
            }
            Ok(Box::new(app))
        }),
    )
}

/// `--translate` / `--check` from a console. `--provider` forces a single
/// provider (no fallback); keys come from settings.json.
fn cli_translate(args: &[String]) {
    use settings::{ProviderEntry, ProviderKind};

    let parse_kind = |name: &str| {
        let kind = ProviderKind::from_cli(name);
        if kind.is_none() {
            println!("unknown provider: {name} (google, yandex, bing, deepl, opus, argos, nllb600, nllb13, gemma4, gemma12)");
        }
        kind
    };
    let mut config: translate::EngineConfig = (&settings::Settings::load()).into();
    if args[0] == "--check" {
        let Some(kind) = parse_kind(args.get(1).map_or("", String::as_str)) else { return };
        let engine = translate::Engine::new(config.clone());
        match engine.check(kind, &config.keys) {
            Ok(()) => println!("{}: OK", kind.short_name()),
            Err(e) => println!("{}: error: {e:?}", kind.short_name()),
        }
        return;
    }
    let mut positional = Vec::new();
    let mut forced = None;
    let mut rest = args[1..].iter();
    while let Some(a) = rest.next() {
        if a == "--provider" {
            match parse_kind(rest.next().map_or("", String::as_str)) {
                Some(kind) => forced = Some(kind),
                None => return,
            }
        } else {
            positional.push(a.clone());
        }
    }
    if let Some(kind) = forced {
        config.providers = vec![ProviderEntry { kind, enabled: true }];
    }
    let text = positional.first().cloned().unwrap_or_default();
    let to = positional.get(1).cloned().unwrap_or_else(|| "ru".into());
    let engine = translate::Engine::new(config);
    let start = std::time::Instant::now();
    // Code is translated the programmer-mode way (strings and comments).
    let result = code::translate(&engine, &text, "auto", &to, forced)
        .unwrap_or_else(|| engine.translate_using(&translate::normalize(&text), "auto", &to, forced));
    match result {
        Ok(t) => println!("{t:#?}\nprovider: {}\n{:?}", t.provider.short_name(), start.elapsed()),
        Err(e) => println!("error: {e:?}"),
    }
}

