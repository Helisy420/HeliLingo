//! Tiers 1–4 in-process through CTranslate2 (`ct2rs`, Ruy CPU backend).
//!
//! - OPUS-MT (Marian): `source.spm` pieces + `</s>`, decoded with `target.spm`.
//! - Argos: `sentencepiece.model` pieces, `model/` is the CTranslate2 model;
//!   pairs without a package pivot through English.
//! - NLLB-200: `[src_Lang] + pieces + </s>`, target prefix `[tgt_Lang]`.
//!
//! Text is split into lines and sentences (as Ultra mode does) and the
//! pieces are translated as one batch, so long selections stay under the
//! models' 512-token input limit and keep their line breaks.
//!
//! This runs inside a worker process (see `worker`). Loaded models stay in
//! memory for the worker's life: ct2rs deliberately leaks a translator on
//! drop on Windows (joining its threads deadlocks), so unloading a model
//! means ending its worker.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex};

use ct2rs::sys::{ComputeType, Config, Device, TranslationOptions, Translator};
use sentencepiece::SentencePieceProcessor;

use super::catalog::tier_dir;
use super::{MODEL_FAILED, OfflineConfig, Tier, UNSUPPORTED_PAIR};
use crate::settings::{OfflineDevice, OfflinePrecision};
use crate::translate::Error;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Marian,
    Argos,
    Nllb,
}

struct Model {
    translator: Translator,
    src: SentencePieceProcessor,
    /// Marian has its own target vocabulary; the others share one.
    tgt: Option<SentencePieceProcessor>,
    kind: Kind,
}

/// What a loaded model was built with; a change means reloading.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Key {
    cuda: bool,
    compute: i32,
    threads: usize,
}

type Loaded = HashMap<PathBuf, (Key, Arc<Model>)>;

static MODELS: LazyLock<Mutex<Loaded>> = LazyLock::new(Default::default);

fn ct2_error<E: std::fmt::Display>(e: E) -> Error {
    eprintln!("ctranslate2: {e}");
    Error::Other(MODEL_FAILED.into())
}

fn compute_code(c: ComputeType) -> i32 {
    match c {
        ComputeType::INT8 => 1,
        ComputeType::INT8_FLOAT16 => 2,
        ComputeType::FLOAT32 => 3,
        ComputeType::FLOAT16 => 4,
        _ => 0,
    }
}

/// CUDA only in builds with the `offline-cuda` feature (CTranslate2 must
/// be compiled against the CUDA toolkit for that).
fn use_cuda(cfg: &OfflineConfig) -> bool {
    cfg!(feature = "offline-cuda")
        && cfg.device != OfflineDevice::Cpu
        && super::sys::gpu().is_some_and(|g| g.vendor == super::sys::NVIDIA)
        && ct2rs::sys::get_device_count(Device::CUDA) > 0
}

fn compute_type(cfg: &OfflineConfig, cuda: bool) -> ComputeType {
    match (cfg.precision, cuda) {
        (OfflinePrecision::Auto, true) | (OfflinePrecision::Int8Float16, true) => ComputeType::INT8_FLOAT16,
        (OfflinePrecision::Float32, _) => ComputeType::FLOAT32,
        // int8_float16 has no CPU kernels: int8 is the closest.
        _ => ComputeType::INT8,
    }
}

fn load(dir: &Path, kind: Kind, cfg: &OfflineConfig) -> Result<Arc<Model>, Error> {
    let cuda = use_cuda(cfg);
    let compute = compute_type(cfg, cuda);
    let key = Key { cuda, compute: compute_code(compute), threads: cfg.thread_count() };
    if let Some((k, m)) = MODELS.lock().unwrap().get(dir)
        && *k == key
    {
        return Ok(m.clone());
    }
    let (model_dir, src, tgt) = match kind {
        Kind::Marian => (dir.to_path_buf(), dir.join("source.spm"), Some(dir.join("target.spm"))),
        Kind::Argos => (dir.join("model"), dir.join("sentencepiece.model"), None),
        Kind::Nllb => (dir.to_path_buf(), dir.join("sentencepiece.bpe.model"), None),
    };
    let config = Config {
        device: if cuda { Device::CUDA } else { Device::CPU },
        compute_type: compute,
        num_threads_per_replica: key.threads,
        ..Default::default()
    };
    let translator = Translator::new(&model_dir, &config).map_err(ct2_error)?;
    let src = SentencePieceProcessor::open(&src).map_err(ct2_error)?;
    let tgt = match tgt {
        Some(p) => Some(SentencePieceProcessor::open(&p).map_err(ct2_error)?),
        None => None,
    };
    let model = Arc::new(Model { translator, src, tgt, kind });
    MODELS.lock().unwrap().insert(dir.to_path_buf(), (key, model.clone()));
    Ok(model)
}

impl Model {
    fn encode(&self, text: &str, src: &str) -> Result<Vec<String>, Error> {
        let pieces = self.src.encode(text).map_err(ct2_error)?;
        let mut tokens: Vec<String> = Vec::with_capacity(pieces.len() + 2);
        if self.kind == Kind::Nllb {
            tokens.push(nllb_code(src).ok_or_else(unsupported)?.to_owned());
        }
        tokens.extend(pieces.into_iter().map(|p| p.piece));
        if self.kind != Kind::Argos {
            tokens.push("</s>".into());
        }
        Ok(tokens)
    }

    fn decode(&self, mut tokens: Vec<String>, tgt: &str) -> Result<String, Error> {
        if self.kind == Kind::Nllb && tokens.first().map(String::as_str) == nllb_code(tgt) {
            tokens.remove(0);
        }
        let spm = self.tgt.as_ref().unwrap_or(&self.src);
        spm.decode_pieces(&tokens).map(|t| tidy(&t, tgt)).map_err(ct2_error)
    }

    /// Translates sentences as one batch.
    fn run(&self, segments: &[String], src: &str, tgt: &str) -> Result<Vec<String>, Error> {
        let source: Vec<Vec<String>> = segments.iter().map(|s| self.encode(s, src)).collect::<Result<_, _>>()?;
        let options = TranslationOptions {
            beam_size: 2,
            max_input_length: 510,
            max_decoding_length: 512,
            repetition_penalty: 1.1,
            ..Default::default()
        };
        let results = if self.kind == Kind::Nllb {
            let code = nllb_code(tgt).ok_or_else(unsupported)?;
            let prefix: Vec<Vec<&str>> = segments.iter().map(|_| vec![code]).collect();
            self.translator
                .translate_batch_with_target_prefix(&source, &prefix, &options, None)
                .map_err(ct2_error)?
        } else {
            self.translator.translate_batch(&source, &options, None).map_err(ct2_error)?
        };
        results
            .into_iter()
            .map(|r| self.decode(r.hypotheses.into_iter().next().unwrap_or_default(), tgt))
            .collect()
    }
}

fn unsupported() -> Error {
    Error::Other(UNSUPPORTED_PAIR.into())
}

/// The models sometimes produce tokenized spacing ("Hey , how 's it
/// going ?"): no space before closing punctuation or after an opening
/// bracket, and English contractions joined. French keeps its space
/// before `! ? : ;`.
fn tidy(text: &str, tgt: &str) -> String {
    let lang = super::base_code(tgt);
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    for (i, &c) in chars.iter().enumerate() {
        if c == ' ' {
            let next = chars.get(i + 1).copied();
            let closing = matches!(next, Some(',' | '.' | ')' | ']' | '}' | '…'))
                || (lang != "fr" && matches!(next, Some('!' | '?' | ':' | ';')));
            let after_open = matches!(out.chars().last(), Some('(' | '[' | '{'));
            if closing || after_open {
                continue;
            }
        }
        out.push(c);
    }
    if lang == "en" {
        for (spaced, joined) in [(" n't", "n't"), (" 's", "'s"), (" 're", "'re"), (" 've", "'ve"), (" 'll", "'ll"), (" 'm", "'m"), (" 'd", "'d")] {
            out = out.replace(spaced, joined);
        }
    }
    out
}

/// Splits into sentences, translates them in one batch, and rebuilds the
/// text with the original line breaks.
fn translate_text(model: &Model, text: &str, src: &str, tgt: &str) -> Result<String, Error> {
    let (lead, mut fragments) = crate::ultra::split(text);
    if fragments.is_empty() {
        return Ok(text.to_owned());
    }
    let segments: Vec<String> = fragments.iter().map(|f| f.text.clone()).collect();
    let out = model.run(&segments, src, tgt)?;
    for (f, t) in fragments.iter_mut().zip(out) {
        f.state = crate::ultra::FragmentState::Translated(t);
    }
    Ok(crate::ultra::join(&lead, &fragments))
}

/// The model folder for a pair, if the tier has one.
fn pair_dir(models: &Path, tier: Tier, src: &str, tgt: &str) -> Option<(PathBuf, Kind)> {
    let dir = tier_dir(models, tier);
    let (path, kind) = match tier {
        Tier::SuperMegaFast => (dir.join(format!("{src}-{tgt}")), Kind::Marian),
        Tier::SuperFast => (dir.join(format!("{src}_{tgt}")), Kind::Argos),
        Tier::Fast | Tier::Normal => (dir, Kind::Nllb),
        _ => return None,
    };
    let probe = match kind {
        Kind::Argos => path.join("model").join("model.bin"),
        _ => path.join("model.bin"),
    };
    probe.is_file().then_some((path, kind))
}

pub fn translate(cfg: &OfflineConfig, tier: Tier, text: &str, src: &str, tgt: &str) -> Result<String, Error> {
    let models = &cfg.model_dir;
    if let Some((dir, kind)) = pair_dir(models, tier, src, tgt) {
        if kind == Kind::Nllb && (nllb_code(src).is_none() || nllb_code(tgt).is_none()) {
            return Err(unsupported());
        }
        let model = load(&dir, kind, cfg)?;
        return translate_text(&model, text, src, tgt);
    }
    // Argos: pivot through English when both halves are installed.
    if tier == Tier::SuperFast && src != "en" && tgt != "en" {
        let (a, b) = (pair_dir(models, tier, src, "en"), pair_dir(models, tier, "en", tgt));
        if let (Some((d1, k1)), Some((d2, k2))) = (a, b) {
            let english = translate_text(&*load(&d1, k1, cfg)?, text, src, "en")?;
            return translate_text(&*load(&d2, k2, cfg)?, &english, "en", tgt);
        }
    }
    Err(unsupported())
}

/// Loads the tier's default pair (en → ru, or the NLLB model).
pub fn preload(cfg: &OfflineConfig, tier: Tier) -> Result<(), Error> {
    let (dir, kind) = pair_dir(&cfg.model_dir, tier, "en", "ru").ok_or_else(unsupported)?;
    load(&dir, kind, cfg).map(|_| ())
}

/// FLORES-200 codes for the languages in `settings::LANGUAGES`.
pub fn nllb_code(code: &str) -> Option<&'static str> {
    Some(match super::base_code(code) {
        "en" => "eng_Latn",
        "ru" => "rus_Cyrl",
        "uk" => "ukr_Cyrl",
        "de" => "deu_Latn",
        "fr" => "fra_Latn",
        "es" => "spa_Latn",
        "it" => "ita_Latn",
        "pt" => "por_Latn",
        "nl" => "nld_Latn",
        "pl" => "pol_Latn",
        "cs" => "ces_Latn",
        "sk" => "slk_Latn",
        "sv" => "swe_Latn",
        "da" => "dan_Latn",
        "no" | "nb" => "nob_Latn",
        "fi" => "fin_Latn",
        "et" => "est_Latn",
        "lv" => "lvs_Latn",
        "lt" => "lit_Latn",
        "hu" => "hun_Latn",
        "ro" => "ron_Latn",
        "bg" => "bul_Cyrl",
        "el" => "ell_Grek",
        "tr" => "tur_Latn",
        "be" => "bel_Cyrl",
        "kk" => "kaz_Cyrl",
        "ka" => "kat_Geor",
        "hy" => "hye_Armn",
        "he" => "heb_Hebr",
        "ar" => "arb_Arab",
        "fa" => "pes_Arab",
        "hi" => "hin_Deva",
        "th" => "tha_Thai",
        "vi" => "vie_Latn",
        "id" => "ind_Latn",
        "ja" => "jpn_Jpan",
        "ko" => "kor_Hang",
        "zh" => "zho_Hans",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_app_language_has_an_nllb_code() {
        for (code, _, _) in crate::settings::LANGUAGES {
            assert!(nllb_code(code).is_some(), "{code}");
        }
        assert_eq!(nllb_code("zh-CN"), Some("zho_Hans"));
        assert_eq!(nllb_code("xx"), None);
    }

    #[test]
    fn tidies_tokenized_spacing() {
        assert_eq!(tidy("Hey , how 's it going ?", "en"), "Hey, how's it going?");
        assert_eq!(tidy("I do n't know ( yet ) .", "en"), "I don't know (yet).");
        assert_eq!(tidy("Привет , как дела ?", "ru"), "Привет, как дела?");
        assert_eq!(tidy("Bonjour , ça va ?", "fr"), "Bonjour, ça va ?");
        assert_eq!(tidy("Line one.\nLine two", "en"), "Line one.\nLine two");
    }

    #[test]
    fn precision_choice() {
        let mut cfg = OfflineConfig::default();
        assert_eq!(compute_type(&cfg, false), ComputeType::INT8);
        assert_eq!(compute_type(&cfg, true), ComputeType::INT8_FLOAT16);
        cfg.precision = OfflinePrecision::Int8Float16;
        assert_eq!(compute_type(&cfg, false), ComputeType::INT8);
        cfg.precision = OfflinePrecision::Float32;
        assert_eq!(compute_type(&cfg, true), ComputeType::FLOAT32);
    }

    #[test]
    fn missing_pairs_are_unsupported() {
        let cfg = OfflineConfig { model_dir: std::env::temp_dir().join("hl-no-models"), ..Default::default() };
        assert!(matches!(translate(&cfg, Tier::SuperFast, "Hallo", "de", "fr"), Err(Error::Other(m)) if m == UNSUPPORTED_PAIR));
    }
}
