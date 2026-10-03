//! The vision projector a GGUF model is launched with.
//!
//! A projector (`mmproj`) is fetched with its model and recorded as an annex in
//! the model index kept beside the models. The engine only uses it when it is
//! named on the command line, so the launch looks it up here: the model that
//! owns the weights about to be loaded, and the first of its annexes still on
//! disk.

use std::path::{Path, PathBuf};

use apollia_llm::model_index::INDEX_FILE_NAME;
use apollia_llm::ModelIndex;

/// The projector of the model at `model_path`, from the index of the user's
/// models directory.
pub(super) fn for_model(model_path: &str) -> Option<String> {
    let home = apollia_core::paths::home_dir_or_temp();
    let models_dir = apollia_core::config::models_dir(&apollia_core::paths::data_dir_under(home));
    in_models_dir(&models_dir, Path::new(model_path)).map(|p| p.display().to_string())
}

/// The projector of `model` among the models of `models_dir`.
///
/// The persisted index is read first. A model it does not know (written by a
/// download whose recording failed, or before any index existed) is looked up
/// in an index rebuilt from the disk, kept in memory only.
pub(super) fn in_models_dir(models_dir: &Path, model: &Path) -> Option<PathBuf> {
    let mut index = match ModelIndex::load(&models_dir.join(INDEX_FILE_NAME)) {
        Ok(index) => index,
        Err(e) => {
            tracing::event!(
                tracing::Level::WARN,
                path = %models_dir.display(),
                error = %e,
                "llama.server.projector.index_unreadable"
            );
            ModelIndex::default()
        }
    };
    if let Some(found) = annex_of(&index, model) {
        return Some(found);
    }
    if !model.starts_with(models_dir) {
        return None;
    }
    if let Err(e) = index.rebuild(models_dir) {
        tracing::event!(
            tracing::Level::WARN,
            path = %models_dir.display(),
            error = %e,
            "llama.server.projector.rebuild_failed"
        );
        return None;
    }
    annex_of(&index, model)
}

fn annex_of(index: &ModelIndex, model: &Path) -> Option<PathBuf> {
    index
        .models
        .iter()
        .find(|m| m.owns(model) || m.source_file_override.as_deref() == Some(model))
        .and_then(|m| m.annexes.iter().find(|a| a.is_file()).cloned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, bytes: &[u8]) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("mkdir");
        }
        std::fs::write(path, bytes).expect("write");
    }

    #[test]
    fn test_projector_of_a_model_with_an_annex_is_found() {
        // GIVEN a models folder holding a variant and its projector, indexed
        let dir = tempfile::tempdir().expect("tempdir");
        let weights = dir.path().join("Vision-Q4_K_M/Vision-Q4_K_M.gguf");
        let projector = dir.path().join("Vision-Q4_K_M/mmproj-Vision-f16.gguf");
        write(&weights, b"GGUF");
        write(&projector, b"GGUF");
        let mut index = ModelIndex::default();
        index.rebuild(dir.path()).expect("rebuild");
        index.save(&dir.path().join(INDEX_FILE_NAME)).expect("save");

        // WHEN the projector of the weights is looked up
        let found = in_models_dir(dir.path(), &weights);

        // THEN it is the annex
        assert_eq!(found, Some(projector));
    }

    #[test]
    fn test_projector_is_found_without_a_persisted_index() {
        // GIVEN a models folder holding a variant and its projector, and no index file
        let dir = tempfile::tempdir().expect("tempdir");
        let weights = dir.path().join("V/V-Q4_K_M.gguf");
        let projector = dir.path().join("V/mmproj-V-f16.gguf");
        write(&weights, b"GGUF");
        write(&projector, b"GGUF");

        // WHEN the projector of the weights is looked up
        let found = in_models_dir(dir.path(), &weights);

        // THEN the disk answers
        assert_eq!(found, Some(projector));
    }

    #[test]
    fn test_model_without_projector_has_none() {
        // GIVEN a models folder holding a text-only model
        let dir = tempfile::tempdir().expect("tempdir");
        let weights = dir.path().join("m-Q4_K_M.gguf");
        write(&weights, b"GGUF");

        // WHEN its projector is looked up
        let found = in_models_dir(dir.path(), &weights);

        // THEN there is none
        assert_eq!(found, None);
    }
}
