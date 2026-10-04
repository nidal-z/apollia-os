//! Tauri IPC commands of the AI screen, over the models of this machine.
//!
//! - `set_default_model`: `PUT /api/v1/llm/default-model`, a local language
//!   model named by its identifier in the model index.
//! - `set_embeddings_default`: `PUT /api/v1/llm/embeddings/default`, the
//!   registry entry search by meaning uses.
//! - `set_installed_model_pinned`: pins a model to the top of its category, or
//!   removes the pin, in the model index of the models folder.
//! - `reveal_installed_model`: shows a model in the file manager of this
//!   machine, refused for a path outside the models folder.
//!
//! The first two are runtime routes and serve both modes through the methods
//! of `attached/proxy.rs`. The pin needs no route: it writes the index of the
//! models folder, which the daemon shares since it listens on this machine,
//! as the download of a variant does.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::attached::RuntimeLink;

const DEFAULT_MODEL_PATH: &str = "/api/v1/llm/default-model";
const EMBEDDINGS_DEFAULT_PATH: &str = "/api/v1/llm/embeddings/default";

/// The default model of the machine, as the runtime answers it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DefaultModelView {
    /// The backend row the default lives on.
    pub backend: String,
    /// `local` or `server`.
    pub model_source: String,
    /// The model, in the form the route accepts.
    pub model_id: String,
}

/// The embeddings model search uses, as the runtime answers it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EmbeddingsDefaultView {
    /// Registry entry; absent when search uses keywords alone.
    pub model: Option<String>,
    /// Whether the file of that entry is installed.
    pub installed: bool,
    /// Path of that file, when installed.
    pub path: Option<String>,
}

fn default_model_body(model_id: &str) -> serde_json::Value {
    serde_json::json!({ "model_source": "local", "model_id": model_id })
}

fn embeddings_default_body(entry: &str) -> serde_json::Value {
    serde_json::json!({ "model": entry })
}

/// Makes a local language model the default of the machine, named by its
/// identifier in the model index. An embeddings or dictation model is refused
/// by the runtime: it never chats.
#[tauri::command]
pub async fn set_default_model(
    link: State<'_, RuntimeLink>,
    model_id: String,
) -> Result<DefaultModelView, String> {
    let json = link
        .put_json(DEFAULT_MODEL_PATH, &default_model_body(&model_id))
        .await?;
    serde_json::from_value(json).map_err(|e| format!("invalid default-model response: {e}"))
}

/// Makes a registry entry the embeddings model search by meaning uses. The
/// sources and the memory are indexed again with it.
#[tauri::command]
pub async fn set_embeddings_default(
    link: State<'_, RuntimeLink>,
    entry: String,
) -> Result<EmbeddingsDefaultView, String> {
    let json = link
        .put_json(EMBEDDINGS_DEFAULT_PATH, &embeddings_default_body(&entry))
        .await?;
    serde_json::from_value(json).map_err(|e| format!("invalid embeddings response: {e}"))
}

/// Pins the model `id` to the top of its category, or removes its pin, in the
/// index of `models_dir`.
fn pin_in(models_dir: &Path, id: &str, pinned: bool) -> Result<(), String> {
    let mut index = super::model_index::current(models_dir).map_err(|e| e.to_string())?;
    if pinned {
        index.pin(id, chrono::Utc::now())
    } else {
        index.unpin(id)
    }
    .map_err(|e| e.to_string())?;
    index
        .save(&super::model_index::index_path(models_dir))
        .map_err(|e| e.to_string())?;
    tracing::event!(
        tracing::Level::INFO,
        id = %id,
        pinned = pinned,
        "model.index.pin"
    );
    Ok(())
}

/// Pins an installed model to the top of its category, or removes its pin.
/// The pin orders the list and keeps nothing in memory.
#[tauri::command]
pub async fn set_installed_model_pinned(id: String, pinned: bool) -> Result<(), String> {
    let dir = super::model_hub::models_dir();
    tokio::task::spawn_blocking(move || pin_in(&dir, &id, pinned))
        .await
        .map_err(|e| format!("spawn_blocking failed: {e}"))?
}

/// `path` resolved, when it lies inside `models_dir`.
fn inside_models_dir(models_dir: &Path, path: &str) -> Result<PathBuf, String> {
    let target = Path::new(path)
        .canonicalize()
        .map_err(|e| format!("model not found: {e}"))?;
    let root = models_dir
        .canonicalize()
        .map_err(|e| format!("models folder not found: {e}"))?;
    if target.starts_with(&root) {
        Ok(target)
    } else {
        Err("refusing to reveal a file outside the models folder".to_owned())
    }
}

/// Shows an installed model, or the models folder itself, in the file manager
/// of this machine (Finder, Explorer, the file manager of the desktop).
#[tauri::command]
pub async fn reveal_installed_model(app: tauri::AppHandle, path: String) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;

    let target = inside_models_dir(&super::model_hub::models_dir(), &path)?;
    app.opener().reveal_item_in_dir(&target).map_err(|e| {
        tracing::event!(
            tracing::Level::WARN,
            target = %target.display(),
            error = %e,
            "model.reveal.failed"
        );
        format!("failed to reveal the model: {e}")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_the_default_model_body_names_a_local_model_by_its_index_identifier() {
        // GIVEN the identifier of an installed language model
        let id = "qwen3-8b-q4_k_m";

        // WHEN the body of the request is built
        let body = default_model_body(id);

        // THEN it asks for a local model under that identifier
        assert_eq!(body["model_source"], "local");
        assert_eq!(body["model_id"], id);
    }

    #[test]
    fn test_the_runtime_answers_parse_into_their_views() {
        // GIVEN the bodies the two routes answer
        let default = serde_json::json!({
            "backend": "local-qwen", "model_source": "local", "model_id": "local-qwen"
        });
        let search = serde_json::json!({
            "model": "multilingual-e5-small", "installed": true, "path": "/m/e5.gguf"
        });

        // WHEN they are read
        let default: DefaultModelView = serde_json::from_value(default).unwrap();
        let search: EmbeddingsDefaultView = serde_json::from_value(search).unwrap();

        // THEN every field is kept
        assert_eq!(default.backend, "local-qwen");
        assert_eq!(search.model.as_deref(), Some("multilingual-e5-small"));
        assert!(search.installed);
        assert_eq!(embeddings_default_body("e5")["model"], "e5");
    }

    #[test]
    fn test_a_pin_is_written_to_the_index_survives_a_relaunch_and_is_removed() {
        // GIVEN a models folder holding one model
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        std::fs::write(dir.join("chat.gguf"), b"GGUF").unwrap();
        super::super::model_index::reconcile(dir).unwrap();

        // WHEN it is pinned, reconciled as at the next launch, then unpinned
        pin_in(dir, "chat", true).unwrap();
        let relaunched = super::super::model_index::reconcile(dir).unwrap();
        pin_in(dir, "chat", false).unwrap();
        let unpinned = super::super::model_index::current(dir).unwrap();

        // THEN the pin survived the relaunch, then went; an unknown model fails
        assert!(relaunched.get("chat").unwrap().pinned_at.is_some());
        assert!(unpinned.get("chat").unwrap().pinned_at.is_none());
        assert!(pin_in(dir, "absent", true).is_err());
    }

    #[test]
    fn test_only_a_path_inside_the_models_folder_is_revealed() {
        // GIVEN a model inside the models folder and a file beside it
        let tmp = tempfile::tempdir().unwrap();
        let models = tmp.path().join("models");
        std::fs::create_dir_all(&models).unwrap();
        std::fs::write(models.join("chat.gguf"), b"GGUF").unwrap();
        std::fs::write(tmp.path().join("secret.txt"), b"x").unwrap();
        let outside = models.join("../secret.txt");

        // WHEN each is checked
        let inside = inside_models_dir(&models, &models.join("chat.gguf").display().to_string());
        let escape = inside_models_dir(&models, &outside.display().to_string());
        let folder = inside_models_dir(&models, &models.display().to_string());

        // THEN the model and the folder pass, the escape is refused
        assert!(inside.is_ok());
        assert!(folder.is_ok());
        assert!(escape.unwrap_err().contains("outside the models folder"));
    }
}
