//! The dictation models the registry offers, and the one a machine should get.
//!
//! A dictation model is a whisper.cpp file: the dictation engine loads it by
//! path and nothing about that engine changes here. What this module adds is
//! the registry side. The `[[dictation]]` entries of `pinned_models.toml` are the
//! dictation category: each is pinned to one file of a HuggingFace repository
//! by its SHA-256, so it downloads through the same verified path as any model,
//! and each names the card that describes it.
//!
//! Whether a file fits a machine is decided in one place, [`dictation_fits`],
//! which the catalogue, the onboarding scan and the route all read: the
//! footprint is the file plus a fixed margin ([`DICTATION_MARGIN_GB`]), held
//! against the same capacity every other "fits" verdict is held against
//! ([`MemoryArchitecture::total_capacity_gb`]). The recommendation picks the
//! largest entry that still fits beside the language model the machine runs,
//! otherwise the smallest.

use serde::{Deserialize, Serialize};

use crate::gguf_probe::GgufHeaderFacts;
use crate::hardware::HardwareProfile;

use super::fit::{estimate_memory, RuntimeShape};
use super::manifest::{ManifestError, EMBEDDED_TOML};
use super::offload::MemoryArchitecture;

/// Bytes in a gibibyte, the unit every figure here is reported in.
const BYTES_PER_GIB: f64 = 1024.0 * 1024.0 * 1024.0;

/// What the dictation engine holds beyond its weights, in gibibytes: the
/// encoder and decoder buffers and the audio window.
///
/// The memory table of the whisper.cpp README
/// (<https://github.com/ggml-org/whisper.cpp#memory-usage>) puts the gap
/// between the file and the process at 0.2 GB for the smallest model and at
/// 1.0 GB for the largest; the largest gap is the one kept, so a file is never
/// called fitting on the strength of a margin taken from a smaller model.
pub const DICTATION_MARGIN_GB: f64 = 1.0;

/// The footprint of a dictation file of `size_bytes`, in gibibytes: the file
/// plus [`DICTATION_MARGIN_GB`].
#[must_use]
pub fn dictation_footprint_gb(size_bytes: u64) -> f64 {
    size_bytes as f64 / BYTES_PER_GIB + DICTATION_MARGIN_GB
}

/// Whether a dictation file of `size_bytes` fits this machine: its footprint
/// is within the capacity the estimator gives the machine.
///
/// The one verdict for a dictation file. The catalogue entries, the
/// onboarding scan of the disk and the route all call it, so the same file is
/// never recommended on one surface and refused on another.
#[must_use]
pub fn dictation_fits(size_bytes: u64, profile: &HardwareProfile) -> bool {
    let capacity_gb = MemoryArchitecture::of(profile).total_capacity_gb(profile);
    dictation_footprint_gb(size_bytes) <= capacity_gb
}

/// One dictation model of the registry, pinned to one file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DictationModel {
    /// Stable key.
    pub id: String,
    /// What the operator reads.
    pub label: String,
    /// The organisation that trained it.
    pub publisher: String,
    /// ISO date the upstream repository was published.
    pub released: String,
    /// SPDX identifier, read on the repository carrying the file.
    pub license: String,
    /// HuggingFace repository carrying the file, `owner/name`.
    pub repo: String,
    /// The whisper.cpp file within the repository.
    pub file: String,
    /// Quantisation of that file, `F16` when it has none.
    pub quantization: String,
    /// Size of the file in bytes.
    pub size_bytes: u64,
    /// SHA-256 of the file, lowercase hex.
    pub sha256: String,
    /// Identifier of the dictation card that describes the model.
    pub card: String,
    /// URL the size and digest were read from.
    pub source: String,
    /// Free prose for whoever edits the table.
    #[serde(default)]
    pub notes: Option<String>,
}

impl DictationModel {
    /// The address the file is downloaded from.
    #[must_use]
    pub fn download_url(&self) -> String {
        format!(
            "https://huggingface.co/{}/resolve/main/{}",
            self.repo, self.file
        )
    }

    /// Its footprint, in gibibytes.
    #[must_use]
    pub fn footprint_gb(&self) -> f64 {
        dictation_footprint_gb(self.size_bytes)
    }
}

/// Why the recommended entry was chosen.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DictationReason {
    /// It is the largest entry that fits in memory at the same time as the
    /// language model.
    FitsBeside {
        /// Footprint of the language model counted, in gibibytes; zero when
        /// none was.
        language_gb: f64,
        /// Footprint of the recommended entry, in gibibytes.
        dictation_gb: f64,
        /// What the machine can hold, in gibibytes.
        capacity_gb: f64,
    },
    /// No entry fits beside the language model, so the smallest is offered.
    NoRoomBeside {
        /// Footprint of the language model counted, in gibibytes.
        language_gb: f64,
        /// Footprint of the smallest entry, in gibibytes.
        dictation_gb: f64,
        /// What the machine can hold, in gibibytes.
        capacity_gb: f64,
    },
}

/// The entry offered to this machine, and why.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DictationRecommendation {
    /// Identifier of the recommended entry.
    pub model: String,
    /// Why it was chosen.
    pub reason: DictationReason,
}

#[derive(Debug, Deserialize)]
struct RawDictation {
    #[serde(default, rename = "dictation")]
    models: Vec<DictationModel>,
}

/// The dictation category of the registry.
#[derive(Debug, Clone)]
pub struct DictationCatalogue {
    models: Vec<DictationModel>,
}

impl DictationCatalogue {
    /// Load the entries embedded in the binary.
    ///
    /// # Errors
    /// See [`ManifestError`]; a failure is a defect in a shipped file.
    pub fn embedded() -> Result<Self, ManifestError> {
        Self::from_toml(EMBEDDED_TOML)
    }

    /// Load the entries of a TOML table.
    ///
    /// # Errors
    /// [`ManifestError::Parse`] when the table does not parse,
    /// [`ManifestError::DuplicateId`] when two entries share an id, and
    /// [`ManifestError::InvalidDictation`] for an entry whose digest is not a
    /// SHA-256, whose size is zero or whose file is not a `.bin`.
    pub fn from_toml(text: &str) -> Result<Self, ManifestError> {
        let raw: RawDictation = toml::from_str(text)?;
        let mut ids = std::collections::BTreeSet::new();
        for model in &raw.models {
            if !ids.insert(model.id.as_str()) {
                return Err(ManifestError::DuplicateId(model.id.clone()));
            }
            let invalid = |reason: &str| ManifestError::InvalidDictation {
                id: model.id.clone(),
                reason: reason.to_owned(),
            };
            let digest_ok = model.sha256.len() == 64
                && model
                    .sha256
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
            if !digest_ok {
                return Err(invalid("sha256 is not 64 lowercase hexadecimal digits"));
            }
            if model.size_bytes == 0 {
                return Err(invalid("size_bytes is zero"));
            }
            if !model.file.ends_with(".bin") || model.file.contains('/') {
                return Err(invalid(
                    "file is not a whisper.cpp .bin at the repository root",
                ));
            }
        }
        Ok(Self { models: raw.models })
    }

    /// Every entry, in file order.
    #[must_use]
    pub fn models(&self) -> &[DictationModel] {
        &self.models
    }

    /// The entry carrying `id`.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<&DictationModel> {
        self.models.iter().find(|m| m.id == id)
    }

    /// The entry downloaded from `url`, the address [`DictationModel::download_url`]
    /// gives.
    #[must_use]
    pub fn by_url(&self, url: &str) -> Option<&DictationModel> {
        self.models.iter().find(|m| m.download_url() == url)
    }

    /// The entry this machine should get: the largest that fits in memory at
    /// the same time as the language model, the smallest otherwise.
    ///
    /// `language_model_bytes` is the size of the language model's weights;
    /// `None` counts no language model. Its footprint comes from
    /// [`estimate_memory`], the dictation one from [`dictation_footprint_gb`],
    /// and they are added because each model is served by its own engine.
    /// `None` only when the catalogue is empty.
    #[must_use]
    pub fn recommend(
        &self,
        profile: &HardwareProfile,
        language_model_bytes: Option<u64>,
    ) -> Option<DictationRecommendation> {
        let architecture = MemoryArchitecture::of(profile);
        let capacity_gb = architecture.total_capacity_gb(profile);
        let language_gb = language_model_bytes.map_or(0.0, |bytes| {
            estimate_memory(
                bytes,
                &GgufHeaderFacts::default(),
                &RuntimeShape::default(),
                architecture.uses_accelerator(),
            )
            .total_gb
        });

        let mut by_size: Vec<&DictationModel> = self.models.iter().collect();
        by_size.sort_by_key(|m| std::cmp::Reverse(m.size_bytes));
        let smallest = *by_size.last()?;

        let fitting = by_size
            .iter()
            .copied()
            .find(|m| language_gb + m.footprint_gb() <= capacity_gb);
        let recommendation = match fitting {
            Some(model) => DictationRecommendation {
                model: model.id.clone(),
                reason: DictationReason::FitsBeside {
                    language_gb,
                    dictation_gb: model.footprint_gb(),
                    capacity_gb,
                },
            },
            None => DictationRecommendation {
                model: smallest.id.clone(),
                reason: DictationReason::NoRoomBeside {
                    language_gb,
                    dictation_gb: smallest.footprint_gb(),
                    capacity_gb,
                },
            },
        };
        tracing::event!(
            tracing::Level::DEBUG,
            model = %recommendation.model,
            language_gb = language_gb,
            capacity_gb = capacity_gb,
            "llm.dictation.recommended"
        );
        Some(recommendation)
    }
}

#[cfg(test)]
mod tests;
