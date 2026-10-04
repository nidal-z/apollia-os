use std::sync::Arc;

use apollia_core::StepBudgetConfig;
use apollia_llm::LlmRouter;
use apollia_tools::ToolRegistryHandle;
use axum::http::StatusCode;
use tempfile::TempDir;

use crate::api::server::AppState;
use crate::api::test_support::{app_state, call, MockBackend, StemLoader};
use crate::chat::manager::ChatSessionManagerHandle;
use crate::chat::ChatToolsConfig;

/// A daemon state whose chat manager runs over `dir`.
fn state_with_chat(dir: &TempDir) -> AppState<MockBackend> {
    let loader = Arc::new(StemLoader { refusal: None });
    let mut state = app_state(loader.clone(), dir.path().to_path_buf());
    let tools_config = Arc::new(ChatToolsConfig {
        data_dir: dir.path().to_path_buf(),
        brave_api_key: None,
        tools_config: apollia_core::ToolsConfig::default(),
        tool_turn_temperature: None,
        source_registry: None,
    });
    let manager = ChatSessionManagerHandle::spawn(
        &dir.path().join("chat.db"),
        Some(Arc::new(LlmRouter::empty())),
        ToolRegistryHandle::start(),
        loader,
        None,
        state.event_sender.clone(),
        StepBudgetConfig::default(),
        None,
        state.registry_handle.clone(),
        None,
        None,
        Some(tools_config),
        apollia_mcp::session::LoadingMode::Eager,
        20,
        None,
        false,
    )
    .unwrap();
    state.chat_manager = Some(manager);
    state
}

async fn new_free_chat(state: &AppState<MockBackend>) -> String {
    let (status, info) = call(
        state.clone(),
        "POST",
        "/api/v1/sessions",
        Some(serde_json::json!({ "mode": "libre" })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{info}");
    info["id"].as_str().unwrap().to_owned()
}

#[tokio::test]
async fn test_the_metrics_of_a_new_session_read_as_an_empty_panel() {
    // GIVEN a daemon holding a free chat where no exchange has completed
    let dir = TempDir::new().unwrap();
    let state = state_with_chat(&dir);
    let id = new_free_chat(&state).await;

    // WHEN a client reads the metrics of the session
    let (status, metrics) = call(
        state,
        "GET",
        &format!("/api/v1/sessions/{id}/metrics"),
        None,
    )
    .await;

    // THEN the record is the zero-filled one the embedded desktop answers
    assert_eq!(status, StatusCode::OK, "{metrics}");
    assert_eq!(metrics["session_id"], id.as_str());
    assert_eq!(metrics["exchanges_count"], 0);
    assert_eq!(metrics["prompt_tokens"], 0);
    assert_eq!(metrics["tool_stats"], serde_json::json!([]));
}

#[tokio::test]
async fn test_the_metrics_are_refused_without_a_chat_subsystem() {
    // GIVEN a daemon whose chat subsystem did not start
    let dir = TempDir::new().unwrap();
    let state = app_state(Arc::new(StemLoader { refusal: None }), dir.path().into());

    // WHEN a client reads the metrics of a session
    let (status, body) = call(state, "GET", "/api/v1/sessions/s-1/metrics", None).await;

    // THEN the refusal says why, in the words of the embedded desktop
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["error"], "chat subsystem not available");
}

/// Grant the Apollia agent the folder at `path`, stored in `dir`, to read.
fn grant_apollia_a_folder(dir: &TempDir, path: &std::path::Path) -> String {
    use apollia_core::paths::DataFile;
    let folder = apollia_core::FolderConnectionStore::open(&DataFile::System.path(dir.path()))
        .unwrap()
        .add(&path.to_string_lossy(), None)
        .unwrap();
    apollia_tools::AgentRepository::open(&DataFile::Agents.path(dir.path()))
        .unwrap()
        .set_override(
            apollia_tools::APOLLIA_AGENT_ID,
            apollia_tools::OverrideField::Access,
            &serde_json::json!([format!("conn:folder:{}/file_read", folder.id)]),
        )
        .unwrap();
    folder.path
}

#[tokio::test]
async fn test_the_workspace_of_a_free_chat_is_the_folder_granted_to_apollia() {
    // GIVEN a daemon whose Apollia agent is granted one folder
    let dir = TempDir::new().unwrap();
    let granted = TempDir::new().unwrap();
    let state = state_with_chat(&dir);
    let expected = grant_apollia_a_folder(&dir, granted.path());
    let id = new_free_chat(&state).await;

    // WHEN a client reads the working directory of the session, and of a
    // session the daemon does not know
    let (status, known) = call(
        state.clone(),
        "GET",
        &format!("/api/v1/sessions/{id}/workspace"),
        None,
    )
    .await;
    let (unknown_status, unknown) =
        call(state, "GET", "/api/v1/sessions/unknown/workspace", None).await;

    // THEN the first is the granted folder, the second resolves to nothing
    assert_eq!(status, StatusCode::OK, "{known}");
    assert_eq!(known["workspace"], expected.as_str());
    assert_eq!(unknown_status, StatusCode::OK, "{unknown}");
    assert_eq!(unknown["workspace"], serde_json::Value::Null);
}

#[tokio::test]
async fn test_a_free_chat_whose_agent_has_no_folder_has_no_workspace() {
    // GIVEN a daemon whose Apollia agent is granted no folder
    let dir = TempDir::new().unwrap();
    let state = state_with_chat(&dir);
    let id = new_free_chat(&state).await;

    // WHEN a client reads the working directory of the session
    let (status, body) = call(
        state,
        "GET",
        &format!("/api/v1/sessions/{id}/workspace"),
        None,
    )
    .await;

    // THEN there is none: neither the data directory nor the home folder
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["workspace"], serde_json::Value::Null);
}

#[tokio::test]
async fn test_the_workspace_is_refused_without_a_chat_subsystem() {
    // GIVEN a daemon whose chat subsystem did not start
    let dir = TempDir::new().unwrap();
    let state = app_state(Arc::new(StemLoader { refusal: None }), dir.path().into());

    // WHEN a client reads the working directory of a session
    let (status, body) = call(state, "GET", "/api/v1/sessions/s-1/workspace", None).await;

    // THEN the refusal says why
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["error"], "chat subsystem not available");
}

/// `state` given a backend table whose default row serves a Qwen3 GGUF the
/// index records as a reasoning switch that reads images.
fn with_default_qwen3(mut state: AppState<MockBackend>, dir: &TempDir) -> AppState<MockBackend> {
    use apollia_core::{LlmBackendConfig, LlmBackendRepository, LlmProvider};
    use apollia_llm::model_index::{
        IndexedModel, ModelCategory, ModelFormat, ModelIndex, ModelOrigin, INDEX_FILE_NAME,
    };
    use apollia_llm::{Capabilities, VisionEngine};

    let models = crate::installed_models::models_dir(dir.path());
    std::fs::create_dir_all(&models).unwrap();
    let file = models.join("qwen3.gguf");
    ModelIndex {
        models: vec![IndexedModel {
            id: "qwen3".into(),
            format: ModelFormat::Gguf,
            path: file.clone(),
            parts: Vec::new(),
            annexes: Vec::new(),
            size_bytes: 0,
            origin: ModelOrigin::Import,
            sha256: None,
            origin_verified: false,
            category: ModelCategory::Language,
            parameters: None,
            modified_at: None,
            pinned_at: None,
            category_override: None,
            source_file_override: None,
            capabilities: Some(
                Capabilities::from_chat_template(Some("{{ enable_thinking }}"))
                    .with_vision(VisionEngine::LlamaServer),
            ),
        }],
        ..ModelIndex::default()
    }
    .save(&models.join(INDEX_FILE_NAME))
    .unwrap();
    let repo = LlmBackendRepository::open(&dir.path().join("system.db")).unwrap();
    repo.save(&LlmBackendConfig {
        name: "local-qwen".into(),
        provider: LlmProvider::LlamaCpp,
        model: file.display().to_string(),
        config_json: serde_json::json!({}),
        is_default: true,
        enabled: true,
    })
    .unwrap();
    state.llm_backend_repo = Some(Arc::new(std::sync::Mutex::new(repo)));
    state
}

#[tokio::test]
async fn test_the_model_capabilities_of_a_free_chat_are_those_of_the_default_model() {
    // GIVEN a daemon whose default model is a local Qwen3 that reasons on a
    // switch and reads images, and a free chat naming no model
    let dir = TempDir::new().unwrap();
    let state = with_default_qwen3(state_with_chat(&dir), &dir);
    let id = new_free_chat(&state).await;

    // WHEN a client reads what the model of the session can do
    let (status, body) = call(
        state,
        "GET",
        &format!("/api/v1/sessions/{id}/model-capabilities"),
        None,
    )
    .await;

    // THEN the default model answers: a reasoning switch, and images in the
    // formats its engine decodes
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["model"], "local-qwen");
    assert_eq!(body["local"], true);
    assert_eq!(body["reasoning_profile"], "toggle");
    assert_eq!(body["vision"], true);
    assert!(body["image_formats"]
        .as_array()
        .is_some_and(|f| f.iter().any(|v| v == "png")));
}

#[tokio::test]
async fn test_the_model_capabilities_of_an_unknown_session_are_not_found() {
    // GIVEN a daemon with a chat subsystem and a backend table
    let dir = TempDir::new().unwrap();
    let state = with_default_qwen3(state_with_chat(&dir), &dir);

    // WHEN a client reads the model capabilities of a session nobody created
    let (status, body) = call(
        state,
        "GET",
        "/api/v1/sessions/unknown/model-capabilities",
        None,
    )
    .await;

    // THEN the session is not found
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
}

fn base64_of(bytes: &[u8]) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

#[tokio::test]
async fn test_a_message_with_images_is_accepted_and_its_images_read_back() {
    // GIVEN a daemon whose default model reads images, and a free chat
    let dir = TempDir::new().unwrap();
    let state = with_default_qwen3(state_with_chat(&dir), &dir);
    let id = new_free_chat(&state).await;

    // WHEN a client sends a message with two images
    let (status, body) = call(
        state.clone(),
        "POST",
        &format!("/api/v1/sessions/{id}/messages"),
        Some(serde_json::json!({
            "content": "compare them",
            "images": [
                {"mime_type": "image/png", "data": base64_of(&[1, 2, 3])},
                {"mime_type": "image/jpeg", "data": base64_of(&[4, 5])},
            ],
        })),
    )
    .await;

    // THEN it is accepted, the session lists both images in the message
    // metadata, and each reads back with its bytes
    assert_eq!(status, StatusCode::ACCEPTED, "{body}");
    let (_, detail) = call(
        state.clone(),
        "GET",
        &format!("/api/v1/sessions/{id}"),
        None,
    )
    .await;
    let images = detail["session"]["history"][0]["metadata"]["images"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert_eq!(images.len(), 2, "{detail}");
    let file = images[0]["file"].as_str().unwrap();
    let (status, image) = call(
        state,
        "GET",
        &format!("/api/v1/sessions/{id}/images/{file}"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{image}");
    assert_eq!(image["mime_type"], "image/png");
    assert_eq!(image["data"], base64_of(&[1, 2, 3]));
}

#[tokio::test]
async fn test_a_message_with_an_image_above_the_default_body_limit_is_accepted() {
    // GIVEN a daemon whose default model reads images, and a free chat
    let dir = TempDir::new().unwrap();
    let state = with_default_qwen3(state_with_chat(&dir), &dir);
    let id = new_free_chat(&state).await;

    // WHEN a client sends one image of 3 MiB, above axum's 2 MiB default
    let (status, body) = call(
        state,
        "POST",
        &format!("/api/v1/sessions/{id}/messages"),
        Some(serde_json::json!({
            "content": "what is this?",
            "images": [{"mime_type": "image/png", "data": base64_of(&vec![7u8; 3 * 1024 * 1024])}],
        })),
    )
    .await;

    // THEN the route takes it: its body limit follows the image ceilings
    assert_eq!(status, StatusCode::ACCEPTED, "{body}");
}

#[tokio::test]
async fn test_a_message_with_images_to_a_model_without_vision_is_refused() {
    // GIVEN a daemon with no model that reads images, and a free chat
    let dir = TempDir::new().unwrap();
    let state = state_with_chat(&dir);
    let id = new_free_chat(&state).await;

    // WHEN a client sends a message with an image anyway
    let (status, body) = call(
        state,
        "POST",
        &format!("/api/v1/sessions/{id}/messages"),
        Some(serde_json::json!({
            "content": "what is this?",
            "images": [{"mime_type": "image/png", "data": base64_of(&[1])}],
        })),
    )
    .await;

    // THEN the runtime refuses it and says why
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert!(body["error"]
        .as_str()
        .is_some_and(|e| e.contains("does not read images")));
}

#[tokio::test]
async fn test_an_image_name_leaving_the_conversation_folder_is_refused() {
    // GIVEN a daemon holding a free chat
    let dir = TempDir::new().unwrap();
    let state = state_with_chat(&dir);
    let id = new_free_chat(&state).await;

    // WHEN a client asks for a hidden file name
    let (status, _) = call(
        state,
        "GET",
        &format!("/api/v1/sessions/{id}/images/..secret.png"),
        None,
    )
    .await;

    // THEN the name is refused
    assert_eq!(status, StatusCode::BAD_REQUEST);
}
