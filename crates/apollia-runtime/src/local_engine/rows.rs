//! A local model chosen by its index identifier gets the backend row the
//! router routes.
//!
//! The router knows backend rows, while the model screens and the agents name
//! a local model by its identifier in the model index. Choosing such a model
//! (as the default) creates the `llama-cpp` row that designates it, or points
//! the existing one at the model's current file or folder. The engine is not
//! stored on the row: the router reads the format from the index at every
//! build (see [`super::model_format`]).

use std::path::Path;

use apollia_core::{LlmBackendConfig, LlmBackendError, LlmBackendRepository, LlmProvider};
use apollia_llm::model_index::{ModelCategory, ModelIndex, INDEX_FILE_NAME};

/// The row name for an index identifier: lowercase, with every character a
/// row name refuses replaced by `-`.
#[must_use]
pub fn row_name(index_id: &str) -> String {
    index_id
        .chars()
        .map(|c| {
            let c = c.to_ascii_lowercase();
            if c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect()
}

/// The backend row that serves `model_id`, created or updated from the index.
///
/// `model_id` already naming a row leaves the table alone and answers it. An
/// identifier of a language model of the index answers the row named after
/// it, written now with the model's path. Anything else answers `None`, and
/// the caller reports the backend as not found.
///
/// # Errors
/// [`LlmBackendError`] when the table cannot be read or written.
pub fn local_row_for(
    repo: &LlmBackendRepository,
    models_dir: &Path,
    model_id: &str,
) -> Result<Option<String>, LlmBackendError> {
    if repo.find_by_name(model_id)?.is_some() {
        return Ok(Some(model_id.to_owned()));
    }
    let Ok(index) = ModelIndex::load(&models_dir.join(INDEX_FILE_NAME)) else {
        return Ok(None);
    };
    let Some(model) = index.get(model_id) else {
        return Ok(None);
    };
    if model.effective_category() != ModelCategory::Language {
        return Ok(None);
    }
    let name = row_name(model_id);
    let path = model.path.display().to_string();
    let row = match repo.find_by_name(&name)? {
        Some(existing) if existing.provider == LlmProvider::LlamaCpp && existing.model == path => {
            return Ok(Some(name));
        }
        Some(existing) if existing.provider == LlmProvider::LlamaCpp => LlmBackendConfig {
            model: path,
            ..existing
        },
        Some(_) => return Ok(None),
        None => LlmBackendConfig {
            name: name.clone(),
            provider: LlmProvider::LlamaCpp,
            model: path,
            config_json: serde_json::json!({}),
            is_default: false,
            enabled: true,
        },
    };
    repo.save(&row)?;
    tracing::event!(
        tracing::Level::INFO,
        backend = %name,
        model_id = %model_id,
        format = ?model.format,
        "llm.local_row.written"
    );
    Ok(Some(name))
}

#[cfg(test)]
mod tests {
    use apollia_llm::model_index::{IndexedModel, ModelFormat, ModelOrigin};

    use super::*;

    fn indexed(id: &str, path: &Path, category: ModelCategory) -> IndexedModel {
        IndexedModel {
            id: id.to_owned(),
            format: ModelFormat::Mlx,
            path: path.to_path_buf(),
            parts: Vec::new(),
            annexes: Vec::new(),
            size_bytes: 0,
            origin: ModelOrigin::Import,
            sha256: None,
            origin_verified: false,
            category,
            parameters: None,
            modified_at: None,
            pinned_at: None,
            category_override: None,
            source_file_override: None,
            capabilities: None,
        }
    }

    #[test]
    fn a_model_chosen_by_its_index_identifier_gets_a_local_row_once() {
        // GIVEN an index holding an MLX language model and an embeddings model,
        // and an empty backend table
        let tmp = tempfile::tempdir().expect("tempdir");
        let models = apollia_core::config::default_models_dir(tmp.path());
        std::fs::create_dir_all(&models).expect("mkdir");
        let folder = models.join("Qwen3-4B-MLX-4bit");
        ModelIndex {
            models: vec![
                indexed("Qwen3-4B-MLX-4bit", &folder, ModelCategory::Language),
                indexed("embed", &models.join("embed"), ModelCategory::Embeddings),
            ],
            ..ModelIndex::default()
        }
        .save(&models.join(INDEX_FILE_NAME))
        .expect("save");
        let repo = LlmBackendRepository::open(&tmp.path().join("system.db")).expect("repo");

        // WHEN the language model is chosen twice, and the others once
        let first = local_row_for(&repo, &models, "Qwen3-4B-MLX-4bit").expect("first");
        let again = local_row_for(&repo, &models, "Qwen3-4B-MLX-4bit").expect("again");
        let embed = local_row_for(&repo, &models, "embed").expect("embed");
        let ghost = local_row_for(&repo, &models, "ghost").expect("ghost");

        // THEN one local row designates the folder, under a valid row name,
        // and nothing is written for a model the chat cannot use
        assert_eq!(first.as_deref(), Some("qwen3-4b-mlx-4bit"));
        assert_eq!(again, first);
        assert_eq!(embed, None);
        assert_eq!(ghost, None);
        let rows = repo.list().expect("list");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].provider, LlmProvider::LlamaCpp);
        assert_eq!(rows[0].model, folder.display().to_string());
    }
}
