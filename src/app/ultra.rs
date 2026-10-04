//! Ultra mode (Ctrl+Alt+U): copies the selection, splits it into fragments,
//! detects each fragment's language and translates the ones not already in
//! the target language. Shows the Ultra window, or (when the window is
//! turned off in Settings) replaces the selection straight away.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use egui::{ViewportBuilder, ViewportCommand, ViewportId};

use super::{App, COPY_TIMEOUT, Floating, UiEvent, send_ui, window_icon};
use crate::history::{self, Mode};
use crate::settings::{ProviderKind, Settings, UltraOptions};
use crate::translate::{self, Engine};
use crate::ui::ultra::{self, UltraAction, UltraState, UltraView};
use crate::ultra::{self as logic, Fragment, FragmentState};
use crate::win::{self, Hwnd, clipboard};

const ULTRA_TITLE: &str = "HeliLingo — Ultra";
/// Fragments translated at the same time. Kept low: the keyless
/// providers rate-limit per IP, and a burst of parallel requests is what
/// trips Google's "busy" answer.
const WORKERS: usize = 2;

fn viewport_id() -> ViewportId {
    ViewportId::from_hash_of("qt-ultra")
}

pub(super) struct UltraWindow {
    win: Floating,
    model: Arc<Mutex<UltraModel>>,
    /// For Esc: the most recently opened window closes first.
    pub(super) opened: Instant,
}

struct UltraModel {
    state: UltraState,
    lead: String,
    fragments: Vec<Fragment>,
    to: String,
    opts: UltraOptions,
    prefer: Option<ProviderKind>,
    providers: Vec<ProviderKind>,
    /// Provider of the latest answer, for the chip.
    provider: Option<ProviderKind>,
    /// The app the text was selected in (for Replace).
    target: Hwnd,
    engine: Arc<Engine>,
    generation: u64,
    copied_at: Option<Instant>,
    /// Open animation on, and whether the window is on screen yet.
    animate: bool,
    shown: bool,
    size: [f32; 2],
}

fn same_lang(a: &str, b: &str) -> bool {
    let base = |s: &str| s.split('-').next().unwrap_or(s).to_ascii_lowercase();
    base(a) == base(b)
}

/// Translates every fragment again (on open and when the provider changes).
fn run(model: &Arc<Mutex<UltraModel>>, ctx: &egui::Context, headless: bool) {
    let (generation, engine, texts, to, opts, prefer) = {
        let mut m = model.lock().unwrap();
        m.generation += 1;
        for f in &mut m.fragments {
            f.state = FragmentState::Pending;
            f.lang = None;
        }
        let texts: Vec<String> = m.fragments.iter().map(|f| f.text.clone()).collect();
        (m.generation, m.engine.clone(), texts, m.to.clone(), m.opts, m.prefer)
    };
    let (model, ctx) = (model.clone(), ctx.clone());
    std::thread::spawn(move || {
        // One language for the whole selection unless each fragment is detected.
        let common = if opts.detect_each {
            None
        } else {
            let whole = texts.join(" ");
            engine.translate_using(&whole, "auto", &to, prefer).ok().and_then(|t| t.src_lang.clone())
        };
        let next = Arc::new(AtomicUsize::new(0));
        let workers: Vec<_> = (0..WORKERS.min(texts.len()))
            .map(|_| {
                let (next, texts, engine, model, ctx, to, common) =
                    (next.clone(), texts.clone(), engine.clone(), model.clone(), ctx.clone(), to.clone(), common.clone());
                std::thread::spawn(move || {
                    loop {
                        let i = next.fetch_add(1, Ordering::Relaxed);
                        let Some(text) = texts.get(i) else { break };
                        let from = common.as_deref().unwrap_or("auto");
                        let result = engine.translate_using(text, from, &to, prefer);
                        let mut m = model.lock().unwrap();
                        if m.generation != generation {
                            return;
                        }
                        let f = &mut m.fragments[i];
                        match result {
                            Ok(t) => {
                                let lang = t.src_lang.clone().or_else(|| common.clone()).unwrap_or_else(|| "auto".into());
                                f.state = if opts.skip_target && same_lang(&lang, &to) {
                                    FragmentState::Unchanged
                                } else {
                                    FragmentState::Translated(t.text.clone())
                                };
                                f.lang = Some(lang);
                                m.provider = Some(t.provider);
                            }
                            Err(e) => {
                                f.lang = common.clone().or_else(|| Some("auto".into()));
                                f.state = FragmentState::Failed(e);
                            }
                        }
                        drop(m);
                        ctx.request_repaint_of(viewport_id());
                    }
                })
            })
            .collect();
        for w in workers {
            let _ = w.join();
        }
        let m = model.lock().unwrap();
        if m.generation != generation {
            return;
        }
        let source = logic::join(&m.lead, &m.fragments.iter().cloned().map(|mut f| {
            f.state = FragmentState::Unchanged;
            f
        }).collect::<Vec<_>>());
        let output = logic::join(&m.lead, &m.fragments);
        let langs = logic::summary(&m.fragments);
        let from = langs.first().map_or("auto".to_owned(), |g| g.lang.clone());
        if let Some(p) = m.provider {
            history::add(&source, &output, &from, &m.to, p, Mode::Ultra);
        }
        let target = m.target;
        drop(m);
        if headless {
            clipboard::paste_text(target, &output);
        }
    });
}

fn usable_providers(s: &Settings) -> Vec<ProviderKind> {
    s.providers
        .iter()
        .filter(|p| p.enabled && s.keys.usable(p.kind))
        .map(|p| p.kind)
        .collect()
}

impl App {
    /// Reads the selection of the foreground app on a worker thread.
    pub(super) fn start_ultra(&mut self) {
        let (tx, ctx) = (self.ui_tx.clone(), self.ctx.clone());
        self.hide_popup();
        self.engine.warm_up();
        std::thread::spawn(move || {
            let target = win::foreground();
            let Some(text) = clipboard::copy_selection(target, COPY_TIMEOUT) else { return };
            let text: String = text.chars().take(translate::MAX_CHARS).collect();
            send_ui(&ctx, &tx, UiEvent::UltraGrabbed { text, target });
        });
    }

    pub(super) fn open_ultra(&mut self, text: &str, target: Hwnd) {
        let (lead, fragments) = logic::split(text);
        if fragments.is_empty() {
            return;
        }
        let s = &self.settings;
        let model = Arc::new(Mutex::new(UltraModel {
            state: UltraState::default(),
            lead,
            fragments,
            to: s.target.clone(),
            opts: s.ultra,
            prefer: None,
            providers: usable_providers(s),
            provider: None,
            target,
            engine: self.engine.clone(),
            generation: 0,
            copied_at: None,
            animate: s.animations && s.hardware_acceleration && win::system_animations(),
            shown: false,
            size: [0.0, 0.0],
        }));
        let headless = !s.ultra.show_window;
        run(&model, &self.ctx, headless);
        if headless {
            return;
        }
        self.ultra = Some(UltraWindow { win: Floating::new(), model, opened: Instant::now() });
    }

    /// Esc: close the provider menu, else stop fragments still being
    /// translated (they keep their original text), else close the window.
    pub(super) fn ultra_escape(&mut self) {
        let Some(u) = &self.ultra else { return };
        let mut m = u.model.lock().unwrap();
        if m.state.escape() {
            drop(m);
            self.ctx.request_repaint_of(viewport_id());
            return;
        }
        let pending = m.fragments.iter().any(|f| matches!(f.state, FragmentState::Pending));
        if pending {
            m.generation += 1;
            for f in &mut m.fragments {
                if matches!(f.state, FragmentState::Pending) {
                    f.state = FragmentState::Failed(crate::translate::Error::Other(crate::i18n::tr("Cancelled").into()));
                }
            }
            drop(m);
            self.ctx.request_repaint_of(viewport_id());
            return;
        }
        drop(m);
        self.ultra = None;
    }

    pub(super) fn on_ultra_action(&mut self, action: UltraAction) {
        let Some(u) = &self.ultra else { return };
        let model = u.model.clone();
        match action {
            UltraAction::Close => self.ultra = None,
            UltraAction::CopyAll => {
                let mut m = model.lock().unwrap();
                let text = logic::join(&m.lead, &m.fragments);
                m.copied_at = Some(Instant::now());
                std::thread::spawn(move || clipboard::set_text(&text, false));
            }
            UltraAction::Replace => {
                let m = model.lock().unwrap();
                let (text, target) = (logic::join(&m.lead, &m.fragments), m.target);
                drop(m);
                self.ultra = None;
                std::thread::spawn(move || {
                    std::thread::sleep(Duration::from_millis(60));
                    clipboard::paste_text(target, &text);
                });
            }
            UltraAction::Provider(p) => {
                model.lock().unwrap().prefer = p;
                run(&model, &self.ctx, false);
            }
        }
    }

    pub(super) fn ultra_frame(&mut self, ctx: &egui::Context) {
        let Some(u) = &mut self.ultra else { return };
        let initial = {
            let m = u.model.lock().unwrap();
            if m.size[0] > 0.0 { m.size } else { [ultra::CARD_W + 2.0 * ultra::MARGIN_X, 640.0] }
        };
        let builder = ViewportBuilder::default()
            .with_title(ULTRA_TITLE)
            .with_icon(window_icon())
            .with_inner_size(initial)
            .with_decorations(false)
            .with_transparent(true)
            .with_resizable(false)
            .with_always_on_top()
            .with_taskbar(false)
            .with_visible(false);
        let (model, tx) = (u.model.clone(), self.ui_tx.clone());
        ctx.show_viewport_deferred(viewport_id(), builder, move |ui, _| {
            let ctx = ui.ctx().clone();
            let mut guard = model.lock().unwrap();
            let m = &mut *guard;
            let groups = logic::summary(&m.fragments);
            let copied = m.copied_at.is_some_and(|t| t.elapsed() < Duration::from_millis(1400));
            if copied {
                ctx.request_repaint_after(Duration::from_millis(200));
            }
            let provider = m
                .provider
                .or(m.prefer)
                .or(m.providers.first().copied())
                .map_or("Google", |p| p.short_name());
            let view = UltraView {
                fragments: &m.fragments,
                groups: &groups,
                provider,
                providers: &m.providers,
                prefer: m.prefer,
                copied,
                animate: m.animate,
                shown: m.shown,
            };
            let action = ultra::show(ui, &mut m.state, &view);
            let close = ui.input(|i| i.viewport().close_requested());
            if let Some(a) = action {
                send_ui(&ctx, &tx, UiEvent::Ultra(a));
            } else if close {
                send_ui(&ctx, &tx, UiEvent::Ultra(UltraAction::Close));
            }
            let size = [
                ultra::CARD_W + 2.0 * ultra::MARGIN_X,
                m.state.card_h + ultra::MARGIN_TOP + ultra::MARGIN_BOTTOM + ultra::MENU_ROOM,
            ];
            if m.state.card_h > 0.0 && (size[1] - m.size[1]).abs() > 0.5 {
                m.size = size;
                ctx.send_viewport_cmd(ViewportCommand::InnerSize(size.into()));
            }
        });
        u.win.reveal(ULTRA_TITLE, || {
            let (x, y) = win::cursor_pos();
            let mon = win::monitor_at(x, y);
            let w = initial[0] * mon.scale;
            let cx = (mon.work.left + mon.work.right) as f32 / 2.0;
            let top = mon.work.top as f32 + (mon.work.bottom - mon.work.top) as f32 * 0.15;
            ((cx - w / 2.0) as i32, top as i32)
        });
        if u.win.shown {
            let mut m = u.model.lock().unwrap();
            if !m.shown {
                m.shown = true;
                ctx.request_repaint_of(viewport_id());
            }
        }
    }
}
