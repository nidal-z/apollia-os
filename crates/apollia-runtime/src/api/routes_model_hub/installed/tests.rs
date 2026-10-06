use std::path::Path;

use apollia_core::{LlmBackendConfig, LlmBackendRepository, LlmProvider};
use apollia_llm::model_index::INDEX_FILE_NAME;
use apollia_llm::ModelIndex;
use axum::http::StatusCode;
use tempfile::TempDir;

use crate::api::server::AppState;
use crate::api::test_support::{app_state, call, MockBackend};

fn state(dir: &TempDir) -> AppState<MockBackend> {
    app_state(dir.path().to_path_buf())
}

/// A models folder under `dir` holding `name`, indexed.
fn installed(dir: &TempDir, name: &str) -> std::path::PathBuf {
    let models = apollia_core::config::default_models_dir(dir.path());
    std::fs::create_dir_all(&models).unwrap();
    let file = models.join(name);
    std::fs::write(&file, b"GGUF").unwrap();
    let mut index = ModelIndex::default();
    index.rebuild(&models).unwrap();
    index.save(&models.join(INDEX_FILE_NAME)).unwrap();
    file
}

fn indexed(dir: &TempDir) -> usize {
    ModelIndex::load(&apollia_core::config::default_models_dir(dir.path()).join(INDEX_FILE_NAME))
        .unwrap()
        .models
        .len()
}

/// The route of the removal of `path`, its value percent-encoded.
fn route(path: &Path) -> String {
    let encoded: String = path
        .to_string_lossy()
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            other => format!("%{other:02X}"),
        })
        .collect();
    format!("/api/v1/llm/installed-models?path={encoded}")
}

#[tokio::test]
async fn test_an_installed_model_is_removed_with_its_index_entry() {
    // GIVEN a daemon whose models folder holds one indexed model
    let dir = TempDir::new().unwrap();
    let file = installed(&dir, "qwen3-8b.gguf");
    assert_eq!(indexed(&dir), 1);

    // WHEN a client asks for its removal
    let (status, body) = call(state(&dir), "DELETE", &route(&file), None).await;

    // THEN the file and its index entry are gone, and the path comes back
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["deleted"], file.to_string_lossy().as_ref());
    assert!(!file.exists());
    assert_eq!(indexed(&dir), 0);
}

#[tokio::test]
async fn test_a_model_an_llm_backend_uses_is_refused_with_its_owner() {
    // GIVEN an installed model that the default LLM backend points to
    let dir = TempDir::new().unwrap();
    let file = installed(&dir, "qwen3-8b.gguf");
    LlmBackendRepository::open(&dir.path().join("system.db"))
        .unwrap()
        .save(&LlmBackendConfig {
            name: "local".into(),
            provider: LlmProvider::LlamaCpp,
            model: file.to_string_lossy().into_owned(),
            config_json: serde_json::json!({}),
            enabled: true,
            is_default: true,
        })
        .unwrap();

    // WHEN a client asks for its removal
    let (status, body) = call(state(&dir), "DELETE", &route(&file), None).await;

    // THEN it is refused as a conflict, in the words the desktop showed when
    // it ran the runtime itself, and the file stays
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(
        body["error"],
        "model is in use by LLM backend 'local'; change or remove that configuration first"
    );
    assert!(file.exists());
    assert_eq!(indexed(&dir), 1);
}

#[tokio::test]
async fn test_a_file_outside_the_models_folder_is_refused_and_kept() {
    // GIVEN a models folder, and a file of the data directory beside it
    let dir = TempDir::new().unwrap();
    installed(&dir, "qwen3-8b.gguf");
    let stray = dir.path().join("apollia.toml");
    std::fs::write(&stray, b"[models]").unwrap();

    // WHEN a client asks for the removal of that file
    let (status, body) = call(state(&dir), "DELETE", &route(&stray), None).await;

    // THEN it is forbidden and the file is still there
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(
        body["error"],
        "refusing to delete a file outside the models directory"
    );
    assert!(stray.exists());
}

#[tokio::test]
async fn test_a_path_that_names_no_file_is_not_found() {
    // GIVEN a models folder without the file asked for
    let dir = TempDir::new().unwrap();
    installed(&dir, "qwen3-8b.gguf");
    let ghost = apollia_core::config::default_models_dir(dir.path()).join("ghost.gguf");

    // WHEN a client asks for its removal
    let (status, body) = call(state(&dir), "DELETE", &route(&ghost), None).await;

    // THEN the daemon answers 404 with the reason, and the index is untouched
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(
        body["error"]
            .as_str()
            .unwrap()
            .starts_with("model file not found"),
        "{body}"
    );
    assert_eq!(indexed(&dir), 1);
}
