use std::sync::{Arc, Mutex};

use apollia_memory::user_memory::UserMemoryRepository;
use axum::http::StatusCode;
use serde_json::json;
use tempfile::TempDir;

use crate::api::server::AppState;
use crate::api::test_support::{app_state, call, MockBackend, StemLoader};

/// A daemon rooted at `dir`, without its onboarding store.
fn daemon(dir: &TempDir) -> AppState<MockBackend> {
    app_state(
        Arc::new(StemLoader { refusal: None }),
        dir.path().to_path_buf(),
    )
}

/// A daemon rooted at `dir`, whose `user_memory.db` opened.
fn daemon_with_onboarding(dir: &TempDir) -> AppState<MockBackend> {
    let mut state = daemon(dir);
    let repo = UserMemoryRepository::new(&dir.path().join("user_memory.db")).unwrap();
    state.user_memory = Some(Arc::new(Mutex::new(repo)));
    state
}

#[tokio::test]
async fn test_the_machine_the_daemon_runs_on_is_described_over_http() {
    // GIVEN a daemon
    let dir = TempDir::new().unwrap();

    // WHEN the onboarding asks what the machine can run, then posts to the
    // read-only route
    let (status, info) = call(daemon(&dir), "GET", "/api/v1/local-ai/system", None).await;
    let (wrong, _) = call(daemon(&dir), "POST", "/api/v1/local-ai/system", None).await;

    // THEN memory, system and architecture come back, and the write is refused
    assert_eq!(status, StatusCode::OK, "{info}");
    assert!(info["total_ram_gb"].as_f64().unwrap() > 0.0);
    assert_eq!(info["os"], std::env::consts::OS);
    assert_eq!(info["arch"], std::env::consts::ARCH);
    assert_eq!(wrong, StatusCode::METHOD_NOT_ALLOWED);
}

#[tokio::test]
async fn test_a_gguf_file_in_the_daemon_models_folder_is_listed_over_http() {
    // GIVEN a daemon whose models folder holds one GGUF file
    let dir = TempDir::new().unwrap();
    let models = dir.path().join("models");
    std::fs::create_dir_all(&models).unwrap();
    let file = models.join("tiny-Q4_K_M.gguf");
    std::fs::write(&file, vec![0u8; 2048]).unwrap();

    // WHEN the onboarding scans for models, then posts to the read-only route
    let (status, found) = call(daemon(&dir), "GET", "/api/v1/local-ai/models/gguf", None).await;
    let (wrong, _) = call(daemon(&dir), "POST", "/api/v1/local-ai/models/gguf", None).await;

    // THEN the file is in the list with its name and size, and the write is
    // refused
    assert_eq!(status, StatusCode::OK, "{found}");
    let entry = found
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["path"] == file.display().to_string())
        .expect("the daemon's model is listed");
    assert_eq!(entry["filename"], "tiny-Q4_K_M.gguf");
    assert_eq!(entry["size_bytes"], 2048);
    assert_eq!(wrong, StatusCode::METHOD_NOT_ALLOWED);
}

#[tokio::test]
async fn test_a_whisper_file_in_the_daemon_models_folder_is_listed_over_http() {
    // GIVEN a daemon whose models folder holds one Whisper file and one file
    // that is not
    let dir = TempDir::new().unwrap();
    let models = dir.path().join("models");
    std::fs::create_dir_all(&models).unwrap();
    let file = models.join("ggml-small.bin");
    std::fs::write(&file, vec![0u8; 1024]).unwrap();
    std::fs::write(models.join("notes.bin"), b"x").unwrap();

    // WHEN the onboarding scans for dictation models, then posts to the
    // read-only route
    let (status, found) = call(daemon(&dir), "GET", "/api/v1/local-ai/models/whisper", None).await;
    let (wrong, _) = call(
        daemon(&dir),
        "POST",
        "/api/v1/local-ai/models/whisper",
        None,
    )
    .await;

    // THEN the Whisper file is listed with its size variant, the other is
    // not, and the write is refused
    assert_eq!(status, StatusCode::OK, "{found}");
    let found = found.as_array().unwrap();
    let entry = found
        .iter()
        .find(|m| m["path"] == file.display().to_string())
        .expect("the daemon's dictation model is listed");
    assert_eq!(entry["model_size"], "small");
    assert!(!found
        .iter()
        .any(|m| m["path"] == models.join("notes.bin").display().to_string()));
    assert_eq!(wrong, StatusCode::METHOD_NOT_ALLOWED);
}

#[tokio::test]
async fn test_a_gguf_file_chosen_over_http_becomes_the_daemon_local_backend() {
    // GIVEN a daemon, and a GGUF file outside its data directory
    let dir = TempDir::new().unwrap();
    let elsewhere = TempDir::new().unwrap();
    let source = elsewhere.path().join("Qwen3-0.6B-Q8_0.gguf");
    std::fs::write(&source, b"GGUF").unwrap();

    // WHEN the onboarding chooses it with a window
    let body = json!({ "ggufPath": source.display().to_string(), "contextWindow": 16384 });
    let (status, done) = call(daemon(&dir), "POST", "/api/v1/local-ai/llm", Some(body)).await;

    // THEN the daemon's models folder holds it, and its quantisation is named
    assert_eq!(status, StatusCode::OK, "{done}");
    let copied = dir.path().join("models").join("Qwen3-0.6B-Q8_0.gguf");
    assert_eq!(done["model_path"], copied.display().to_string());
    assert_eq!(done["quantization"], "q8_0");
    assert!(copied.exists());
}

#[tokio::test]
async fn test_a_file_that_is_not_there_is_refused_with_its_path() {
    // GIVEN a daemon, and a path no file occupies
    let dir = TempDir::new().unwrap();
    let absent = dir.path().join("absent.gguf").display().to_string();

    // WHEN the onboarding chooses it
    let body = json!({ "ggufPath": absent });
    let (status, refused) = call(daemon(&dir), "POST", "/api/v1/local-ai/llm", Some(body)).await;

    // THEN the request is refused with the words the interface shows
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(refused["error"], format!("file not found: {absent}"));
}

#[tokio::test]
async fn test_a_dictation_model_chosen_over_http_is_stored_in_the_daemon() {
    // GIVEN a daemon whose onboarding store opened
    let dir = TempDir::new().unwrap();

    // WHEN the onboarding chooses a Whisper file in French
    let body = json!({ "modelPath": "/m/ggml-base.bin", "language": "fr" });
    let (status, done) = call(
        daemon_with_onboarding(&dir),
        "POST",
        "/api/v1/local-ai/dictation",
        Some(body),
    )
    .await;

    // THEN the model and the language are stored
    assert_eq!(status, StatusCode::OK, "{done}");
    assert_eq!(done["model_path"], "/m/ggml-base.bin");
    assert_eq!(done["language"], "fr");
    let stt = apollia_core::SttConfigRepository::open(&dir.path().join("system.db")).unwrap();
    assert!(stt.get_or_default().unwrap().enabled);
}

#[tokio::test]
async fn test_a_dictation_model_is_refused_while_the_onboarding_store_is_closed() {
    // GIVEN a daemon whose user_memory.db did not open
    let dir = TempDir::new().unwrap();

    // WHEN the onboarding chooses a Whisper file
    let body = json!({ "modelPath": "/m/ggml-base.bin" });
    let (status, refused) = call(
        daemon(&dir),
        "POST",
        "/api/v1/local-ai/dictation",
        Some(body),
    )
    .await;

    // THEN the daemon says it is unavailable, in the onboarding's words
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(refused["error"], "UserMemory database not initialized");
}
