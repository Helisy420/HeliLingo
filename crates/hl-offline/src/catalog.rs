//! What each model downloads, and how the model folder is laid out.
//!
//! ```text
//! <models>/opus/en-ru, opus/ru-en       OPUS-MT pairs (CTranslate2 int8)
//! <models>/argos/en_ru, argos/ru_en     Argos packages (unzipped)
//! <models>/nllb600, nllb13              NLLB-200 (CTranslate2 int8)
//! <models>/gemma4, gemma12, granite     GGUF (+ Granite's vision projector)
//! <models>/runtime/llama                llama-server, shared
//! <models>/<model>/installed.json       marker: the model is complete
//! ```
//!
//! Hugging Face files are pinned to a commit; large files carry the SHA-256
//! Hugging Face publishes (checked after download). Every pin was checked
//! against the Hugging Face API on 2026-10-14. Argos packages have no
//! published checksum (size + zip integrity only); llama.cpp release assets
//! come with GitHub's SHA-256 digest.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{Model, Runtime};

/// One file from a Hugging Face repo.
#[derive(Clone, Copy, Debug)]
pub struct HfFile {
    pub repo: &'static str,
    /// Commit the file is pinned to.
    pub rev: &'static str,
    pub path: &'static str,
    pub size: u64,
    pub sha256: Option<&'static str>,
}

impl HfFile {
    pub fn url(&self) -> String {
        format!("https://huggingface.co/{}/resolve/{}/{}", self.repo, self.rev, self.path)
    }
}

/// A download unit inside a model's folder.
#[derive(Clone, Copy, Debug)]
pub enum Component {
    /// Files into `<model>/<sub>` (`sub` may be empty).
    Hf { sub: &'static str, files: &'static [HfFile] },
    /// An Argos package (`translate-<from>_<to>`) unzipped into `<model>/<from>_<to>`.
    Argos { from: &'static str, to: &'static str, approx: u64 },
}

impl Component {
    pub fn approx_size(&self) -> u64 {
        match self {
            Component::Hf { files, .. } => files.iter().map(|f| f.size).sum(),
            Component::Argos { approx, .. } => *approx,
        }
    }
}

const fn hf(repo: &'static str, rev: &'static str, path: &'static str, size: u64, sha256: Option<&'static str>) -> HfFile {
    HfFile { repo, rev, path, size, sha256 }
}

const OPUS_EN_RU: &str = "manancode/opus-mt-en-ru-ctranslate2-android";
const OPUS_EN_RU_REV: &str = "8693481face72247976d2a7da8610ed4c75d9e83";
const OPUS_RU_EN: &str = "manancode/opus-mt-ru-en-ctranslate2-android";
const OPUS_RU_EN_REV: &str = "9f66440658d009510d9e670ae813c7452fd38234";
const SPM_EN: &str = "16bebef1389a0b8ab452772c4e35b9e605e5713f8ac7baa71ca701394eaa086d";
const SPM_RU: &str = "745998e51ba5b058e38b7ac7765c25c43ed5c1c39cc92b27163b9b2e323c9d7c";

/// Helsinki-NLP/opus-mt-en-ru, CTranslate2 int8 (Apache-2.0 / CC-BY-4.0).
const OPUS_EN_RU_FILES: &[HfFile] = &[
    hf(OPUS_EN_RU, OPUS_EN_RU_REV, "config.json", 223, None),
    hf(OPUS_EN_RU, OPUS_EN_RU_REV, "model.bin", 78_276_475, Some("f5ae1da30bcb61e877c549be8fb5a434193893eb97509ccd0b954465f9a8f80c")),
    hf(OPUS_EN_RU, OPUS_EN_RU_REV, "shared_vocabulary.json", 2_300_269, None),
    hf(OPUS_EN_RU, OPUS_EN_RU_REV, "source.spm", 802_781, Some(SPM_EN)),
    hf(OPUS_EN_RU, OPUS_EN_RU_REV, "target.spm", 1_080_169, Some(SPM_RU)),
];

const OPUS_RU_EN_FILES: &[HfFile] = &[
    hf(OPUS_RU_EN, OPUS_RU_EN_REV, "config.json", 223, None),
    hf(OPUS_RU_EN, OPUS_RU_EN_REV, "model.bin", 78_276_475, Some("0efde44155f1d73b07d81bae640d9cca79657c6ade8e338d0862e34c30ffeee9")),
    hf(OPUS_RU_EN, OPUS_RU_EN_REV, "shared_vocabulary.json", 2_300_269, None),
    hf(OPUS_RU_EN, OPUS_RU_EN_REV, "source.spm", 1_080_169, Some(SPM_RU)),
    hf(OPUS_RU_EN, OPUS_RU_EN_REV, "target.spm", 802_781, Some(SPM_EN)),
];

const NLLB600: &str = "JustFrederik/nllb-200-distilled-600M-ct2-int8";
const NLLB600_REV: &str = "302d78f00e6fdb50a1064059df7c392b735e9d05";
const NLLB_SPM: HfFile = hf(
    NLLB600,
    NLLB600_REV,
    "sentencepiece.bpe.model",
    4_852_054,
    Some("14bb8dfb35c0ffdea7bc01e56cea38b9e3d5efcdcb9c251d6b40538e1aab555a"),
);

/// facebook/nllb-200-distilled-600M, CTranslate2 int8 (CC-BY-NC-4.0).
const NLLB600_FILES: &[HfFile] = &[
    hf(NLLB600, NLLB600_REV, "config.json", 159, None),
    hf(NLLB600, NLLB600_REV, "model.bin", 622_595_991, Some("ed1beaf75134de7505315a5223162f56acff397eff6b50638a500d3936fe707b")),
    NLLB_SPM,
    hf(NLLB600, NLLB600_REV, "shared_vocabulary.txt", 2_568_098, None),
];

const NLLB13: &str = "OpenNMT/nllb-200-distilled-1.3B-ct2-int8";
const NLLB13_REV: &str = "70f572adafa4794890ce7826156a4209717855af";

/// facebook/nllb-200-distilled-1.3B, CTranslate2 int8 (CC-BY-NC-4.0). The
/// repo has no SentencePiece model; NLLB's is the same for every size.
const NLLB13_FILES: &[HfFile] = &[
    hf(NLLB13, NLLB13_REV, "config.json", 1_071, None),
    hf(NLLB13, NLLB13_REV, "model.bin", 1_377_755_738, Some("89dc4b9eb7f4dcd3ff36023c9b1c55fcecb47ddc0df97ab31b068ef9cf194940")),
    hf(NLLB13, NLLB13_REV, "shared_vocabulary.json", 5_921_176, None),
    NLLB_SPM,
];

/// google/translategemma-4b-it, GGUF Q4_K_M by mradermacher (Gemma terms).
pub const GEMMA4_FILE: HfFile = hf(
    "mradermacher/translategemma-4b-it-GGUF",
    "35a7486e128b19642cdc72d7b91b21ba388aaf42",
    "translategemma-4b-it.Q4_K_M.gguf",
    2_489_909_760,
    Some("81200d03e843d2ec1ece6eeafe7d13cb6e5211e1fcd336ade55790b683a08330"),
);

/// google/translategemma-12b-it, GGUF Q4_K_M by mradermacher (Gemma terms).
pub const GEMMA12_FILE: HfFile = hf(
    "mradermacher/translategemma-12b-it-GGUF",
    "fdf84c9f6fe14e69d58814f14e7b5b63bb6a1b28",
    "translategemma-12b-it.Q4_K_M.gguf",
    7_300_794_112,
    Some("b7aac4b4be7ab0c49b6556c29c4467e74313df7f1e95d9f9676bb2adf0afa528"),
);

const GRANITE: &str = "ibm-granite/granite-vision-4.1-4b-GGUF";
const GRANITE_REV: &str = "b1fa14294b0f5cac04c43076d1c4574091abf117";

/// ibm-granite/granite-vision-4.1-4b, GGUF Q4_K_M by IBM (Apache-2.0).
/// Needs llama.cpp b9534 or newer.
pub const GRANITE_FILE: HfFile = hf(
    GRANITE,
    GRANITE_REV,
    "granite-vision-4.1-4b-Q4_K_M.gguf",
    2_099_510_240,
    Some("973a58c4cfc2b3f58dedc515a89f73c372fe0910da515ecefcebe966131f769f"),
);

/// Granite's vision encoder + projector.
pub const GRANITE_MMPROJ: HfFile = hf(
    GRANITE,
    GRANITE_REV,
    "mmproj-model-f16.gguf",
    1_162_347_936,
    Some("573dd2579f6043649299f0b2225000a5691d92f320aabe909fb4c6e75450cad2"),
);

const GEMMA4_FILES: &[HfFile] = &[GEMMA4_FILE];
const GEMMA12_FILES: &[HfFile] = &[GEMMA12_FILE];
const GRANITE_FILES: &[HfFile] = &[GRANITE_FILE, GRANITE_MMPROJ];

/// The download units of a model (without the shared llama runtime).
pub fn components(m: Model) -> &'static [Component] {
    match m {
        Model::Opus => &[
            Component::Hf { sub: "en-ru", files: OPUS_EN_RU_FILES },
            Component::Hf { sub: "ru-en", files: OPUS_RU_EN_FILES },
        ],
        Model::Argos => &[
            Component::Argos { from: "en", to: "ru", approx: 195_746_693 },
            Component::Argos { from: "ru", to: "en", approx: 156_239_112 },
        ],
        Model::Nllb600 => &[Component::Hf { sub: "", files: NLLB600_FILES }],
        Model::Nllb13 => &[Component::Hf { sub: "", files: NLLB13_FILES }],
        Model::Gemma4 => &[Component::Hf { sub: "", files: GEMMA4_FILES }],
        Model::Gemma12 => &[Component::Hf { sub: "", files: GEMMA12_FILES }],
        Model::Granite => &[Component::Hf { sub: "", files: GRANITE_FILES }],
    }
}

/// The GGUF of a llama model.
pub fn gguf(m: Model) -> Option<HfFile> {
    match m {
        Model::Gemma4 => Some(GEMMA4_FILE),
        Model::Gemma12 => Some(GEMMA12_FILE),
        Model::Granite => Some(GRANITE_FILE),
        _ => None,
    }
}

/// The vision projector, for models that read images.
pub fn mmproj(m: Model) -> Option<HfFile> {
    (m == Model::Granite).then_some(GRANITE_MMPROJ)
}

/// Where the Argos package index lives.
pub const ARGOS_INDEX: &str = "https://raw.githubusercontent.com/argosopentech/argospm-index/main/index.json";

/// When the index can't be read (the 1.9 packages, checked 2026-10-14).
pub fn argos_fallback_url(from: &str, to: &str) -> String {
    format!("https://argos-net.com/v1/translate-{from}_{to}-1_9.argosmodel")
}

/// GitHub releases of llama.cpp (prebuilt `llama-server`).
pub const LLAMA_RELEASES: &str = "https://api.github.com/repos/ggml-org/llama.cpp/releases?per_page=10";
/// Oldest llama.cpp build that runs Granite Vision 4.1.
pub const LLAMA_MIN_BUILD: u32 = 9534;

/// Which llama.cpp build to use.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LlamaBackend {
    Cuda13,
    Cuda12,
    Vulkan,
    Cpu,
}

impl LlamaBackend {
    /// The best build for this PC: CUDA for NVIDIA (13.x when the driver
    /// supports it, else 12.4), Vulkan for other cards, else CPU.
    pub fn detect() -> LlamaBackend {
        match crate::sys::gpu() {
            Some(g) if g.vendor == crate::sys::NVIDIA && g.cuda.is_some_and(|v| v >= (13, 0)) => LlamaBackend::Cuda13,
            Some(g) if g.vendor == crate::sys::NVIDIA && g.cuda.is_some_and(|v| v >= (12, 4)) => LlamaBackend::Cuda12,
            Some(_) if crate::sys::has_vulkan() => LlamaBackend::Vulkan,
            _ => LlamaBackend::Cpu,
        }
    }

    /// Release asset patterns: (prefix, contains) for the binaries and the
    /// CUDA runtime zip when one is needed.
    pub fn assets(self) -> &'static [(&'static str, &'static str)] {
        match self {
            LlamaBackend::Cuda13 => &[("llama-", "-bin-win-cuda-13."), ("cudart-llama-bin-win-cuda-13.", "")],
            LlamaBackend::Cuda12 => &[("llama-", "-bin-win-cuda-12."), ("cudart-llama-bin-win-cuda-12.", "")],
            LlamaBackend::Vulkan => &[("llama-", "-bin-win-vulkan-x64.zip")],
            LlamaBackend::Cpu => &[("llama-", "-bin-win-cpu-x64.zip")],
        }
    }

    /// Download size before the release is read (b11617, 2026-10-14).
    pub fn approx_size(self) -> u64 {
        match self {
            LlamaBackend::Cuda13 => 577_000_000,
            LlamaBackend::Cuda12 => 657_000_000,
            LlamaBackend::Vulkan => 32_000_000,
            LlamaBackend::Cpu => 20_000_000,
        }
    }

    pub fn uses_gpu(self) -> bool {
        self != LlamaBackend::Cpu
    }

    pub fn label(self) -> &'static str {
        match self {
            LlamaBackend::Cuda13 => "CUDA 13",
            LlamaBackend::Cuda12 => "CUDA 12",
            LlamaBackend::Vulkan => "Vulkan",
            LlamaBackend::Cpu => "CPU",
        }
    }
}

/// Does `name` match one asset pattern (`.zip`, x64)?
pub fn asset_matches(name: &str, (prefix, contains): (&str, &str)) -> bool {
    name.starts_with(prefix) && name.contains(contains) && name.ends_with("x64.zip")
}

/// "b11617" → 11617.
pub fn build_number(tag: &str) -> Option<u32> {
    tag.trim_start_matches('b').parse().ok()
}

pub const MARKER: &str = "installed.json";

pub fn model_dir(models: &Path, m: Model) -> PathBuf {
    models.join(m.id())
}

pub fn runtime_dir(models: &Path) -> PathBuf {
    models.join("runtime").join("llama")
}

pub fn runtime_installed(models: &Path) -> bool {
    let dir = runtime_dir(models);
    dir.join(MARKER).is_file() && dir.join("llama-server.exe").is_file()
}

pub fn is_installed(models: &Path, m: Model) -> bool {
    model_dir(models, m).join(MARKER).is_file() && (m.runtime() == Runtime::Ct2 || runtime_installed(models))
}

/// Download size of a model not yet on disk (with the llama runtime when
/// it's missing).
pub fn download_size(models: &Path, m: Model) -> u64 {
    let own: u64 = components(m).iter().map(Component::approx_size).sum();
    let runtime = if m.runtime() == Runtime::Llama && !runtime_installed(models) {
        LlamaBackend::detect().approx_size()
    } else {
        0
    };
    own + runtime
}

/// What the runtime marker records.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RuntimeInfo {
    pub backend: LlamaBackend,
    pub release: String,
    pub assets: Vec<String>,
}

pub fn runtime_info(models: &Path) -> Option<RuntimeInfo> {
    let json = std::fs::read_to_string(runtime_dir(models).join(MARKER)).ok()?;
    serde_json::from_str(&json).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_model_has_pinned_files() {
        for m in Model::ALL {
            assert!(!components(m).is_empty());
            for c in components(m) {
                assert!(c.approx_size() > 0);
                if let Component::Hf { files, .. } = c {
                    for f in *files {
                        assert_eq!(f.rev.len(), 40, "{}", f.path);
                        assert!(f.url().starts_with("https://huggingface.co/"));
                        if f.size > 10_000_000 {
                            assert!(f.sha256.is_some_and(|s| s.len() == 64), "{} needs a checksum", f.path);
                        }
                    }
                }
            }
        }
        assert!(gguf(Model::Gemma4).is_some() && gguf(Model::Nllb600).is_none());
        assert!(mmproj(Model::Granite).is_some() && mmproj(Model::Gemma4).is_none());
    }

    #[test]
    fn llama_assets() {
        let cuda = LlamaBackend::Cuda12.assets();
        assert!(asset_matches("llama-b11617-bin-win-cuda-12.4-x64.zip", cuda[0]));
        assert!(asset_matches("cudart-llama-bin-win-cuda-12.4-x64.zip", cuda[1]));
        assert!(!asset_matches("llama-b11617-bin-win-cuda-13.4-arm64.zip", LlamaBackend::Cuda13.assets()[0]));
        assert!(!asset_matches("cudart-llama-bin-win-cuda-12.4-x64.zip", cuda[0]));
        assert!(asset_matches("llama-b11617-bin-win-vulkan-x64.zip", LlamaBackend::Vulkan.assets()[0]));
        assert!(asset_matches("llama-b11617-bin-win-cpu-x64.zip", LlamaBackend::Cpu.assets()[0]));
        assert!(!asset_matches("llama-b11617-bin-win-openvino-2026.4.1-x64.zip", LlamaBackend::Cpu.assets()[0]));
        assert_eq!(build_number("b11617"), Some(11617));
    }
}
