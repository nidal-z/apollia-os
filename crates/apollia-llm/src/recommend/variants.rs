//! The short list of variants a model is offered in, and the one recommended.
//!
//! A repository publishes a model at many quantisations, sometimes as split
//! files, sometimes beside a vision projector. The operator chooses among a few
//! levels of precision, not among file names: at most one variant per level
//! among Q4, Q5, Q6, Q8 and 16-bit, each with its weight on disk, its estimated
//! footprint, its verdict for this machine, whether every layer fits on the
//! accelerator, and whether it is already downloaded.
//!
//! A split GGUF is one variant whose size is the sum of its parts; a projector
//! is never a variant. An MLX repository is one variant, at the precision its
//! `config.json` declares.
//!
//! [`recommend`] then designates one variant across formats: the most faithful
//! at or below the planning ceiling of [`super::quant::PLANNING_CEILING`] whose
//! plan places every layer on the accelerator; failing that, the most faithful
//! at or below the ceiling that fits in total memory; failing that, the
//! smallest that fits. Between a GGUF and an MLX variant of the same level, MLX
//! wins on Apple Silicon and GGUF everywhere else. Nothing that does not fit is
//! recommended. It is the rule the planner already applies between model sizes
//! ([`super::quant`]), extended to the variants of one model.
//!
//! Every function here is pure, so a recorded repository tree is enough to
//! test it.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::fit::{assess_fit, RuntimeShape};
use crate::gguf_probe::GgufHeaderFacts;
use crate::hardware::{AcceleratorProfile, CompatibilityBadge, HardwareProfile};
use crate::model_index::{is_projector_name, split_part, ModelFormat, ModelIndex, ModelOrigin};

/// A level of precision, the unit the operator chooses among.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuantLevel {
    /// About four bits per weight.
    Q4,
    /// About five bits per weight.
    Q5,
    /// About six bits per weight.
    Q6,
    /// About eight bits per weight.
    Q8,
    /// Unquantised, 16-bit (`BF16` or `F16`).
    Fp16,
}

/// GGUF quantisation names, upper case, with their level, in preference order
/// within each level: the first one a repository publishes stands for the
/// level. `Q4_K_M` comes first among the four-bit formats because it is the
/// planning ceiling.
const GGUF_QUANTS: &[(&str, QuantLevel)] = &[
    ("Q4_K_M", QuantLevel::Q4),
    ("Q4_K_S", QuantLevel::Q4),
    ("IQ4_XS", QuantLevel::Q4),
    ("IQ4_NL", QuantLevel::Q4),
    ("Q4_0", QuantLevel::Q4),
    ("Q4_1", QuantLevel::Q4),
    ("Q5_K_M", QuantLevel::Q5),
    ("Q5_K_S", QuantLevel::Q5),
    ("Q5_0", QuantLevel::Q5),
    ("Q5_1", QuantLevel::Q5),
    ("Q6_K", QuantLevel::Q6),
    ("Q8_0", QuantLevel::Q8),
    ("BF16", QuantLevel::Fp16),
    ("F16", QuantLevel::Fp16),
    ("FP16", QuantLevel::Fp16),
];

/// The quantisation a GGUF file name declares, as publishers spell it in upper
/// case, when it is one of the levels offered. A part suffix
/// (`-00001-of-00002`) and a folder are ignored.
#[must_use]
pub fn gguf_quantization(file_name: &str) -> Option<&'static str> {
    gguf_quant_entry(file_name).map(|(_, (name, _))| *name)
}

/// The quantisation a GGUF file name declares, offered level or not
/// (`Q3_K_M`, `Q4_K_XL`, `F32` included), upper case: an offered name when one
/// matches, else the last token of the name when it reads as a quantisation.
#[must_use]
pub fn gguf_file_quantization(file_name: &str) -> Option<String> {
    if let Some(name) = gguf_quantization(file_name) {
        return Some(name.to_owned());
    }
    let path = Path::new(file_name);
    let base = match split_part(path) {
        Some((base, _)) => base,
        None => path.file_stem()?.to_str()?.to_owned(),
    };
    let upper = base.to_ascii_uppercase();
    let token = upper.rsplit(['-', '.']).next()?;
    let quant_like = token
        .strip_prefix("IQ")
        .or_else(|| token.strip_prefix('Q'))
        .is_some_and(|rest| {
            rest.starts_with(|c: char| c.is_ascii_digit())
                && rest.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        });
    let float = matches!(token, "F16" | "BF16" | "F32" | "FP16" | "FP32");
    (quant_like || float).then(|| token.to_owned())
}

fn gguf_quant_entry(file_name: &str) -> Option<(usize, &'static (&'static str, QuantLevel))> {
    let path = Path::new(file_name);
    let base = match split_part(path) {
        Some((base, _)) => base,
        None => path.file_stem()?.to_str()?.to_owned(),
    };
    let upper = base.to_ascii_uppercase();
    GGUF_QUANTS
        .iter()
        .enumerate()
        .filter(|(_, (name, _))| {
            upper
                .strip_suffix(name)
                .is_some_and(|head| head.ends_with(['-', '.', '_']))
        })
        .max_by_key(|(_, (name, _))| name.len())
}

/// The level and label an MLX `config.json` declares: its `quantization.bits`
/// (4, 5, 6 or 8), or 16-bit when it carries no quantisation. Another width,
/// or unreadable JSON, is no offered level.
#[must_use]
pub fn mlx_quantization(config_json: &str) -> Option<(QuantLevel, String)> {
    let value: serde_json::Value = serde_json::from_str(config_json).ok()?;
    let block = value
        .get("quantization")
        .or_else(|| value.get("quantization_config"));
    match block.and_then(|b| b.get("bits")).and_then(|b| b.as_u64()) {
        Some(4) => Some((QuantLevel::Q4, "4bit".to_owned())),
        Some(5) => Some((QuantLevel::Q5, "5bit".to_owned())),
        Some(6) => Some((QuantLevel::Q6, "6bit".to_owned())),
        Some(8) => Some((QuantLevel::Q8, "8bit".to_owned())),
        Some(_) => None,
        None if block.is_some() => None,
        None => {
            let dtype = value
                .get("torch_dtype")
                .and_then(|d| d.as_str())
                .unwrap_or("bfloat16");
            let label = if dtype.contains("float16") && !dtype.contains("bfloat") {
                "fp16"
            } else {
                "bf16"
            };
            Some((QuantLevel::Fp16, label.to_owned()))
        }
    }
}

/// One file of a repository, as its tree lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoFile {
    /// Path inside the repository.
    pub path: String,
    /// Size the tree declares, in bytes.
    pub size_bytes: u64,
    /// SHA-256 the repository publishes for the content, when it does.
    pub sha256: Option<String>,
}

/// One variant of a model, as the operator chooses it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelVariant {
    /// Repository the variant lives in.
    pub repo_id: String,
    /// Storage format.
    pub format: ModelFormat,
    /// Level of precision.
    pub level: QuantLevel,
    /// Quantisation as published: `Q4_K_M`, `BF16`, `4bit`.
    pub quantization: String,
    /// The file a download names: the first weights file of a GGUF (what the
    /// variant download takes as its selector), `config.json` for MLX.
    pub origin_file: String,
    /// Every weights file, in order: the parts of a split GGUF, the
    /// `*.safetensors` of an MLX repository.
    pub files: Vec<String>,
    /// Bytes on disk once downloaded, every file of the variant included.
    pub size_bytes: u64,
    /// Bytes of the weights alone, what the footprint estimate reads.
    pub weights_bytes: u64,
    /// SHA-256 of the first weights file, when published.
    #[serde(default)]
    pub sha256: Option<String>,
    /// Estimated footprint at the default context, in gibibytes.
    #[serde(default)]
    pub memory_gb: Option<f64>,
    /// Verdict for this machine.
    #[serde(default)]
    pub compatibility: Option<CompatibilityBadge>,
    /// Every layer reaches the accelerator at the default context; on unified
    /// memory, the whole model fits the accelerator budget.
    #[serde(default)]
    pub fully_offloaded: bool,
    /// The variant is already in the model index.
    #[serde(default)]
    pub downloaded: bool,
    /// The one variant recommended for this machine across formats.
    #[serde(default)]
    pub recommended: bool,
}

/// The GGUF variants of one repository: at most one per level, ordered from
/// four bits to sixteen, each split file counted once at the sum of its parts.
///
/// Projectors are never variants; their size is added to every variant of the
/// repository, since the variant download fetches one with each. A split file
/// with a part missing is left out.
#[must_use]
pub fn gguf_variants(repo_id: &str, files: &[RepoFile]) -> Vec<ModelVariant> {
    let weights: Vec<&RepoFile> = files
        .iter()
        .filter(|f| has_gguf_extension(&f.path))
        .collect();
    let projector_bytes = weights
        .iter()
        .filter(|f| is_projector_name(Path::new(&f.path)))
        .map(|f| f.size_bytes)
        .max()
        .unwrap_or(0);

    let mut groups: BTreeMap<(String, String), Vec<(u32, &RepoFile)>> = BTreeMap::new();
    for file in weights
        .iter()
        .copied()
        .filter(|f| !is_projector_name(Path::new(&f.path)))
    {
        let path = Path::new(&file.path);
        let folder = file
            .path
            .rsplit_once('/')
            .map_or(String::new(), |(d, _)| d.to_owned());
        let (base, index) = match split_part(path) {
            Some((base, index)) => (base, index),
            None => (file.path.clone(), 0),
        };
        groups
            .entry((folder, base))
            .or_default()
            .push((index, file));
    }

    let mut best: BTreeMap<QuantLevel, (usize, ModelVariant)> = BTreeMap::new();
    for mut parts in groups.into_values() {
        parts.sort_by_key(|(index, _)| *index);
        if !is_complete(&parts) {
            continue;
        }
        let Some((_, first)) = parts.first().copied() else {
            continue;
        };
        let Some((rank, (name, level))) = gguf_quant_entry(&first.path) else {
            continue;
        };
        let weights_bytes: u64 = parts.iter().map(|(_, f)| f.size_bytes).sum();
        let variant = ModelVariant {
            repo_id: repo_id.to_owned(),
            format: ModelFormat::Gguf,
            level: *level,
            quantization: (*name).to_owned(),
            origin_file: first.path.clone(),
            files: parts.iter().map(|(_, f)| f.path.clone()).collect(),
            size_bytes: weights_bytes + projector_bytes,
            weights_bytes,
            sha256: first.sha256.clone(),
            memory_gb: None,
            compatibility: None,
            fully_offloaded: false,
            downloaded: false,
            recommended: false,
        };
        let replace = best.get(level).is_none_or(|(held, kept)| {
            rank < *held || (rank == *held && variant.origin_file < kept.origin_file)
        });
        if replace {
            best.insert(*level, (rank, variant));
        }
    }
    best.into_values().map(|(_, v)| v).collect()
}

/// The variant an MLX repository is, from its files and the precision its
/// `config.json` declares ([`mlx_quantization`]). `None` when the repository
/// is not an MLX model (no `config.json` or no `*.safetensors` at its root) or
/// its precision is not an offered level.
#[must_use]
pub fn mlx_variant(
    repo_id: &str,
    files: &[RepoFile],
    quantization: Option<(QuantLevel, String)>,
) -> Option<ModelVariant> {
    let at_root = |f: &&RepoFile| !f.path.contains('/');
    let has_config = files
        .iter()
        .filter(at_root)
        .any(|f| f.path == "config.json");
    let mut weights: Vec<&RepoFile> = files
        .iter()
        .filter(at_root)
        .filter(|f| f.path.to_ascii_lowercase().ends_with(".safetensors"))
        .collect();
    if !has_config || weights.is_empty() {
        return None;
    }
    weights.sort_by(|a, b| a.path.cmp(&b.path));
    let (level, quantization) = quantization?;
    let size_bytes = files
        .iter()
        .filter(|f| !f.path.split('/').any(|seg| seg.starts_with('.')))
        .map(|f| f.size_bytes)
        .sum();
    Some(ModelVariant {
        repo_id: repo_id.to_owned(),
        format: ModelFormat::Mlx,
        level,
        quantization,
        origin_file: "config.json".to_owned(),
        files: weights.iter().map(|f| f.path.clone()).collect(),
        size_bytes,
        weights_bytes: weights.iter().map(|f| f.size_bytes).sum(),
        sha256: weights.first().and_then(|f| f.sha256.clone()),
        memory_gb: None,
        compatibility: None,
        fully_offloaded: false,
        downloaded: false,
        recommended: false,
    })
}

/// Size every variant for this machine through the one estimator
/// ([`assess_fit`]), at the given launch shape. No header has been read, so
/// the cache is the estimator's proportional share.
pub fn assess_variants(
    variants: &mut [ModelVariant],
    profile: &HardwareProfile,
    shape: &RuntimeShape,
) {
    for v in variants {
        let fit = assess_fit(v.weights_bytes, &GgufHeaderFacts::default(), profile, shape);
        v.memory_gb = Some(fit.estimate.total_gb);
        v.compatibility = Some(fit.badge);
        v.fully_offloaded = fit.plan.fully_offloaded;
    }
}

/// Mark the variants the model index already holds: by the repository and the
/// file the download recorded, or by the SHA-256 of the weights.
pub fn mark_downloaded(variants: &mut [ModelVariant], index: &ModelIndex) {
    for v in variants {
        v.downloaded = index.models.iter().any(|m| {
            let by_origin = match &m.origin {
                ModelOrigin::Catalog { repo, file } => {
                    *repo == v.repo_id && (*file == v.origin_file || v.files.contains(file))
                }
                ModelOrigin::Import => false,
            };
            let by_digest = v.sha256.is_some() && m.sha256 == v.sha256;
            by_origin || by_digest
        });
    }
}

/// Designate the one variant to recommend on this machine, across formats,
/// and mark it. Returns its position, or `None` when nothing fits.
///
/// Variants must have been sized by [`assess_variants`]. MLX variants are only
/// candidates on Apple Silicon, where the MLX engine runs.
pub fn recommend(variants: &mut [ModelVariant], profile: &HardwareProfile) -> Option<usize> {
    let apple = matches!(profile.accelerator, AcceleratorProfile::AppleSilicon { .. });
    let native = if apple {
        ModelFormat::Mlx
    } else {
        ModelFormat::Gguf
    };
    let candidates: Vec<usize> = variants
        .iter()
        .enumerate()
        .filter(|(_, v)| v.format == ModelFormat::Gguf || apple)
        .filter(|(_, v)| {
            matches!(
                v.compatibility,
                Some(CompatibilityBadge::Fits | CompatibilityBadge::MightFit)
            )
        })
        .map(|(i, _)| i)
        .collect();

    let under_ceiling = |i: &usize| variants[*i].level <= QuantLevel::Q4;
    let most_faithful = |pool: Vec<usize>| {
        pool.into_iter().max_by_key(|&i| {
            let v = &variants[i];
            (v.level, v.format == native, std::cmp::Reverse(v.size_bytes))
        })
    };

    let full: Vec<usize> = candidates
        .iter()
        .copied()
        .filter(under_ceiling)
        .filter(|&i| variants[i].fully_offloaded)
        .collect();
    let partial: Vec<usize> = candidates.iter().copied().filter(under_ceiling).collect();
    let chosen = most_faithful(full)
        .or_else(|| most_faithful(partial))
        .or_else(|| {
            candidates.iter().copied().min_by_key(|&i| {
                let v = &variants[i];
                (v.size_bytes, v.format != native)
            })
        });

    for (i, v) in variants.iter_mut().enumerate() {
        v.recommended = Some(i) == chosen;
    }
    chosen
}

fn is_complete(parts: &[(u32, &RepoFile)]) -> bool {
    match parts {
        [(0, _)] => true,
        _ => {
            let expected = parts
                .first()
                .and_then(|(_, f)| split_total(&f.path))
                .unwrap_or(0) as usize;
            parts.len() == expected
                && parts
                    .iter()
                    .enumerate()
                    .all(|(i, (index, _))| *index as usize == i + 1)
        }
    }
}

fn split_total(path: &str) -> Option<u32> {
    let stem = Path::new(path).file_stem()?.to_str()?;
    stem.rsplit_once("-of-")?.1.parse().ok()
}

fn has_gguf_extension(path: &str) -> bool {
    path.to_ascii_lowercase().ends_with(".gguf")
}

#[cfg(test)]
mod tests;
