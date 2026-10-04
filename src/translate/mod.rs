//! Translation engine: a chain of providers in the user's order, raced so a
//! slow one never holds the answer up (see [`hedge`]), response parsing and
//! an in-memory cache. Nothing is written to disk
//! ("Selected text is sent for translation and not stored").
//!
//! API keys never appear in error messages: errors carry fixed English
//! sentences (translated in i18n's ENGINE table), and Google's key travels
//! in a header so no URL holds one.

mod bing;
mod deepl;
mod google;
pub mod hedge;
mod lang;
mod yandex;

use std::collections::VecDeque;
use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex, Weak};
use std::time::{Duration, Instant};

use lru::LruCache;
use reqwest::StatusCode;
use reqwest::blocking::{Client, RequestBuilder};
use serde::de::DeserializeOwned;

use crate::offline::{ContextPair, OfflineConfig};
use crate::settings::{AlreadyInTarget, GoogleMode, ProviderEntry, ProviderKeys, ProviderKind, Settings};

/// Longest selection we send in one request.
pub const MAX_CHARS: usize = 5000;

// User-visible error texts (`Error::Other`). Russian is in i18n's ENGINE
// table; the first two are in POPUP.
pub const KEY_REJECTED: &str = "The API key was rejected";
pub const EMPTY: &str = "Empty response";
pub const NO_KEY: &str = "No API key";
pub const FOLDER_REQUIRED: &str = "Folder ID required";
pub const NOT_INSTALLED: &str = "Not installed";
pub const QUOTA: &str = "Quota exceeded";
pub const REJECTED: &str = "The service rejected the request";
pub const SERVICE_ERROR: &str = "The service returned an error";
pub const UNEXPECTED: &str = "Unexpected response";
pub const NETWORK: &str = "Network error";
pub const NO_PROVIDER: &str = "No translation service is available";

/// Racing the chain: the next online provider starts when the previous
/// one hasn't answered within this time (or at once when it failed).
const ONLINE_HEDGE: Duration = Duration::from_millis(700);
/// An offline tier joins later: loading a model costs processor time and
/// memory, and online answers usually arrive well before this.
const OFFLINE_HEDGE: Duration = Duration::from_millis(2500);

#[derive(Clone, Debug)]
pub struct DictGroup {
    /// Part of speech, e.g. "adverb".
    pub pos: String,
    pub terms: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct Translation {
    pub source: String,
    pub text: String,
    /// Detected (or given) source language code.
    pub src_lang: Option<String>,
    pub tgt_lang: String,
    /// A single word or short dictionary phrase (vs. a sentence/paragraph).
    pub is_word: bool,
    pub dict: Vec<DictGroup>,
    /// The provider that produced this result.
    pub provider: ProviderKind,
}

impl Translation {
    pub fn pos(&self) -> Option<&str> {
        self.dict.first().map(|g| g.pos.as_str()).filter(|p| !p.is_empty())
    }

    /// Other common translations of the word, excluding the main one.
    pub fn alternatives(&self, max: usize) -> Vec<String> {
        let main = self.text.to_lowercase();
        let mut out: Vec<String> = Vec::new();
        for term in self.dict.iter().flat_map(|g| g.terms.iter()) {
            let lower = term.to_lowercase();
            if lower != main && !out.iter().any(|t| t.to_lowercase() == lower) {
                out.push(term.clone());
            }
            if out.len() == max {
                break;
            }
        }
        out
    }
}

#[derive(Clone, Debug)]
pub enum Error {
    /// No network / service unreachable.
    Offline,
    /// Rate limited or temporarily unavailable.
    Busy,
    /// One of the English sentences above.
    Other(String),
}

type Key = (String, String, String);

/// The part of [`Settings`] the engine works from.
#[derive(Clone, Debug)]
pub struct EngineConfig {
    /// Providers in the order they are tried.
    pub providers: Vec<ProviderEntry>,
    pub keys: ProviderKeys,
    pub already_in_target: AlreadyInTarget,
    /// Offline tiers: model folder, device, "offline only", context.
    pub offline: OfflineConfig,
    pub google_mode: GoogleMode,
}

impl From<&Settings> for EngineConfig {
    fn from(s: &Settings) -> Self {
        Self {
            providers: s.providers.clone(),
            keys: s.keys.clone(),
            already_in_target: s.already_in_target,
            offline: s.into(),
            google_mode: s.google_mode,
        }
    }
}

impl EngineConfig {
    /// Can `kind` run now? Online providers need their keys (and are off
    /// in "offline only" mode); offline tiers need to be installed.
    fn usable(&self, kind: ProviderKind) -> bool {
        match kind.tier() {
            Some(tier) => self.offline.installed(tier),
            None => !self.offline.offline_only && self.keys.usable(kind),
        }
    }

    /// Providers to try, in order: `prefer` first (when usable), then the
    /// enabled usable ones.
    fn chain(&self, prefer: Option<ProviderKind>) -> Vec<ProviderKind> {
        let usable = |k: ProviderKind| self.usable(k);
        let mut order: Vec<ProviderKind> = prefer.filter(|&k| usable(k)).into_iter().collect();
        for p in &self.providers {
            if p.enabled && usable(p.kind) && !order.contains(&p.kind) {
                order.push(p.kind);
            }
        }
        order
    }
}

/// Tries `order` until one provider succeeds. Any failure moves on to the
/// next one. When all fail, the first error other than `Offline` wins: it
/// comes from a provider that was reachable, so it says more (a rejected
/// key, rate limiting) than "can't connect". All `Offline` → `Offline`.
#[cfg(test)]
fn run_chain(
    order: &[ProviderKind],
    mut call: impl FnMut(ProviderKind) -> Result<Translation, Error>,
) -> Result<Translation, Error> {
    let mut first: Option<Error> = None;
    for &kind in order {
        match call(kind) {
            Ok(t) => return Ok(t),
            Err(e) => {
                if first.as_ref().is_none_or(|f| matches!(f, Error::Offline) && !matches!(e, Error::Offline)) {
                    first = Some(e);
                }
            }
        }
    }
    Err(first.unwrap_or_else(|| Error::Other(NO_PROVIDER.into())))
}

/// The error to report when every provider failed: the first one that
/// isn't `Offline` (see [`run_chain`]).
fn chain_error(errors: Vec<Option<Error>>) -> Error {
    let mut first: Option<Error> = None;
    for e in errors.into_iter().flatten() {
        if first.as_ref().is_none_or(|f| matches!(f, Error::Offline) && !matches!(e, Error::Offline)) {
            first = Some(e);
        }
    }
    first.unwrap_or_else(|| Error::Other(NO_PROVIDER.into()))
}

/// [`run_chain`], raced: providers start in order, each next one after
/// [`ONLINE_HEDGE`] ([`OFFLINE_HEDGE`] for an offline tier) or as soon as
/// the running ones failed; the first answer wins.
fn run_hedged(
    order: &[ProviderKind],
    call: impl Fn(ProviderKind) -> Result<Translation, Error> + Send + Sync + 'static,
) -> Result<Translation, Error> {
    let order: Vec<ProviderKind> = order.to_vec();
    let delays: Vec<Duration> = order
        .iter()
        .map(|k| if k.is_offline() { OFFLINE_HEDGE } else { ONLINE_HEDGE })
        .collect();
    let kinds = order.clone();
    hedge::race(order.len(), |i| delays[i], Arc::new(move |i| {
        crate::timing(&format!("engine: start {:?}", kinds[i]));
        call(kinds[i])
    }))
    .map_err(chain_error)
}

pub struct Engine {
    /// This engine's own `Arc`, for racing requests on other threads.
    me: Weak<Engine>,
    http: Client,
    cache: Mutex<LruCache<Key, Arc<Translation>>>,
    config: Mutex<EngineConfig>,
    last_network: Mutex<Option<Instant>>,
    /// Keyless Microsoft Translator token (or why fetching it failed) and
    /// when it was fetched.
    bing_token: Mutex<Option<(Result<String, Error>, Instant)>>,
    /// The last translated fragments of this session (target language,
    /// pair), newest last: context for the context tiers and DeepL.
    context: Mutex<VecDeque<(String, ContextPair)>>,
    /// Keyless Google endpoints that answered "busy", and until when to
    /// leave them alone (see `google::Endpoint`).
    google_backoff: Mutex<google::Backoff>,
    /// Keyless Yandex answered "busy": leave it alone until then.
    yandex_backoff: Mutex<yandex::Cooldown>,
    /// Keyless Yandex client id (random, per session).
    yandex_client_id: String,
}

/// Fragments the context ring keeps (the slider goes up to 10).
const CONTEXT_KEEP: usize = 10;

impl Engine {
    pub fn new(config: EngineConfig) -> Arc<Self> {
        let http = Client::builder()
            .connect_timeout(Duration::from_secs(3))
            .timeout(Duration::from_secs(8))
            .tcp_nodelay(true)
            .tcp_keepalive(Duration::from_secs(30))
            .pool_idle_timeout(Duration::from_secs(300))
            .user_agent(concat!("HeliLingo/", env!("CARGO_PKG_VERSION")))
            .build()
            .expect("HTTP client");
        Arc::new_cyclic(|me| Self {
            me: me.clone(),
            http,
            cache: Mutex::new(LruCache::new(NonZeroUsize::new(512).unwrap())),
            config: Mutex::new(config),
            last_network: Mutex::new(None),
            bing_token: Mutex::new(None),
            context: Mutex::new(VecDeque::new()),
            google_backoff: Mutex::new(google::Backoff::load()),
            yandex_backoff: Mutex::new(yandex::Cooldown::default()),
            yandex_client_id: yandex::new_client_id(),
        })
    }

    /// New provider order or keys from Settings. Cached results stay valid:
    /// the cache key doesn't depend on the provider.
    ///
    /// Offline tiers switched off are unloaded (their memory freed); a new
    /// device, thread count, precision or folder unloads them all (they
    /// load again, with the new setup, when next used).
    pub fn configure(&self, config: EngineConfig) {
        let old = std::mem::replace(&mut *self.config.lock().unwrap(), config.clone());
        let (a, b) = (&old.offline, &config.offline);
        if (a.device, a.threads, a.precision, &a.model_dir) != (b.device, b.threads, b.precision, &b.model_dir) {
            crate::offline::unload_all();
            return;
        }
        let enabled = |c: &EngineConfig, kind: ProviderKind| c.providers.iter().any(|p| p.kind == kind && p.enabled);
        for tier in crate::offline::Tier::ALL {
            if enabled(&old, tier.kind()) && !enabled(&config, tier.kind()) {
                crate::offline::unload(tier);
            }
        }
    }

    /// Runs a short test translation with exactly `kind` and the given keys
    /// (Settings → Providers → Check), not the configured ones. Bypasses
    /// the cache and the chain.
    /// Offline tiers translate "Hello" to Russian (every tier has en → ru).
    pub fn check(&self, kind: ProviderKind, keys: &ProviderKeys) -> Result<(), Error> {
        if kind == ProviderKind::Bing {
            self.reset_edge_token();
        }
        let offline = self.config.lock().unwrap().offline.clone();
        let to = if kind.is_offline() { "ru" } else { "de" };
        self.call(kind, keys, &offline, "Hello", "en", to, &[]).map(|_| ())
    }

    /// Opens (or refreshes) the pooled TLS connection to the first usable
    /// provider in the background, so the real request doesn't pay for DNS
    /// and the TLS handshake. Called the moment the shortcut fires, in
    /// parallel with reading the selection.
    pub fn warm_up(self: &Arc<Self>) {
        let idle = self
            .last_network
            .lock()
            .unwrap()
            .is_none_or(|t| t.elapsed() > Duration::from_secs(45));
        if !idle {
            return;
        }
        *self.last_network.lock().unwrap() = Some(Instant::now());
        let config = self.config.lock().unwrap().clone();
        let Some(&kind) = config.chain(None).first() else { return };
        // An offline tier first in line is about to be used: load it now.
        // Fallback tiers load only when they are actually used.
        if let Some(tier) = kind.tier() {
            crate::offline::preload(&config.offline, tier);
            return;
        }
        let this = self.clone();
        std::thread::spawn(move || {
            let keys = &config.keys;
            let url = match kind {
                ProviderKind::Google if keys.google.is_set() => google::CLOUD_URL,
                // The host only: warming the connection must not count as a
                // translation request against Google's rate limit.
                ProviderKind::Google => google::WARM_URL,
                ProviderKind::Yandex if keys.yandex.is_set() => yandex::URL,
                ProviderKind::Yandex => yandex::WARM_URL,
                ProviderKind::Bing => {
                    // The keyless token is a request of its own: fetch it now.
                    if !keys.azure.is_set() && this.edge_token(false).is_err() {
                        return;
                    }
                    bing::URL
                }
                ProviderKind::DeepL => deepl::base_url(&keys.deepl_key().unwrap_or_default()),
                _ => return,
            };
            let _ = this.http.head(url).send();
        });
    }

    /// Providers for the compact popup's other translations: the chain
    /// without `exclude`, online ones first, at most `max`.
    pub fn variant_providers(&self, exclude: ProviderKind, max: usize) -> Vec<ProviderKind> {
        let config = self.config.lock().unwrap().clone();
        let mut kinds: Vec<ProviderKind> = config.chain(None).into_iter().filter(|k| *k != exclude).collect();
        kinds.sort_by_key(|k| k.is_offline());
        kinds.truncate(max);
        kinds
    }

    /// One provider's translation, outside the chain and the cache (the
    /// compact popup's other translations).
    pub fn translate_by(&self, kind: ProviderKind, text: &str, from: &str, to: &str) -> Result<Translation, Error> {
        let config = self.config.lock().unwrap().clone();
        let mut t = self.call(kind, &config.keys, &config.offline, text, from, to, &[])?;
        t.is_word = is_word_like(text, !t.dict.is_empty());
        if t.is_word {
            t.text = match_case(text, t.text.trim());
        }
        Ok(t)
    }

    pub fn cached(&self, text: &str, from: &str, to: &str) -> Option<Arc<Translation>> {
        let key = (text.to_owned(), from.to_owned(), to.to_owned());
        self.cache.lock().unwrap().get(&key).cloned()
    }

    pub fn translate(&self, text: &str, from: &str, to: &str) -> Result<Arc<Translation>, Error> {
        self.translate_using(text, from, to, None)
    }

    /// Like [`Engine::translate`], but `prefer` (when set and usable) is
    /// tried before the configured order: the provider chip in the Quick
    /// and main windows. Results still share the cache; a cached result
    /// from another provider is replaced by `prefer`'s.
    pub fn translate_using(
        &self,
        text: &str,
        from: &str,
        to: &str,
        prefer: Option<ProviderKind>,
    ) -> Result<Arc<Translation>, Error> {
        let config = self.config.lock().unwrap().clone();
        let context = Arc::new(self.context_for(&config.offline, to));
        let this = self.me.upgrade().expect("the engine lives in an Arc");
        let (keys, offline, source) = (Arc::new(config.keys.clone()), Arc::new(config.offline.clone()), Arc::new(text.to_owned()));
        let t = self.translate_with(text, from, to, prefer, &config, |order, from, to| {
            let (this, keys, offline, context, source) = (this.clone(), keys.clone(), offline.clone(), context.clone(), source.clone());
            let (from, to) = (from.to_owned(), to.to_owned());
            run_hedged(order, move |kind| this.call(kind, &keys, &offline, &source, &from, &to, &context))
        })?;
        if config.offline.use_context && !t.is_word {
            self.remember(&t);
        }
        Ok(t)
    }

    /// Previous fragments translated into `to`, oldest first (empty when
    /// context is off).
    fn context_for(&self, offline: &OfflineConfig, to: &str) -> Vec<ContextPair> {
        if !offline.use_context {
            return Vec::new();
        }
        let ring = self.context.lock().unwrap();
        let mut v: Vec<ContextPair> = ring
            .iter()
            .rev()
            .filter(|(lang, _)| same_lang(lang, to))
            .take(offline.context_fragments as usize)
            .map(|(_, p)| p.clone())
            .collect();
        v.reverse();
        v
    }

    /// Adds a translation to the context ring (skipping repeats).
    fn remember(&self, t: &Translation) {
        let pair = ContextPair { source: t.source.clone(), translation: t.text.clone() };
        let mut ring = self.context.lock().unwrap();
        if ring.back().is_some_and(|(_, p)| p.source == pair.source) {
            return;
        }
        ring.push_back((t.tgt_lang.clone(), pair));
        while ring.len() > CONTEXT_KEEP {
            ring.pop_front();
        }
    }

    /// Cache, chain, "already in the target language" and post-processing
    /// around `run(order, from, to)`, which gets one translation from the
    /// providers in `order`.
    fn translate_with(
        &self,
        text: &str,
        from: &str,
        to: &str,
        prefer: Option<ProviderKind>,
        config: &EngineConfig,
        run: impl Fn(&[ProviderKind], &str, &str) -> Result<Translation, Error>,
    ) -> Result<Arc<Translation>, Error> {
        let order = config.chain(prefer);
        if let Some(hit) = self.cached(text, from, to)
            && (prefer.is_none() || order.first() == Some(&hit.provider))
        {
            return Ok(hit);
        }
        let mut t = run(&order, from, to)?;
        // Selected text already in the target language: translate it the
        // other way instead of echoing it back.
        if config.already_in_target == AlreadyInTarget::TranslateBack
            && from == "auto"
            && t.src_lang.as_deref().is_some_and(|s| same_lang(s, to))
        {
            let other = if same_lang(to, "en") { "ru" } else { "en" };
            t = run(&order, from, other)?;
        }
        t.is_word = is_word_like(text, !t.dict.is_empty());
        if t.is_word {
            t.text = match_case(text, t.text.trim());
        }
        let t = Arc::new(t);
        self.cache
            .lock()
            .unwrap()
            .put((text.to_owned(), from.to_owned(), to.to_owned()), t.clone());
        Ok(t)
    }

    /// One request to one provider with `keys` (or one offline tier).
    /// `context`: previous fragments, for the context tiers and DeepL.
    #[allow(clippy::too_many_arguments)]
    fn call(
        &self,
        kind: ProviderKind,
        keys: &ProviderKeys,
        offline: &OfflineConfig,
        text: &str,
        from: &str,
        to: &str,
        context: &[ContextPair],
    ) -> Result<Translation, Error> {
        if let Some(tier) = kind.tier() {
            let out = crate::offline::translate(offline, tier, text, from, to, context)?;
            if out.text.trim().is_empty() {
                return Err(empty());
            }
            return Ok(Translation {
                source: text.to_owned(),
                text: out.text,
                src_lang: Some(out.src_lang),
                tgt_lang: to.to_owned(),
                is_word: false,
                dict: Vec::new(),
                provider: kind,
            });
        }
        *self.last_network.lock().unwrap() = Some(Instant::now());
        crate::timing(&format!("engine: call {kind:?}"));
        let t = match kind {
            ProviderKind::Google if keys.google.is_set() => self.google_cloud(keys.google.expose(), text, from, to),
            ProviderKind::Google => {
                let mode = self.config.lock().unwrap().google_mode;
                self.google_keyless(mode, text, from, to)
            }
            ProviderKind::Yandex if keys.yandex.is_set() => {
                self.yandex(keys.yandex.expose(), &keys.yandex_folder, text, from, to)
            }
            ProviderKind::Bing => self.bing(keys, text, from, to),
            ProviderKind::DeepL => match keys.deepl_key() {
                Some(key) => self.deepl(&key, text, from, to, crate::offline::prompts::deepl_context(context).as_deref()),
                None => Err(Error::Other(NO_KEY.into())),
            },
            ProviderKind::Yandex => self.yandex_keyless(text, from, to),
            // Offline tiers returned above.
            _ => Err(Error::Other(NOT_INSTALLED.into())),
        }?;
        if t.text.trim().is_empty() {
            return Err(empty());
        }
        Ok(t)
    }
}

/// Sends a request; returns the status and body. Errors are fixed texts:
/// reqwest's own messages include the URL.
fn send(req: RequestBuilder) -> Result<(StatusCode, String), Error> {
    let resp = req.send().map_err(network_error)?;
    let status = resp.status();
    let body = resp.text().map_err(network_error)?;
    Ok((status, body))
}

fn network_error(e: reqwest::Error) -> Error {
    crate::timing(&format!("net error: {e:?}"));
    if e.is_connect() || e.is_timeout() || e.is_request() {
        Error::Offline
    } else {
        Error::Other(NETWORK.into())
    }
}

fn parse<T: DeserializeOwned>(body: &str) -> Result<T, Error> {
    serde_json::from_str(body).map_err(|_| Error::Other(UNEXPECTED.into()))
}

fn empty() -> Error {
    Error::Other(EMPTY.into())
}

/// A failed status as an [`Error`]. `keyed`: the request carried an API
/// key, so 401/403 means the key; keyless endpoints answer 403 when they
/// throttle.
fn status_error(status: StatusCode, keyed: bool) -> Error {
    match status.as_u16() {
        401 | 403 if keyed => Error::Other(KEY_REJECTED.into()),
        401 | 403 | 429 | 502..=504 => Error::Busy,
        456 => Error::Other(QUOTA.into()),
        400 | 404 | 413 | 414 | 422 => Error::Other(REJECTED.into()),
        _ => Error::Other(SERVICE_ERROR.into()),
    }
}

fn same_lang(a: &str, b: &str) -> bool {
    let base = |s: &str| s.split('-').next().unwrap_or(s).to_ascii_lowercase();
    base(a) == base(b)
}

fn is_word_like(text: &str, has_dict: bool) -> bool {
    let words = text.split_whitespace().count();
    words == 1 || (has_dict && words <= 3)
}

/// "effectively" → "эффективно", not "Эффективно".
fn match_case(source: &str, translated: &str) -> String {
    let src_lower = source.chars().next().is_some_and(char::is_lowercase);
    let mut chars = translated.chars();
    match chars.next() {
        Some(first) if src_lower && first.is_uppercase() => {
            first.to_lowercase().chain(chars).collect()
        }
        _ => translated.to_owned(),
    }
}

/// Trims the selection and, for single words, stray punctuation
/// ("effectively," → "effectively").
pub fn normalize(selection: &str) -> String {
    let t = selection.trim();
    let t: String = t.chars().take(MAX_CHARS).collect();
    if t.split_whitespace().count() == 1 {
        t.trim_matches(|c: char| !c.is_alphanumeric()).to_owned()
    } else {
        t
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Secret;

    fn config(kinds: &[ProviderKind]) -> EngineConfig {
        EngineConfig {
            providers: kinds.iter().map(|&kind| ProviderEntry { kind, enabled: true }).collect(),
            // Bing needs an Azure key to be usable.
            keys: ProviderKeys { azure: Secret::new("test"), ..Default::default() },
            already_in_target: AlreadyInTarget::TranslateBack,
            // No models: offline tiers are not installed.
            offline: OfflineConfig {
                model_dir: std::env::temp_dir().join("hl-engine-tests-no-models"),
                ..Default::default()
            },
            google_mode: GoogleMode::Api,
        }
    }

    fn fake(kind: ProviderKind, text: &str, src: &str, to: &str) -> Translation {
        Translation {
            source: String::new(),
            text: text.into(),
            src_lang: Some(src.into()),
            tgt_lang: to.into(),
            is_word: false,
            dict: Vec::new(),
            provider: kind,
        }
    }

    use ProviderKind::*;

    #[test]
    fn chain_skips_unusable_and_disabled() {
        let mut c = config(&[Argos, DeepL, Bing, Google]);
        // No DeepL key; Argos isn't installed.
        assert_eq!(c.chain(None), [Bing, Google]);
        c.keys.deepl = Secret::new("k");
        c.providers[2].enabled = false;
        assert_eq!(c.chain(None), [DeepL, Google]);
        // Yandex and Google need no key.
        assert_eq!(config(&[Yandex, Google]).chain(None), [Yandex, Google]);
        // A preferred provider goes first, even when disabled in the list.
        assert_eq!(c.chain(Some(Bing))[0], Bing);
        assert_eq!(c.chain(Some(Argos))[0], c.chain(None)[0]);
    }

    #[test]
    fn installed_tiers_join_the_chain_and_offline_only_keeps_them_alone() {
        let dir = std::env::temp_dir().join(format!("hl-engine-chain-{}", std::process::id()));
        let opus = crate::offline::catalog::tier_dir(&dir, crate::offline::Tier::SuperMegaFast);
        std::fs::create_dir_all(&opus).unwrap();
        std::fs::write(opus.join(crate::offline::catalog::MARKER), "{}").unwrap();
        let mut c = config(&[Google, OpusMt, Nllb600, Bing]);
        c.offline.model_dir = dir.clone();
        assert_eq!(c.chain(None), [Google, OpusMt, Bing]);
        c.offline.offline_only = true;
        assert_eq!(c.chain(None), [OpusMt]);
        assert_eq!(c.chain(Some(Google)), [OpusMt]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn context_ring_keeps_recent_fragments_per_target() {
        let c = config(&[Google]);
        let engine = Engine::new(c.clone());
        let mut t = fake(Google, "Раз.", "en", "ru");
        t.source = "One.".into();
        engine.remember(&t);
        engine.remember(&t);
        let mut de = fake(Google, "Zwei.", "en", "de");
        de.source = "Two.".into();
        engine.remember(&de);
        let mut off = c.offline.clone();
        assert_eq!(engine.context_for(&off, "ru"), [ContextPair { source: "One.".into(), translation: "Раз.".into() }]);
        for i in 0..20 {
            let mut t = fake(Google, &format!("{i}"), "en", "ru");
            t.source = format!("s{i}");
            engine.remember(&t);
        }
        off.context_fragments = 3;
        let ctx = engine.context_for(&off, "ru");
        assert_eq!(ctx.iter().map(|p| p.source.as_str()).collect::<Vec<_>>(), ["s17", "s18", "s19"]);
        off.use_context = false;
        assert!(engine.context_for(&off, "ru").is_empty());
    }

    #[test]
    fn chain_falls_back_in_order() {
        let mut tried = Vec::new();
        let r = run_chain(&[Yandex, Bing, Google], |k| {
            tried.push(k);
            match k {
                Yandex => Err(Error::Other(KEY_REJECTED.into())),
                Bing => Err(Error::Busy),
                _ => Ok(fake(k, "ok", "en", "ru")),
            }
        });
        assert_eq!(tried, [Yandex, Bing, Google]);
        assert_eq!(r.unwrap().provider, Google);
    }

    #[test]
    fn chain_reports_first_reachable_error() {
        let r = run_chain(&[Google, DeepL, Bing], |k| match k {
            Google => Err(Error::Offline),
            DeepL => Err(Error::Other(KEY_REJECTED.into())),
            _ => Err(Error::Busy),
        });
        assert!(matches!(r, Err(Error::Other(m)) if m == KEY_REJECTED));
        let r = run_chain(&[Google, Bing], |_| Err(Error::Offline));
        assert!(matches!(r, Err(Error::Offline)));
        let r = run_chain(&[], |k| Ok(fake(k, "x", "en", "ru")));
        assert!(matches!(r, Err(Error::Other(m)) if m == NO_PROVIDER));
    }

    #[test]
    fn cache_key_ignores_provider() {
        let c = config(&[Bing, Google]);
        let engine = Engine::new(c.clone());
        let calls = std::cell::Cell::new(0);
        let call = |k, _: &str, to: &str| {
            calls.set(calls.get() + 1);
            Ok(fake(k, "Привет", "en", to))
        };
        let t = engine.translate_with("Hello", "auto", "ru", None, &c, |o, f, t| run_chain(o, |k| call(k, f, t))).unwrap();
        assert_eq!(t.provider, Bing);
        // Another order, same key: served from the cache.
        let c2 = config(&[Google, Bing]);
        let t = engine.translate_with("Hello", "auto", "ru", None, &c2, |o, f, t| run_chain(o, |k| call(k, f, t))).unwrap();
        assert_eq!(t.provider, Bing);
        assert_eq!(engine.cached("Hello", "auto", "ru").unwrap().provider, Bing);
        // Preferring the provider that made it: still cached.
        engine.translate_with("Hello", "auto", "ru", Some(Bing), &c, |o, f, t| run_chain(o, |k| call(k, f, t))).unwrap();
        assert_eq!(calls.get(), 1);
        // Preferring another one replaces it under the same key.
        let t = engine.translate_with("Hello", "auto", "ru", Some(Google), &c, |o, f, t| run_chain(o, |k| call(k, f, t))).unwrap();
        assert_eq!((t.provider, calls.get()), (Google, 2));
        assert_eq!(engine.cached("Hello", "auto", "ru").unwrap().provider, Google);
    }

    #[test]
    fn already_in_target_translates_back() {
        let mut c = config(&[Google]);
        let engine = Engine::new(c.clone());
        let call = |k, _: &str, to: &str| Ok(fake(k, if to == "en" { "Hello" } else { "Привет" }, "ru", to));
        let t = engine.translate_with("привет", "auto", "ru", None, &c, |o, f, t| run_chain(o, |k| call(k, f, t))).unwrap();
        assert_eq!((t.text.as_str(), t.tgt_lang.as_str()), ("hello", "en"));

        c.already_in_target = AlreadyInTarget::Keep;
        let engine = Engine::new(c.clone());
        let t = engine.translate_with("привет", "auto", "ru", None, &c, |o, f, t| run_chain(o, |k| call(k, f, t))).unwrap();
        assert_eq!((t.text.as_str(), t.tgt_lang.as_str()), ("привет", "ru"));
    }

    #[test]
    fn word_post_processing() {
        let c = config(&[Bing]);
        let engine = Engine::new(c.clone());
        let call = |k, _: &str, to: &str| Ok(fake(k, " Эффективно ", "en", to));
        let t = engine.translate_with("effectively", "auto", "ru", None, &c, |o, f, t| run_chain(o, |k| call(k, f, t))).unwrap();
        assert!(t.is_word);
        assert_eq!(t.text, "эффективно");
        let t = engine.translate_with("It works well", "auto", "ru", None, &c, |o, f, t| run_chain(o, |k| call(k, f, t))).unwrap();
        assert!(!t.is_word);
    }

    #[test]
    fn statuses() {
        assert!(matches!(status_error(StatusCode::FORBIDDEN, true), Error::Other(m) if m == KEY_REJECTED));
        assert!(matches!(status_error(StatusCode::FORBIDDEN, false), Error::Busy));
        assert!(matches!(status_error(StatusCode::TOO_MANY_REQUESTS, true), Error::Busy));
        assert!(matches!(status_error(StatusCode::from_u16(456).unwrap(), true), Error::Other(m) if m == QUOTA));
    }

    #[test]
    fn messages_have_russian() {
        let all = [
            KEY_REJECTED, EMPTY, NO_KEY, FOLDER_REQUIRED, NOT_INSTALLED, QUOTA, REJECTED,
            SERVICE_ERROR, UNEXPECTED, NETWORK, NO_PROVIDER,
        ];
        // Russian is the default interface language.
        for m in all {
            assert_ne!(crate::i18n::tr(m), m, "no Russian for {m}");
        }
    }

    #[test]
    fn check_without_key() {
        crate::i18n::set_lang("ru");
        let engine = Engine::new(config(&[]));
        let keys = ProviderKeys::default();
        assert!(matches!(engine.check(DeepL, &keys), Err(Error::Other(m)) if m == NO_KEY));
        assert!(matches!(engine.check(Argos, &keys), Err(Error::Other(m)) if m == NOT_INSTALLED));
    }
}
