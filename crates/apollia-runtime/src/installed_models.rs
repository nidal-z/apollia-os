//! The models installed under `<data_dir>/models/`: which configuration uses
//! each one, and the guarded removal of one of them.
//!
//! Served to the desktop running the runtime in its own process, which calls
//! these functions, and to any client of the daemon through
//! `DELETE /api/v1/llm/installed-models` (`crate::api::routes_model_hub`).
//! Both modes therefore refuse the same paths with the same words.

use std::path::{Path, PathBuf};

use apollia_core::paths::DataFile;
use apollia_core::{LlmBackendRepository, SttConfigRepository};
use apollia_llm::model_index::INDEX_FILE_NAME;
use apollia_llm::ModelIndex;

/// A model file referenced by the active configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelRef {
    /// Basename of the referenced file, used for a conservative match.
    pub name: String,
    /// Owner label surfaced to the operator.
    pub label: String,
}

/// Why an installed model was not removed.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum InstalledModelError {
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

/// Where the models of `data_dir` are kept.
pub fn models_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("models")
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
    let index_path = dir.join(INDEX_FILE_NAME);
    let mut index = ModelIndex::load(&index_path).unwrap_or_else(|e| {
        tracing::event!(
            tracing::Level::WARN,
            path = %index_path.display(),
            error = %e,
            "model.index.load.failed"
        );
        ModelIndex::default()
    });
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
