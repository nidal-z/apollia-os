//! The models installed under `<data_dir>/models/`, as the model index holds
//! them: where the index is kept, the list the Hub shows, which configuration
//! uses each model, and the guarded removal of one of them.
//!
//! The index itself lives in `apollia_llm::model_index`. The desktop running
//! the runtime in its own process calls these functions, and any client of the
//! daemon reaches them through `GET` and `DELETE /api/v1/llm/installed-models`
//! (`crate::api::routes_model_hub::installed`). Both modes therefore list the
//! same models and refuse the same paths with the same words.

use std::path::{Path, PathBuf};

use apollia_core::paths::DataFile;
use apollia_core::{LlmBackendRepository, SttConfigRepository};
use apollia_llm::model_index::INDEX_FILE_NAME;
use apollia_llm::{IndexedModel, ModelCategory, ModelFormat, ModelIndex, ModelIndexError};
use serde::Serialize;

/// Why the installed models could not be listed, or one of them removed.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum InstalledModelError {
    /// The index could neither be read nor rebuilt from the disk.
    #[error("failed to read the model index: {0}")]
    Index(#[from] ModelIndexError),
    /// The path names no file.
    #[error("model file not found: {0}")]
    NotFound(#[source] std::io::Error),
    /// The models directory does not exist.
    #[error("models directory not found: {0}")]
    NoModelsDir(#[source] std::io::Error),
    /// The path resolves outside the models directory.
    #[error("refusing to delete a file outside the models directory")]
    OutsideModelsDir,
    /// An LLM backend or the dictation engine still uses the file.
    #[error("model is in use by {owner}; change or remove that configuration first")]
    InUse {
        /// The configuration that references it.
        owner: String,
    },
    /// The file could not be removed.
    #[error("failed to delete model file: {0}")]
    Delete(#[source] std::io::Error),
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelRef {
    /// Basename of the referenced file, used for a conservative match.
    pub name: String,
    /// Owner label surfaced to the operator.
    pub label: String,
}

/// The models folder in force for `data_dir`: the `[models] dir` setting,
/// else `<data_dir>/models` (see [`apollia_core::config::models_dir`]).
pub fn models_dir(data_dir: &Path) -> PathBuf {
    apollia_core::config::models_dir(data_dir)
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

/// Every model file referenced by an LLM backend or the dictation
/// configuration of `data_dir/system.db`.
///
/// A backend whose `model` is a cloud model name (not a path) never matches a
/// local file, so it is harmless here. An unreadable database yields no
/// reference rather than an error, as the listing always did.
pub fn in_use_refs(data_dir: &Path) -> Vec<ModelRef> {
    let mut refs = Vec::new();
    let db_path = DataFile::System.path(data_dir);

    let mut push = |raw: &str, label: String| {
        if let Some(name) = Path::new(raw).file_name() {
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

    if let Ok(repo) = SttConfigRepository::open(&db_path) {
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
/// [`InstalledModelError::Index`] when the index can be neither read nor
/// built.
pub fn list(data_dir: &Path) -> Result<Vec<InstalledModelView>, InstalledModelError> {
    list_in(&models_dir(data_dir), &in_use_refs(data_dir))
}

/// The installed models of the folder `dir`, read from its model index.
///
/// # Errors
///
/// [`InstalledModelError::Index`] when the index can be neither read nor
/// built.
pub fn list_in(
    dir: &Path,
    refs: &[ModelRef],
) -> Result<Vec<InstalledModelView>, InstalledModelError> {
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

/// Removes the installed model at `path`, then its entry in the model index.
///
/// # Errors
///
/// Refuses a path that names no file, a path resolving outside
/// `<data_dir>/models/` (traversal, a link pointing outside), and a file still
/// referenced by an LLM backend or the dictation configuration.
pub fn delete_installed(data_dir: &Path, path: &str) -> Result<(), InstalledModelError> {
    let target = PathBuf::from(path);
    let dir = models_dir(data_dir);

    let canonical_target = target
        .canonicalize()
        .map_err(InstalledModelError::NotFound)?;
    let canonical_dir = dir
        .canonicalize()
        .map_err(InstalledModelError::NoModelsDir)?;
    if !canonical_target.starts_with(&canonical_dir) {
        return Err(InstalledModelError::OutsideModelsDir);
    }

    if let Some(owner) = used_by(&canonical_target, &in_use_refs(data_dir)) {
        return Err(InstalledModelError::InUse { owner });
    }

    std::fs::remove_file(&canonical_target).map_err(InstalledModelError::Delete)?;
    forget(&dir, &[&target, &canonical_target]);
    Ok(())
}

/// Drops the index entries of `paths`, the file just deleted under each of
/// its spellings. A failure is logged: the next reconciliation repairs it.
fn forget(dir: &Path, paths: &[&Path]) {
    let index_path = index_path(dir);
    let mut index = load_or_empty(dir);
    for path in paths {
        index.remove_path(path);
    }
    if let Err(e) = index.save(&index_path) {
        tracing::event!(
            tracing::Level::WARN,
            path = %index_path.display(),
            error = %e,
            "model.index.forget.failed"
        );
    }
}

#[cfg(test)]
mod tests;
