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
use apollia_llm::model_index::{ModelIndex, INDEX_FILE_NAME};

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
    if !model.is_language_model() {
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

/// The route a chat session names for a chosen model, and whether choosing it
/// wrote a backend row the router in force does not know yet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatRoute {
    /// The name the router routes by: a backend row, or `<server>/<model>`.
    pub name: String,
    /// True when a row was created or repointed, so the router must be
    /// rebuilt before the session can reach it.
    pub written: bool,
}

/// The route of `model` for a chat session.
///
/// A language model of the index named by its identifier gets its row, as
/// [`local_row_for`] writes it. Any other name, a backend row or a model of a
/// declared server (`<server>/<model>`), is the route as it stands; one the
/// router does not know is reported at the turn as a fallback.
///
/// # Errors
/// [`LlmBackendError`] when the table cannot be read or written.
pub fn chat_route(
    repo: &LlmBackendRepository,
    models_dir: &Path,
    model: &str,
) -> Result<ChatRoute, LlmBackendError> {
    let model = model.trim();
    let candidate = row_name(model);
    let before_as_named = repo.find_by_name(model)?.map(|row| row.model);
    let before_as_row = repo.find_by_name(&candidate)?.map(|row| row.model);
    let Some(name) = local_row_for(repo, models_dir, model)? else {
        return Ok(ChatRoute {
            name: model.to_owned(),
            written: false,
        });
    };
    let before = if name == model {
        before_as_named
    } else {
        before_as_row
    };
    let after = repo.find_by_name(&name)?.map(|row| row.model);
    Ok(ChatRoute {
        written: before != after,
        name,
    })
}

#[cfg(test)]
mod tests {
    use apollia_llm::model_index::{IndexedModel, ModelCategory, ModelFormat, ModelOrigin};

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

    #[test]
    fn a_chat_route_says_when_choosing_a_model_wrote_its_row() {
        // GIVEN an index holding a GGUF language model and an embeddings
        // model, and an empty backend table
        let tmp = tempfile::tempdir().expect("tempdir");
        let models = apollia_core::config::default_models_dir(tmp.path());
        std::fs::create_dir_all(&models).expect("mkdir");
        let file = models.join("Qwen3-4B-Q4_K_M.gguf");
        let mut gguf = indexed("Qwen3-4B-Q4_K_M.gguf", &file, ModelCategory::Language);
        gguf.format = ModelFormat::Gguf;
        ModelIndex {
            models: vec![
                gguf,
                indexed("embed", &models.join("embed"), ModelCategory::Embeddings),
            ],
            ..ModelIndex::default()
        }
        .save(&models.join(INDEX_FILE_NAME))
        .expect("save");
        let repo = LlmBackendRepository::open(&tmp.path().join("system.db")).expect("repo");

        // WHEN the chat chooses the language model twice, the row it got, a
        // model of a declared server and the embeddings model
        let first = chat_route(&repo, &models, "Qwen3-4B-Q4_K_M.gguf").expect("first");
        let again = chat_route(&repo, &models, "Qwen3-4B-Q4_K_M.gguf").expect("again");
        let by_row = chat_route(&repo, &models, &first.name).expect("row");
        let server = chat_route(&repo, &models, "lab/llama-3.3-70b").expect("server");
        let embed = chat_route(&repo, &models, "embed").expect("embed");

        // THEN only the first choice writes, every choice names a route, and
        // a model the chat cannot use is passed through for the turn to report
        assert_eq!(
            first,
            ChatRoute {
                name: "qwen3-4b-q4_k_m-gguf".into(),
                written: true
            }
        );
        assert_eq!(again.name, first.name);
        assert!(!again.written);
        assert_eq!(by_row, again);
        assert_eq!(server.name, "lab/llama-3.3-70b");
        assert!(!server.written);
        assert_eq!(embed.name, "embed");
        assert!(!embed.written);
        assert_eq!(repo.list().expect("list").len(), 1);
    }
}
