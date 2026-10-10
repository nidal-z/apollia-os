//! The embeddings models the registry offers, and the one a machine should get.
//!
//! An embeddings model turns a text into a vector, which is what search by
//! meaning compares. It is never a chat engine, and the gate that refuses it as
//! one stays: [`super::verdict::assess`] rejects an embeddings header, whether
//! its architecture says so (`bert`) or only its pooling key does (Qwen3
//! Embedding names the same `qwen3` architecture as the chat models).
//!
//! What this module adds is the other side of that gate. The `[[embedding]]`
//! entries of `families.toml` are the embeddings category of the registry: each
//! is pinned to one GGUF file by its SHA-256, so it downloads through the same
//! verified path as any model (its `repo` and `file` are what the downloader is
//! given, and the digest the Hub publishes for that file is the pinned one), and
//! a copy imported from a disk is recognised by its digest.
//! A header is admitted into the category by its pooling key ([`admits`]).
//!
//! The recommendation picks the largest entry that still fits beside the
//! language model the machine runs, through the same estimator every other
//! "fits" verdict goes through ([`super::fit::estimate_memory`]); otherwise the
//! smallest. An entry marked `by_request` is listed and never recommended: it
//! is there for whoever picks it, and changing what a first launch downloads is
//! a decision, not a side effect of adding a file to the table.

use serde::{Deserialize, Serialize};

use crate::gguf_probe::GgufHeaderFacts;
use crate::hardware::HardwareProfile;

use super::fit::{estimate_memory, MemoryEstimate, RuntimeShape};
use super::manifest::{ManifestError, EMBEDDED_TOML};
use super::offload::MemoryArchitecture;

/// How the engine reduces a text to one vector, the `--pooling` argument.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Pooling {
    /// The average of every token's vector.
    Mean,
    /// The vector of the first token.
    Cls,
    /// The vector of the last token.
    Last,
}

impl Pooling {
    /// The `{arch}.pooling_type` value a GGUF header carries for this pooling.
    #[must_use]
    pub fn header_value(self) -> u64 {
        match self {
            Self::Mean => 1,
            Self::Cls => 2,
            Self::Last => 3,
        }
    }

    /// The pooling a `{arch}.pooling_type` value names, when it is one an
    /// embeddings model uses. `0` (none) and `4` (rank, a reranker) are not.
    #[must_use]
    pub fn from_header_value(value: u64) -> Option<Self> {
        match value {
            1 => Some(Self::Mean),
            2 => Some(Self::Cls),
            3 => Some(Self::Last),
            _ => None,
        }
    }
}

/// One embeddings model of the registry, pinned to one GGUF file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EmbeddingModel {
    /// Stable key.
    pub id: String,
    /// What the operator reads.
    pub label: String,
    /// The organisation that trained it.
    pub publisher: String,
    /// ISO date of its first public release.
    pub released: String,
    /// SPDX identifier.
    pub license: String,
    /// HuggingFace repository carrying the GGUF, `owner/name`.
    pub repo: String,
    /// The GGUF file within the repository.
    pub file: String,
    /// Quantisation of that file, for example `Q8_0`.
    pub quantization: String,
    /// Size of the file in bytes.
    pub size_bytes: u64,
    /// SHA-256 of the file, lowercase hex.
    pub sha256: String,
    /// `general.architecture` of its header.
    pub architecture: String,
    /// Length of the vector it produces.
    pub dimensions: u32,
    /// Tokens it reads at most per text.
    pub context_length: u32,
    /// How the engine reduces a text to one vector.
    pub pooling: Pooling,
    /// Text put before a search query.
    pub query_prefix: String,
    /// Text put before a passage to index.
    pub passage_prefix: String,
    /// URL the facts were read from.
    pub source: String,
    /// Listed for whoever picks it, and left out of the recommendation.
    #[serde(default)]
    pub by_request: bool,
    /// Free prose for whoever edits the table.
    #[serde(default)]
    pub notes: Option<String>,
}

impl EmbeddingModel {
    /// What it costs to run beside a chat model, through the one estimator.
    ///
    /// No header is read, so the cache takes the estimator's proportional
    /// fallback, which errs high.
    #[must_use]
    pub fn estimate(&self, accelerated: bool) -> MemoryEstimate {
        let shape = RuntimeShape {
            n_ctx: self.context_length,
            ..RuntimeShape::default()
        };
        estimate_memory(
            self.size_bytes,
            &GgufHeaderFacts::default(),
            &shape,
            accelerated,
        )
    }
}

/// Whether a GGUF header belongs to the embeddings category: its pooling key
/// names a pooling an embeddings model uses.
#[must_use]
pub fn admits(facts: &GgufHeaderFacts) -> bool {
    facts
        .pooling_type
        .and_then(Pooling::from_header_value)
        .is_some()
}

/// Why the recommended entry was chosen.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EmbeddingReason {
    /// It fits in memory at the same time as the language model.
    FitsBeside {
        /// Footprint of the language model counted, in gibibytes; zero when
        /// none was.
        language_gb: f64,
        /// Footprint of the recommended entry, in gibibytes.
        embeddings_gb: f64,
        /// What the machine can hold, in gibibytes.
        capacity_gb: f64,
    },
    /// The largest entry would not fit beside the language model, so a smaller
    /// one is offered: the largest of those that fit, the smallest when none
    /// does.
    NoRoomBeside {
        /// Footprint of the language model counted, in gibibytes.
        language_gb: f64,
        /// The larger entry that did not fit.
        larger: String,
        /// Its footprint, in gibibytes.
        larger_gb: f64,
        /// What the machine can hold, in gibibytes.
        capacity_gb: f64,
    },
}

/// The entry offered to this machine, and why.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EmbeddingRecommendation {
    /// Identifier of the recommended entry.
    pub model: String,
    /// Why it was chosen.
    pub reason: EmbeddingReason,
}

#[derive(Debug, Deserialize)]
struct RawEmbeddings {
    #[serde(default, rename = "embedding")]
    embeddings: Vec<EmbeddingModel>,
}

/// The embeddings category of the registry.
#[derive(Debug, Clone)]
pub struct EmbeddingCatalogue {
    models: Vec<EmbeddingModel>,
}

impl EmbeddingCatalogue {
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
    /// [`ManifestError::InvalidEmbedding`] for an entry whose digest is not a
    /// SHA-256 or whose size is zero.
    pub fn from_toml(text: &str) -> Result<Self, ManifestError> {
        let raw: RawEmbeddings = toml::from_str(text)?;
        let mut ids = std::collections::BTreeSet::new();
        for model in &raw.embeddings {
            if !ids.insert(model.id.as_str()) {
                return Err(ManifestError::DuplicateId(model.id.clone()));
            }
            let digest_ok = model.sha256.len() == 64
                && model
                    .sha256
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
            if !digest_ok {
                return Err(ManifestError::InvalidEmbedding {
                    id: model.id.clone(),
                    reason: "sha256 is not 64 lowercase hexadecimal digits".to_owned(),
                });
            }
            if model.size_bytes == 0 {
                return Err(ManifestError::InvalidEmbedding {
                    id: model.id.clone(),
                    reason: "size_bytes is zero".to_owned(),
                });
            }
        }
        Ok(Self {
            models: raw.embeddings,
        })
    }

    /// Every entry, in file order.
    #[must_use]
    pub fn models(&self) -> &[EmbeddingModel] {
        &self.models
    }

    /// The entry carrying `id`.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<&EmbeddingModel> {
        self.models.iter().find(|m| m.id == id)
    }

    /// The entry whose pinned file has this SHA-256, in any case.
    #[must_use]
    pub fn by_sha256(&self, sha256: &str) -> Option<&EmbeddingModel> {
        self.models
            .iter()
            .find(|m| m.sha256.eq_ignore_ascii_case(sha256))
    }

    /// The entry this machine should get: the largest that fits in memory at
    /// the same time as the language model, the smallest otherwise. Entries
    /// marked `by_request` are not considered.
    ///
    /// `language_model_bytes` is the size of the language model's weights;
    /// `None` counts no language model. Both footprints come from
    /// [`estimate_memory`], and they are added because each model is served by
    /// its own engine process. `None` only when no entry can be recommended.
    #[must_use]
    pub fn recommend(
        &self,
        profile: &HardwareProfile,
        language_model_bytes: Option<u64>,
    ) -> Option<EmbeddingRecommendation> {
        let architecture = MemoryArchitecture::of(profile);
        let accelerated = architecture.uses_accelerator();
        let capacity_gb = architecture.total_capacity_gb(profile);
        let language_gb = language_model_bytes.map_or(0.0, |bytes| {
            estimate_memory(
                bytes,
                &GgufHeaderFacts::default(),
                &RuntimeShape::default(),
                accelerated,
            )
            .total_gb
        });

        let mut by_size: Vec<&EmbeddingModel> =
            self.models.iter().filter(|m| !m.by_request).collect();
        by_size.sort_by_key(|m| std::cmp::Reverse(m.size_bytes));
        let smallest = *by_size.last()?;
        let largest = *by_size.first()?;

        let fitting = by_size
            .iter()
            .copied()
            .find(|m| language_gb + m.estimate(accelerated).total_gb <= capacity_gb);
        let recommendation = match fitting {
            Some(model) if model.id == largest.id => EmbeddingRecommendation {
                model: model.id.clone(),
                reason: EmbeddingReason::FitsBeside {
                    language_gb,
                    embeddings_gb: model.estimate(accelerated).total_gb,
                    capacity_gb,
                },
            },
            other => EmbeddingRecommendation {
                model: other.unwrap_or(smallest).id.clone(),
                reason: EmbeddingReason::NoRoomBeside {
                    language_gb,
                    larger: largest.id.clone(),
                    larger_gb: largest.estimate(accelerated).total_gb,
                    capacity_gb,
                },
            },
        };
        tracing::event!(
            tracing::Level::DEBUG,
            model = %recommendation.model,
            language_gb = language_gb,
            capacity_gb = capacity_gb,
            "llm.embeddings.recommended"
        );
        Some(recommendation)
    }
}

#[cfg(test)]
mod tests;
