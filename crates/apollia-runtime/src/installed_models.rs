//! The models installed on this machine, as the model index holds them.
//!
//! The index itself lives in `apollia_llm::model_index`; this module decides
//! where it is kept (beside the models, in `<data dir>/models/`), reads it
//! for the list the Hub shows, and says which model a configuration still
//! references. The desktop in embedded mode and the daemon's
//! `GET /api/v1/llm/models/installed` call the same functions, so both modes
//! list the same models.

use std::path::{Path, PathBuf};

use apollia_core::LlmBackendRepository;
use apollia_llm::model_index::INDEX_FILE_NAME;
use apollia_llm::{IndexedModel, ModelCategory, ModelFormat, ModelIndex, ModelIndexError};
use serde::Serialize;

/// Why the installed models could not be listed.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum InstalledModelsError {
    /// The index could neither be read nor rebuilt from the disk.
    #[error("failed to read the model index: {0}")]
    Index(#[from] ModelIndexError),
}

/// Classification of an installed model, from the category its header declares.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum InstalledModelKind {
    /// GGUF weights for local LLM inference.
    Llm,
    /// GGML weights for local speech-to-text.
    Stt,
}

/// A model installed under the models folder, as the model index holds it.
#[derive(Debug, Clone, Serialize)]
pub struct InstalledModelView {
    /// Identifier in the model index.
    pub id: String,
    /// Absolute path on disk: the file of a GGUF, the folder of an MLX model.
    pub path: String,
    /// File or folder name (basename).
    pub name: String,
    /// Size in bytes, every part of a split file included.
    pub size_bytes: u64,
    /// LLM or STT, from the category in force.
    pub kind: InstalledModelKind,
    /// Storage format.
    pub format: ModelFormat,
    /// Category in force (the developer override when set).
    pub category: ModelCategory,
    /// Parameter count label read from the header, for example `"8B"`.
    pub parameters: Option<String>,
    /// Last modification time, RFC 3339.
    pub modified_at: Option<String>,
    /// When the model was pinned to the top of the list, RFC 3339.
    pub pinned_at: Option<String>,
    /// Whether the weights matched a published or already verified digest.
    /// False for a model imported from a disk with an unknown digest.
    pub origin_verified: bool,
    /// True when an LLM backend or the STT config references this file.
    pub in_use: bool,
    /// Human-readable owner when `in_use` (e.g. `LLM backend 'local-code'`).
    pub used_by: Option<String>,
    /// What the model can do, vision included only when the engine can read
    /// an image for it. `None` until the index has been rebuilt by this build.
    pub capabilities: Option<apollia_llm::Capabilities>,
}

/// A model file referenced by the active configuration.
#[derive(Debug, Clone)]
pub struct ModelRef {
    /// Basename of the referenced file, used for a conservative match.
    name: String,
    /// Owner label surfaced to the operator.
    label: String,
}

/// The models folder of `data_dir`.
pub fn models_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("models")
}

/// Where the index of `models_dir` is kept.
pub fn index_path(models_dir: &Path) -> PathBuf {
    models_dir.join(INDEX_FILE_NAME)
}

/// Read the persisted index, or start from an empty one when the file is
/// unreadable: the disk is the truth and the next rebuild restores the rest.
pub fn load_or_empty(models_dir: &Path) -> ModelIndex {
    let path = index_path(models_dir);
    ModelIndex::load(&path).unwrap_or_else(|e| {
        tracing::event!(
            tracing::Level::WARN,
            path = %path.display(),
            error = %e,
            "model.index.load.failed"
        );
        ModelIndex::default()
    })
}

/// Reconcile the index with the disk and persist it.
///
/// # Errors
///
/// [`ModelIndexError`] when the folder cannot be walked or the index written.
pub fn reconcile(models_dir: &Path) -> Result<ModelIndex, ModelIndexError> {
    let mut index = load_or_empty(models_dir);
    index.rebuild(models_dir)?;
    index.save(&index_path(models_dir))?;
    Ok(index)
}

/// The index the Hub lists. Built from the disk the first time, when no index
/// has been written yet.
///
/// # Errors
///
/// [`ModelIndexError`] when no index exists and the first build fails.
pub fn current(models_dir: &Path) -> Result<ModelIndex, ModelIndexError> {
    if index_path(models_dir).is_file() {
        Ok(load_or_empty(models_dir))
    } else {
        reconcile(models_dir)
    }
}

fn expand_tilde(raw: &str) -> PathBuf {
    match raw.strip_prefix("~/") {
        Some(rest) => apollia_core::paths::home_dir_or_temp().join(rest),
        None => PathBuf::from(raw),
    }
}

/// Every model file referenced by an LLM backend or the STT config of
/// `data_dir`.
///
/// Read-only over `system.db`. A backend whose `model` is a cloud model name
/// (not a path) simply never matches a local `.gguf`, so it is harmless here.
pub fn in_use_refs(data_dir: &Path) -> Vec<ModelRef> {
    let mut refs = Vec::new();
    let db_path = apollia_core::paths::DataFile::System.path(data_dir);

    let mut push = |raw: &str, label: String| {
        if let Some(name) = expand_tilde(raw).file_name() {
            refs.push(ModelRef {
                name: name.to_string_lossy().to_string(),
                label,
            });
        }
    };

    if let Ok(repo) = LlmBackendRepository::open(&db_path) {
        if let Ok(list) = repo.list() {
            for cfg in list {
                let label = format!("LLM backend '{}'", cfg.name);
                push(&cfg.model, label.clone());
                if let Some(mp) = cfg.config_json.get("model_path").and_then(|v| v.as_str()) {
                    push(mp, label);
                }
            }
        }
    }

    if let Ok(repo) = apollia_core::SttConfigRepository::open(&db_path) {
        if let Ok(row) = repo.get_or_default() {
            push(&row.model_path, "STT engine".to_string());
        }
    }

    refs
}

/// Conservative match: same file name as a referenced model. Prefers a false
/// "in use" (blocks a delete) over an accidental removal of a live model.
pub fn used_by(file: &Path, refs: &[ModelRef]) -> Option<String> {
    let name = file.file_name()?.to_string_lossy().to_string();
    refs.iter()
        .find(|r| r.name == name)
        .map(|r| r.label.clone())
}

/// The installed models of `data_dir`, each marked when a configuration
/// references it.
///
/// # Errors
///
/// [`InstalledModelsError::Index`] when the index can be neither read nor
/// built.
pub fn list(data_dir: &Path) -> Result<Vec<InstalledModelView>, InstalledModelsError> {
    list_in(&models_dir(data_dir), &in_use_refs(data_dir))
}

/// The installed models of the folder `dir`, read from its model index.
///
/// # Errors
///
/// [`InstalledModelsError::Index`] when the index can be neither read nor
/// built.
pub fn list_in(
    dir: &Path,
    refs: &[ModelRef],
) -> Result<Vec<InstalledModelView>, InstalledModelsError> {
    let index = current(dir)?;
    let mut out: Vec<InstalledModelView> = index
        .models
        .iter()
        .map(|model| installed_view(model, refs))
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

fn installed_view(model: &IndexedModel, refs: &[ModelRef]) -> InstalledModelView {
    let category = model.effective_category();
    let kind = match category {
        ModelCategory::Dictation => InstalledModelKind::Stt,
        _ => InstalledModelKind::Llm,
    };
    let owner = used_by(&model.path, refs);
    InstalledModelView {
        id: model.id.clone(),
        path: model.path.to_string_lossy().to_string(),
        name: model
            .path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default(),
        size_bytes: model.size_bytes,
        kind,
        format: model.format,
        category,
        parameters: model.parameters.clone(),
        modified_at: model.modified_at.map(|t| t.to_rfc3339()),
        pinned_at: model.pinned_at.map(|t| t.to_rfc3339()),
        origin_verified: model.origin_verified,
        in_use: owner.is_some(),
        used_by: owner,
        capabilities: model.capabilities.clone(),
    }
}

#[cfg(test)]
mod tests;
