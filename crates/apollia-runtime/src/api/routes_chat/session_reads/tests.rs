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

/// A daemon state whose chat manager runs over `dir`, free chats working in
/// `dir/workspace`.
fn state_with_chat(dir: &TempDir) -> AppState<MockBackend> {
    let loader = Arc::new(StemLoader { refusal: None });
    let mut state = app_state(loader.clone(), dir.path().to_path_buf());
    let workspace = dir.path().join("workspace");
    std::fs::create_dir_all(&workspace).unwrap();
    let tools_config = Arc::new(ChatToolsConfig {
        data_dir: dir.path().to_path_buf(),
        brave_api_key: None,
        tools_config: apollia_core::ToolsConfig::default(),
        default_workspace: Some(workspace),
        tool_turn_temperature: None,
        trusted_paths: Vec::new(),
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

#[tokio::test]
async fn test_the_workspace_of_a_free_chat_is_the_default_one() {
    // GIVEN a daemon whose free chats work in a configured folder
    let dir = TempDir::new().unwrap();
    let state = state_with_chat(&dir);
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

    // THEN the first is the configured folder, the second resolves to nothing
    assert_eq!(status, StatusCode::OK, "{known}");
    let expected = dir.path().join("workspace");
    assert_eq!(known["workspace"], expected.to_string_lossy().as_ref());
    assert_eq!(unknown_status, StatusCode::OK, "{unknown}");
    assert_eq!(unknown["workspace"], serde_json::Value::Null);
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
