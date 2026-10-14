//! Shared app state: settings, running requests, the engines and their bench.

use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use hl_cache::history::History;
use hl_cache::stats::Stats;
use hl_cache::{Cache, Store};
use hl_core::prompt::{ContextPair, Glossary};
use hl_core::{Bench, Engine, HedgePolicy, Requests, Router, Surface};
use hl_pipeline::Pipeline;
use hl_providers::{Bing, DeepL, Google, GoogleMode, OpenRouter, Yandex};
use serde::Serialize;

use crate::settings::{self, Settings};

pub struct AppState {
    pub requests: Requests,
    pub settings: RwLock<Settings>,
    bench: Arc<Bench>,
    http: reqwest::Client,
    /// Enabled, working engines in the user's order (rebuilt on changes).
    engines: RwLock<Vec<Arc<dyn Engine>>>,
    /// The other app's window that was in front when one of ours opened:
    /// Replace pastes there.
    pub target: Mutex<hl_win::Hwnd>,
    pub pipeline: Pipeline,
    /// History and statistics; `None` if the data file can't be opened.
    pub history: Option<History>,
    pub stats: Option<Stats>,
    /// Tray «Включён» switch and «Пауза на 1 час».
    pub power: Mutex<Power>,
    /// Recent fragments per window («Переводить с контекстом»).
    context: Mutex<std::collections::HashMap<hl_core::Surface, std::collections::VecDeque<ContextPair>>>,
    pub glossary: Arc<Glossary>,
}

/// Whether shortcuts and the hook do anything right now.
#[derive(Clone, Copy, Debug, Default)]
pub struct Power {
    pub off: bool,
    pub paused_until: Option<std::time::Instant>,
}

impl Power {
    pub fn active(&self) -> bool {
        !self.off && self.paused_until.is_none_or(|t| std::time::Instant::now() >= t)
    }
}

/// What the tray menu shows.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PowerInfo {
    pub enabled: bool,
    /// Seconds left of a pause, 0 if none.
    pub paused_secs: u64,
}

/// An engine as the pickers and the Providers page show it.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineInfo {
    pub id: &'static str,
    pub name: &'static str,
    /// "cloud" or "offline".
    pub group: &'static str,
    /// Switched on in Settings.
    pub enabled: bool,
    /// Implemented in this build.
    pub implemented: bool,
    /// Works only with a key.
    pub needs_key: bool,
    /// Takes a key but works without one (Google, Yandex, Bing).
    pub key_optional: bool,
    /// OpenRouter without a chosen model.
    pub needs_model: bool,
    /// "•••• 4f2a" when a key is stored.
    pub key_hint: Option<String>,
    /// Offline models: a short note (the UI words cloud ones itself).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<&'static str>,
}

/// "offline.<model>", as a `&'static str` for [`EngineInfo`].
pub fn offline_id(m: hl_offline::Model) -> &'static str {
    match m {
        hl_offline::Model::Opus => "offline.opus",
        hl_offline::Model::Argos => "offline.argos",
        hl_offline::Model::Nllb600 => "offline.nllb600",
        hl_offline::Model::Nllb13 => "offline.nllb13",
        hl_offline::Model::Gemma4 => "offline.gemma4",
        hl_offline::Model::Gemma12 => "offline.gemma12",
        hl_offline::Model::Granite => "offline.granite",
    }
}

/// id, name, key: required (true) / optional (false). Notes under the
/// names are in the UI's strings (`engineNote`).
const CATALOG: [(&str, &str, bool); 5] = [
    ("google", "Google", false),
    ("yandex", "Yandex", false),
    ("deepl", "DeepL", true),
    ("bing", "Bing", false),
    ("openrouter", "OpenRouter", true),
];

impl AppState {
    pub fn new(settings: Settings) -> Self {
        let http = hl_providers::client();
        let store = match Store::open(&crate::settings::data_dir().join("helilingo.redb")) {
            Ok(s) => Some(s),
            Err(e) => {
                tracing::error!("data file unavailable, running without cache/history: {e}");
                None
            }
        };
        let state = Self {
            requests: Requests::new(),
            settings: RwLock::new(settings),
            bench: Arc::default(),
            http,
            engines: RwLock::default(),
            target: Mutex::default(),
            pipeline: Pipeline { cache: store.clone().map(Cache::new) },
            history: store.clone().map(History::new),
            stats: store.map(Stats::new),
            power: Mutex::default(),
            context: Mutex::default(),
            glossary: Arc::new(Glossary::new(crate::settings::data_dir().join("glossary.tsv"))),
        };
        state.rebuild_engines();
        state
    }

    /// Re-creates the engines after settings or keys change.
    pub fn rebuild_engines(&self) {
        let s = self.settings.read().unwrap();
        let http = || self.http.clone();
        let key = hl_win::secret::get;
        let mut list: Vec<Arc<dyn Engine>> = Vec::new();
        for entry in s.engines.iter().filter(|e| e.enabled) {
            let engine: Arc<dyn Engine> = match entry.id.as_str() {
                "google" => {
                    let mode = match s.google_mode {
                        settings::GoogleMode::Auto => GoogleMode::Auto,
                        settings::GoogleMode::Web => GoogleMode::Web,
                        settings::GoogleMode::Api => GoogleMode::Api,
                    };
                    let cool = settings::local_dir().join("google-cooldown.json");
                    Arc::new(Google::new(http(), key("google")).with_mode(mode).with_cool_file(cool))
                }
                "yandex" => Arc::new(Yandex::new(http(), key("yandex"), &s.yandex_folder)),
                "deepl" => Arc::new(DeepL::new(http(), key("deepl"))),
                "bing" => Arc::new(Bing::new(http(), key("bing"), &s.bing_region)),
                "openrouter" => Arc::new(
                    OpenRouter::new(http(), key("openrouter"), s.openrouter_model.clone())
                        .with_glossary(self.glossary.clone())
                        .on_usage(self.token_counter()),
                ),
                _ => continue,
            };
            list.push(engine);
        }
        // Offline models after the cloud ones: the router reaches them when
        // the cloud fails or the PC is offline.
        if s.offline.enabled {
            let cfg = s.offline.config();
            for m in s.offline.tiers.iter().filter_map(|id| hl_offline::Model::from_id(id)) {
                if m.translates() && cfg.installed(m) {
                    let e = hl_offline::engine::OfflineEngine::new(m, cfg.clone()).with_glossary(self.glossary.clone());
                    list.push(Arc::new(e));
                }
            }
        }
        *self.engines.write().unwrap() = list;
    }

    /// Every cloud engine in the user's order, then the installed offline
    /// models (when offline is on), for pickers and Settings.
    /// Counts OpenRouter tokens into today's statistics (off the runtime:
    /// it writes to disk), unless «Вести статистику» is off.
    fn token_counter(&self) -> hl_providers::openrouter::OnUsage {
        let stats = self.stats.clone();
        let keep = self.settings.read().unwrap().keep_stats;
        Arc::new(move |u| {
            let Some(st) = stats.clone().filter(|_| keep) else { return };
            tauri::async_runtime::spawn_blocking(move || {
                if let Err(e) = st.add_tokens(hl_win::local_day(), u.prompt, u.completion) {
                    tracing::warn!("stats: {e}");
                }
            });
        })
    }

    pub fn engine_list(&self) -> Vec<EngineInfo> {
        let settings = self.settings.read().unwrap();
        let mut list: Vec<EngineInfo> = settings
            .engines
            .iter()
            .filter_map(|e| CATALOG.iter().find(|c| c.0 == e.id).map(|c| (e, c)))
            .map(|(e, &(id, name, needs_key))| EngineInfo {
                id,
                name,
                group: "cloud",
                enabled: e.enabled,
                implemented: true,
                needs_key,
                key_optional: !needs_key,
                needs_model: id == "openrouter" && settings.openrouter_model.as_deref().is_none_or(|m| m.trim().is_empty()),
                key_hint: hl_win::secret::get(id).map(|k| hl_win::secret::hint(&k)),
                note: None,
            })
            .collect();
        if settings.offline.enabled {
            let cfg = settings.offline.config();
            for m in hl_offline::Model::ALL.into_iter().filter(|m| m.translates() && cfg.installed(*m)) {
                list.push(EngineInfo {
                    id: offline_id(m),
                    name: m.name(),
                    group: "offline",
                    enabled: settings.offline.tiers.iter().any(|t| t == m.id()),
                    implemented: true,
                    needs_key: false,
                    key_optional: false,
                    needs_model: false,
                    key_hint: None,
                    note: Some(if m.only_en_ru() { "en ⇄ ru" } else { "" }),
                });
            }
        }
        list
    }

    /// The chain for the next request; `prefer` goes first when it works.
    pub fn router(&self, prefer: Option<&str>) -> Router {
        let mut engines = self.engines.read().unwrap().clone();
        if let Some(p) = prefer
            && let Some(i) = engines.iter().position(|e| e.id().as_str() == p)
        {
            let e = engines.remove(i);
            engines.insert(0, e);
        }
        let s = self.settings.read().unwrap();
        let policy = HedgePolicy {
            fallback: s.fallback,
            timeout: Duration::from_secs(s.timeout_secs.into()),
            ..HedgePolicy::default()
        };
        Router { engines, policy, bench: self.bench.clone() }
    }

    /// Whether the PC has a network connection (cloud engines are skipped
    /// at once without one).
    pub fn online(&self) -> bool {
        hl_win::net::online()
    }
}

impl AppState {
    /// The fragments to send with the next request on `surface` (empty
    /// when «Переводить с контекстом» is off).
    pub fn context_for(&self, surface: Surface) -> Vec<ContextPair> {
        let n = {
            let s = self.settings.read().unwrap();
            if !s.context_enabled {
                return Vec::new();
            }
            s.context_fragments as usize
        };
        let ctx = self.context.lock().unwrap();
        ctx.get(&surface).map(|q| q.iter().rev().take(n).rev().cloned().collect()).unwrap_or_default()
    }

    /// Remembers a finished fragment. Typing in Quick translates every
    /// few letters: a text that grows or shrinks the last one replaces it.
    pub fn remember(&self, surface: Surface, source: &str, translation: &str) {
        if !self.settings.read().unwrap().context_enabled {
            return;
        }
        let mut ctx = self.context.lock().unwrap();
        let q = ctx.entry(surface).or_default();
        if q.back().is_some_and(|p| p.source.starts_with(source) || source.starts_with(&p.source) || p.source == source) {
            q.pop_back();
        }
        q.push_back(ContextPair { source: source.to_owned(), translation: translation.to_owned() });
        while q.len() > 10 {
            q.pop_front();
        }
    }

    /// Shortcuts and the hook should act.
    pub fn active(&self) -> bool {
        self.power.lock().unwrap().active()
    }

    pub fn power_info(&self) -> PowerInfo {
        let p = *self.power.lock().unwrap();
        let left = p.paused_until.map_or(0, |t| t.saturating_duration_since(std::time::Instant::now()).as_secs());
        PowerInfo { enabled: !p.off, paused_secs: left }
    }
}
