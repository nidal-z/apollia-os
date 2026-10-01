//! The desktop side of the installed-model index.
//!
//! The index itself lives in `apollia_llm::model_index`; this module decides
//! where it is kept (beside the models, in `~/.apollia/models/`) and when it is
//! written: reconciled with the disk at startup, extended when a download or an
//! import finishes, trimmed when a model is deleted. The Hub lists what the
//! index holds.
//!
//! Every function takes the models directory as an argument so the tests run
//! on a temporary folder, without touching the process environment.

use std::path::{Path, PathBuf};

use apollia_llm::model_index::INDEX_FILE_NAME;
use apollia_llm::{ModelIndex, ModelIndexError, ModelOrigin};

/// Where the index of `models_dir` is kept.
pub(crate) fn index_path(models_dir: &Path) -> PathBuf {
    models_dir.join(INDEX_FILE_NAME)
}

/// Read the persisted index, or start from an empty one when the file is
/// unreadable: the disk is the truth and the next rebuild restores the rest.
fn load_or_empty(models_dir: &Path) -> ModelIndex {
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

/// Reconcile the index with the disk and persist it. Run at startup.
pub(crate) fn reconcile(models_dir: &Path) -> Result<ModelIndex, ModelIndexError> {
    let mut index = load_or_empty(models_dir);
    index.rebuild(models_dir)?;
    index.save(&index_path(models_dir))?;
    Ok(index)
}

/// The index the Hub lists. Built from the disk the first time, when no index
/// has been written yet.
pub(crate) fn current(models_dir: &Path) -> Result<ModelIndex, ModelIndexError> {
    if index_path(models_dir).is_file() {
        Ok(load_or_empty(models_dir))
    } else {
        reconcile(models_dir)
    }
}

/// Add the model that owns `path` once a download or an import has finished.
pub(crate) fn record(
    models_dir: &Path,
    path: &Path,
    origin: ModelOrigin,
) -> Result<String, ModelIndexError> {
    let mut index = load_or_empty(models_dir);
    let id = index.record(models_dir, path, origin)?.id.clone();
    index.save(&index_path(models_dir))?;
    tracing::event!(
        tracing::Level::INFO,
        id = %id,
        path = %path.display(),
        "model.index.recorded"
    );
    Ok(id)
}

/// Record a finished download, logging rather than failing: the file is on
/// disk either way and the next startup reconciliation indexes it.
pub(crate) fn record_download(
    models_dir: &Path,
    path: &Path,
    repo_id: Option<String>,
    file: String,
) {
    let origin = match repo_id {
        Some(repo) => ModelOrigin::Catalog { repo, file },
        None => ModelOrigin::Import,
    };
    if let Err(e) = record(models_dir, path, origin) {
        tracing::event!(
            tracing::Level::WARN,
            path = %path.display(),
            error = %e,
            "model.index.record.failed"
        );
    }
}

/// Drop the entry of a model whose file was just deleted.
pub(crate) fn forget(models_dir: &Path, path: &Path) -> Result<(), ModelIndexError> {
    let mut index = load_or_empty(models_dir);
    index.remove_path(path);
    index.save(&index_path(models_dir))
}

#[cfg(test)]
mod tests {
    use super::*;
    use apollia_llm::ModelCategory;

    fn whisper_gguf() -> Vec<u8> {
        let key = b"general.architecture";
        let value = b"whisper";
        let mut out = Vec::new();
        out.extend_from_slice(b"GGUF");
        out.extend_from_slice(&3u32.to_le_bytes());
        out.extend_from_slice(&0u64.to_le_bytes());
        out.extend_from_slice(&1u64.to_le_bytes());
        out.extend_from_slice(&(key.len() as u64).to_le_bytes());
        out.extend_from_slice(key);
        out.extend_from_slice(&8u32.to_le_bytes());
        out.extend_from_slice(&(value.len() as u64).to_le_bytes());
        out.extend_from_slice(value);
        out
    }

    // GIVEN a models folder holding a GGUF in a sub-folder and no index yet
    // WHEN the current index is asked for, as the Hub list does
    // THEN the sub-folder model is in it and the index is now on disk
    #[test]
    fn test_current_builds_the_index_and_sees_subfolders() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        std::fs::create_dir_all(dir.join("vendor")).unwrap();
        std::fs::write(dir.join("vendor/model.gguf"), b"GGUF").unwrap();

        let index = current(dir).unwrap();

        assert_eq!(index.models.len(), 1);
        assert_eq!(index.models[0].path, dir.join("vendor/model.gguf"));
        assert!(index_path(dir).is_file());
    }

    // GIVEN an index already written, then a model copied in behind its back
    //       and another one deleted
    // WHEN the startup reconciliation runs
    // THEN the copied model is added and the deleted one removed
    #[test]
    fn test_reconcile_follows_the_disk() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        std::fs::write(dir.join("old.gguf"), b"GGUF").unwrap();
        reconcile(dir).unwrap();
        std::fs::remove_file(dir.join("old.gguf")).unwrap();
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("sub/new.gguf"), b"GGUF").unwrap();

        let stale = current(dir).unwrap();
        let index = reconcile(dir).unwrap();

        assert_eq!(stale.models[0].id, "old");
        let ids: Vec<&str> = index.models.iter().map(|m| m.id.as_str()).collect();
        assert_eq!(ids, vec!["new"]);
        assert_eq!(current(dir).unwrap(), index);
    }

    // GIVEN a finished download of a file from a catalog repository
    // WHEN it is recorded
    // THEN the index holds it with the repository and file as its origin
    #[test]
    fn test_a_finished_download_is_recorded_with_its_origin() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        reconcile(dir).unwrap();
        let path = dir.join("qwen.gguf");
        std::fs::write(&path, b"GGUF").unwrap();

        record_download(dir, &path, Some("org/qwen-GGUF".into()), "qwen.gguf".into());

        let index = current(dir).unwrap();
        assert_eq!(
            index.get("qwen").unwrap().origin,
            ModelOrigin::Catalog {
                repo: "org/qwen-GGUF".into(),
                file: "qwen.gguf".into()
            }
        );
    }

    // GIVEN a dictation model saved as .gguf in the models folder
    // WHEN the index is rebuilt
    // THEN it is filed as dictation, where the extension alone said LLM
    #[test]
    fn test_a_gguf_dictation_model_is_filed_as_dictation() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        std::fs::write(dir.join("whisper-fr.gguf"), whisper_gguf()).unwrap();

        let index = reconcile(dir).unwrap();

        assert_eq!(
            index.get("whisper-fr").unwrap().effective_category(),
            ModelCategory::Dictation
        );
    }

    // GIVEN an indexed model whose file is then deleted
    // WHEN the deletion is reported to the index
    // THEN its entry is gone without waiting for the next startup
    #[test]
    fn test_forget_drops_a_deleted_model() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        let path = dir.join("gone.gguf");
        std::fs::write(&path, b"GGUF").unwrap();
        reconcile(dir).unwrap();

        std::fs::remove_file(&path).unwrap();
        forget(dir, &path).unwrap();

        assert!(current(dir).unwrap().models.is_empty());
    }
}
