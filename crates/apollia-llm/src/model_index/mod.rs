//! An index of the models present on this machine, GGUF and MLX.
//!
//! The inventory used to be a listing of the files sitting at the top of the
//! models directory, classified by their extension. That missed every model
//! kept in a sub-folder, could not describe an MLX model (a folder, not a
//! file), showed the parts of a split GGUF and its vision projector as models
//! of their own, and filed a whisper model saved as `.gguf` among the language
//! models.
//!
//! The index replaces the listing. One entry is one model:
//!
//! - a GGUF model is a file, or the first part of a split file whose other
//!   parts are listed in [`IndexedModel::parts`]; a projector (`mmproj`) beside
//!   it is an annex, never a model;
//! - an MLX model is a folder holding a `config.json` and at least one
//!   `*.safetensors` file.
//!
//! The category comes from the header, never from the extension: the GGML
//! magic of whisper.cpp for a `.bin`, `general.architecture` for a GGUF.
//!
//! The index is persisted as JSON beside the models ([`INDEX_FILE_NAME`]), in a
//! versioned format ([`INDEX_FORMAT_VERSION`]). [`ModelIndex::rebuild`]
//! reconciles it with the disk: a model that appeared is added, a model whose
//! files are gone is dropped, and what the disk cannot tell (origin, digest,
//! pin, overrides) is carried over from the previous entry.

mod drafts;
pub mod import;
mod overrides;
mod scan;

pub(crate) use scan::{is_projector_name, split_part};

#[cfg(test)]
mod tests;

use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Version of the persisted format. A file written with another version is
/// refused rather than misread.
pub const INDEX_FORMAT_VERSION: u32 = 1;

/// Name of the index file inside the models directory.
pub const INDEX_FILE_NAME: &str = "index.json";

/// How the weights of a model are stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModelFormat {
    /// A GGUF file, possibly split into numbered parts.
    Gguf,
    /// An MLX folder: `config.json` plus `*.safetensors` weights.
    Mlx,
}

/// What a model is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelCategory {
    /// A language model, the default for a GGUF or MLX model.
    Language,
    /// An embeddings model.
    Embeddings,
    /// A speech-to-text model (whisper.cpp, GGML or GGUF).
    Dictation,
    /// A small model of the same vocabulary, used for speculative decoding.
    Draft,
}

/// Where a model came from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ModelOrigin {
    /// Downloaded from the catalog: the repository and the file fetched.
    Catalog {
        /// Repository identifier, `owner/name`.
        repo: String,
        /// File name within the repository.
        file: String,
    },
    /// Imported by the user, or found on disk with no recorded download.
    Import,
}

/// One model of the index.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexedModel {
    /// Stable identifier, the name an agent uses for the model.
    pub id: String,
    /// Storage format.
    pub format: ModelFormat,
    /// The file for a GGUF (the first part when split), the folder for MLX.
    pub path: PathBuf,
    /// Every part of a split GGUF, in order; empty when the file is whole.
    #[serde(default)]
    pub parts: Vec<PathBuf>,
    /// Files fetched with the model and never shown on their own (projector).
    #[serde(default)]
    pub annexes: Vec<PathBuf>,
    /// Size on disk in bytes, every part and every MLX file included.
    pub size_bytes: u64,
    /// Where the model came from.
    pub origin: ModelOrigin,
    /// SHA-256 of the weights, lowercase hex, when known.
    #[serde(default)]
    pub sha256: Option<String>,
    /// Whether the origin has been verified against a published digest.
    #[serde(default)]
    pub origin_verified: bool,
    /// Category detected from the header.
    pub category: ModelCategory,
    /// Parameter count label read from the header, for example `"8B"`.
    #[serde(default)]
    pub parameters: Option<String>,
    /// Last modification time of the weights.
    #[serde(default)]
    pub modified_at: Option<DateTime<Utc>>,
    /// When the model was pinned to the top of the list, if it is.
    #[serde(default)]
    pub pinned_at: Option<DateTime<Utc>>,
    /// Category chosen by the user over the detected one.
    #[serde(default)]
    pub category_override: Option<ModelCategory>,
    /// Another weights file of the same model to load instead of `path`.
    #[serde(default)]
    pub source_file_override: Option<PathBuf>,
    /// What the model can do: tools and reasoning from its chat template,
    /// vision only when the engine can read an image for it (a GGUF with its
    /// projector attached, an MLX folder whose `config.json` declares a
    /// `vision_config`). `None` in an index written before it was computed;
    /// the next rebuild fills it.
    #[serde(default)]
    pub capabilities: Option<crate::capabilities::Capabilities>,
}

impl IndexedModel {
    /// The category in force: the override when set, the detected one otherwise.
    #[must_use]
    pub fn effective_category(&self) -> ModelCategory {
        self.category_override.unwrap_or(self.category)
    }

    /// Whether a conversation can run on the model: a language model, or a
    /// draft model, which stays one.
    #[must_use]
    pub fn is_language_model(&self) -> bool {
        matches!(
            self.effective_category(),
            ModelCategory::Language | ModelCategory::Draft
        )
    }

    /// The weights the engine loads: the source file override when set, the
    /// model's own file otherwise.
    #[must_use]
    pub fn weights_path(&self) -> &Path {
        self.source_file_override.as_deref().unwrap_or(&self.path)
    }

    /// Whether `path` is this model's file, folder, one of its parts or annexes.
    #[must_use]
    pub fn owns(&self, path: &Path) -> bool {
        self.path == path
            || self.parts.iter().any(|p| p == path)
            || self.annexes.iter().any(|p| p == path)
    }
}

/// Failure to read, write or change the index.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ModelIndexError {
    /// A file or folder could not be read or written.
    #[error("model index i/o failed at {path}")]
    Io {
        /// Path involved.
        path: PathBuf,
        /// Underlying failure.
        #[source]
        source: std::io::Error,
    },
    /// The index file is not valid JSON for this format.
    #[error("invalid model index at {path}")]
    Parse {
        /// Path of the index file.
        path: PathBuf,
        /// Underlying failure.
        #[source]
        source: serde_json::Error,
    },
    /// The index file was written in a format version this build cannot read.
    #[error("unsupported model index version {found} (expected {expected})")]
    UnsupportedVersion {
        /// Version found in the file.
        found: u32,
        /// Version this build reads.
        expected: u32,
    },
    /// No model carries this identifier.
    #[error("unknown model {id}")]
    UnknownModel {
        /// Identifier looked up.
        id: String,
    },
    /// No model owns this path after a rebuild.
    #[error("no indexed model at {path}")]
    NotIndexed {
        /// Path looked up.
        path: PathBuf,
    },
    /// A source file override pointed at a file that does not exist.
    #[error("source file not found: {path}")]
    SourceNotFound {
        /// Rejected path.
        path: PathBuf,
    },
    /// A source file override pointed at a vision projector.
    #[error("source file is a projector, not weights: {path}")]
    SourceIsProjector {
        /// Rejected path.
        path: PathBuf,
    },
    /// A source file override pointed at one part of a split file.
    #[error("source file is a part of a split model: {path}")]
    SourceIsPart {
        /// Rejected path.
        path: PathBuf,
    },
    /// A source file override pointed at the weights of another model.
    #[error("source file belongs to another model: {path}")]
    SourceFromOtherModel {
        /// Rejected path.
        path: PathBuf,
    },
}

/// The persisted index.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelIndex {
    /// Format version, [`INDEX_FORMAT_VERSION`] when written by this build.
    pub version: u32,
    /// Models, sorted by identifier.
    pub models: Vec<IndexedModel>,
}

impl Default for ModelIndex {
    fn default() -> Self {
        Self {
            version: INDEX_FORMAT_VERSION,
            models: Vec::new(),
        }
    }
}

impl ModelIndex {
    /// Read the index at `path`. A missing file is an empty index.
    ///
    /// # Errors
    /// [`ModelIndexError::Io`] when the file exists but cannot be read,
    /// [`ModelIndexError::Parse`] when it is not an index, and
    /// [`ModelIndexError::UnsupportedVersion`] for another format version.
    pub fn load(path: &Path) -> Result<Self, ModelIndexError> {
        let bytes = match std::fs::read(path) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(source) => {
                return Err(ModelIndexError::Io {
                    path: path.to_path_buf(),
                    source,
                })
            }
        };
        let probe: VersionProbe =
            serde_json::from_slice(&bytes).map_err(|source| ModelIndexError::Parse {
                path: path.to_path_buf(),
                source,
            })?;
        if probe.version != INDEX_FORMAT_VERSION {
            return Err(ModelIndexError::UnsupportedVersion {
                found: probe.version,
                expected: INDEX_FORMAT_VERSION,
            });
        }
        serde_json::from_slice(&bytes).map_err(|source| ModelIndexError::Parse {
            path: path.to_path_buf(),
            source,
        })
    }

    /// Write the index to `path`, through a temporary file renamed into place
    /// so a reader never sees half an index.
    ///
    /// # Errors
    /// [`ModelIndexError::Io`] when the file cannot be written.
    pub fn save(&self, path: &Path) -> Result<(), ModelIndexError> {
        let io = |source| ModelIndexError::Io {
            path: path.to_path_buf(),
            source,
        };
        let json = serde_json::to_vec_pretty(self).map_err(|source| ModelIndexError::Parse {
            path: path.to_path_buf(),
            source,
        })?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(io)?;
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, json).map_err(io)?;
        std::fs::rename(&tmp, path).map_err(io)
    }

    /// Reconcile the index with the models found under `models_dir`.
    ///
    /// Sub-folders are walked. A model whose files are gone is dropped; a new
    /// one is added with an import origin; an existing one keeps its origin,
    /// digest, pin and overrides. The header is read again only when the size
    /// or the modification time changed.
    ///
    /// # Errors
    /// [`ModelIndexError::Io`] when `models_dir` exists but cannot be read.
    pub fn rebuild(&mut self, models_dir: &Path) -> Result<(), ModelIndexError> {
        let found = scan::scan(models_dir).map_err(|source| ModelIndexError::Io {
            path: models_dir.to_path_buf(),
            source,
        })?;
        let ids = scan::assign_ids(models_dir, &found);
        let previous = std::mem::take(&mut self.models);

        let mut models: Vec<IndexedModel> = found
            .into_iter()
            .zip(ids)
            .map(|(model, id)| {
                let before = previous.iter().find(|p| p.path == model.path);
                merge(model, id, before)
            })
            .collect();
        models.sort_by(|a, b| a.id.cmp(&b.id));

        let dropped = previous
            .iter()
            .filter(|p| !models.iter().any(|m| m.path == p.path))
            .count();
        tracing::event!(
            tracing::Level::INFO,
            models = models.len(),
            dropped = dropped,
            "model.index.rebuilt"
        );
        self.models = models;
        self.version = INDEX_FORMAT_VERSION;
        Ok(())
    }

    /// Rebuild, then set the origin of the model that owns `path`. Called when
    /// a download or an import has finished.
    ///
    /// # Errors
    /// [`ModelIndexError::Io`] from the rebuild, or
    /// [`ModelIndexError::NotIndexed`] when no model owns `path`.
    pub fn record(
        &mut self,
        models_dir: &Path,
        path: &Path,
        origin: ModelOrigin,
    ) -> Result<&IndexedModel, ModelIndexError> {
        self.rebuild(models_dir)?;
        let model = self
            .models
            .iter_mut()
            .find(|m| m.owns(path))
            .ok_or_else(|| ModelIndexError::NotIndexed {
                path: path.to_path_buf(),
            })?;
        model.origin = origin;
        Ok(model)
    }

    /// Store the SHA-256 of the model that owns `path`, and whether it was
    /// verified against a published or already verified digest. A digest that
    /// is the pinned file of an embeddings entry of the registry files the
    /// model as embeddings, whatever its header said.
    ///
    /// # Errors
    /// [`ModelIndexError::NotIndexed`] when no model owns `path`.
    pub fn set_digest(
        &mut self,
        path: &Path,
        sha256: String,
        origin_verified: bool,
    ) -> Result<(), ModelIndexError> {
        let model = self
            .models
            .iter_mut()
            .find(|m| m.owns(path))
            .ok_or_else(|| ModelIndexError::NotIndexed {
                path: path.to_path_buf(),
            })?;
        if scan::registry_embedding(&sha256) {
            model.category = ModelCategory::Embeddings;
        }
        model.sha256 = Some(sha256);
        model.origin_verified = origin_verified;
        Ok(())
    }

    /// The digests of the models whose origin is verified: what an import of
    /// the same weights is compared with.
    #[must_use]
    pub fn verified_digests(&self) -> Vec<&str> {
        self.models
            .iter()
            .filter(|m| m.origin_verified)
            .filter_map(|m| m.sha256.as_deref())
            .collect()
    }

    /// The model carrying `id`.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<&IndexedModel> {
        self.models.iter().find(|m| m.id == id)
    }

    fn get_mut(&mut self, id: &str) -> Result<&mut IndexedModel, ModelIndexError> {
        self.models
            .iter_mut()
            .find(|m| m.id == id)
            .ok_or_else(|| ModelIndexError::UnknownModel { id: id.to_owned() })
    }

    /// Pin a model to the top of the list, dated `at`.
    ///
    /// # Errors
    /// [`ModelIndexError::UnknownModel`] when no model carries `id`.
    pub fn pin(&mut self, id: &str, at: DateTime<Utc>) -> Result<(), ModelIndexError> {
        self.get_mut(id)?.pinned_at = Some(at);
        Ok(())
    }

    /// Remove a model's pin.
    ///
    /// # Errors
    /// [`ModelIndexError::UnknownModel`] when no model carries `id`.
    pub fn unpin(&mut self, id: &str) -> Result<(), ModelIndexError> {
        self.get_mut(id)?.pinned_at = None;
        Ok(())
    }

    /// Set the category override, or remove it with `None`.
    ///
    /// # Errors
    /// [`ModelIndexError::UnknownModel`] when no model carries `id`.
    pub fn set_category_override(
        &mut self,
        id: &str,
        category: Option<ModelCategory>,
    ) -> Result<(), ModelIndexError> {
        self.get_mut(id)?.category_override = category;
        Ok(())
    }

    /// Set the source file override, or remove it with `None`.
    ///
    /// The file has to be another weights file of the same model already on
    /// this machine: a projector, a part of a split file and the weights of
    /// another model are refused.
    ///
    /// # Errors
    /// [`ModelIndexError::UnknownModel`] when no model carries `id`, and one
    /// of the `Source*` variants when the file is refused.
    pub fn set_source_file_override(
        &mut self,
        id: &str,
        source: Option<PathBuf>,
    ) -> Result<(), ModelIndexError> {
        let model = self
            .get(id)
            .ok_or_else(|| ModelIndexError::UnknownModel { id: id.to_owned() })?;
        if let Some(candidate) = &source {
            overrides::check_source_file(model, &self.models, candidate)?;
        }
        self.get_mut(id)?.source_file_override = source;
        Ok(())
    }

    /// Drop the model that owns `path`, after its files were deleted.
    pub fn remove_path(&mut self, path: &Path) {
        self.models.retain(|m| !m.owns(path));
    }
}

#[derive(Deserialize)]
struct VersionProbe {
    version: u32,
}

/// Build an entry from what the disk shows, keeping what only the previous
/// entry knows.
fn merge(found: scan::FoundModel, id: String, before: Option<&IndexedModel>) -> IndexedModel {
    let unchanged = before
        .is_some_and(|b| b.size_bytes == found.size_bytes && b.modified_at == found.modified_at);
    let (category, parameters, template_caps) = match before {
        Some(b)
            if unchanged
                && b.capabilities
                    .as_ref()
                    .is_some_and(crate::capabilities::Capabilities::is_current) =>
        {
            (
                b.category,
                b.parameters.clone(),
                b.capabilities.clone().unwrap_or_default(),
            )
        }
        _ => scan::read_header(&found),
    };
    let sha256 = before.filter(|_| unchanged).and_then(|b| b.sha256.clone());
    let category = match &sha256 {
        Some(digest) if scan::registry_embedding(digest) => ModelCategory::Embeddings,
        _ => category,
    };
    // The card is read at every rebuild rather than kept with the header: it
    // is copied beside the weights after the download, and can be removed.
    let category = match category {
        ModelCategory::Language | ModelCategory::Draft => {
            if scan::card_marks_draft(&found.path) {
                ModelCategory::Draft
            } else {
                ModelCategory::Language
            }
        }
        other => other,
    };
    let capabilities = scan::with_usable_vision(&found, template_caps);
    let source_file_override = before
        .and_then(|b| b.source_file_override.clone())
        .filter(|p| p.is_file());

    IndexedModel {
        id,
        format: found.format,
        path: found.path,
        parts: found.parts,
        annexes: found.annexes,
        size_bytes: found.size_bytes,
        origin: before.map_or(ModelOrigin::Import, |b| b.origin.clone()),
        sha256,
        origin_verified: before.is_some_and(|b| unchanged && b.origin_verified),
        category,
        parameters,
        modified_at: found.modified_at,
        pinned_at: before.and_then(|b| b.pinned_at),
        category_override: before.and_then(|b| b.category_override),
        source_file_override,
        capabilities: Some(capabilities),
    }
}
