use std::sync::{Arc, Mutex};

use apollia_core::StepBudgetConfig;
use apollia_llm::LlmRouter;
use apollia_memory::user_memory::UserMemoryRepository;
use apollia_tools::ToolRegistryHandle;
use axum::http::StatusCode;
use serde_json::json;
use tempfile::TempDir;

use crate::api::server::AppState;
use crate::api::test_support::{app_state, call, manifest, MockBackend};
use crate::chat::manager::ChatSessionManagerHandle;

/// A daemon state whose user memory is open over `dir`, the chat absent.
fn state_with_memory(dir: &TempDir) -> AppState<MockBackend> {
    let mut state = app_state(dir.path().to_path_buf());
    let repo = UserMemoryRepository::new(&dir.path().join("user_memory.db")).unwrap();
    state.user_memory = Some(Arc::new(Mutex::new(repo)));
    state
}

/// The same state with a chat manager over `dir`.
fn state_with_chat(dir: &TempDir) -> AppState<MockBackend> {
    let mut state = state_with_memory(dir);
    let manager = ChatSessionManagerHandle::spawn(
        &dir.path().join("chat.db"),
        Some(Arc::new(LlmRouter::empty())),
        ToolRegistryHandle::start(),
        None,
        state.event_sender.clone(),
        StepBudgetConfig::default(),
        None,
        state.registry_handle.clone(),
        None,
        None,
        None,
        apollia_mcp::session::LoadingMode::Eager,
        20,
        None,
        false,
    )
    .unwrap();
    state.chat_manager = Some(manager);
    state
}

#[tokio::test]
async fn test_the_preference_put_over_http_is_read_back() {
    // GIVEN a daemon whose user memory is open and holds no preference
    let dir = TempDir::new().unwrap();
    let state = state_with_memory(&dir);

    // WHEN a client reads it, turns the companion on, and reads it again
    let (first, before) = call(state.clone(), "GET", "/api/v1/companion/enabled", None).await;
    let (put, stored) = call(
        state.clone(),
        "PUT",
        "/api/v1/companion/enabled",
        Some(json!({ "enabled": true })),
    )
    .await;
    let (second, after) = call(state, "GET", "/api/v1/companion/enabled", None).await;

    // THEN it reads off, then on as stored
    assert_eq!(first, StatusCode::OK, "{before}");
    assert_eq!(before, json!({ "enabled": false }));
    assert_eq!(put, StatusCode::OK, "{stored}");
    assert_eq!(stored, json!({ "enabled": true }));
    assert_eq!(second, StatusCode::OK);
    assert_eq!(after, json!({ "enabled": true }));
}

#[tokio::test]
async fn test_a_daemon_without_user_memory_refuses_the_preference_with_its_words() {
    // GIVEN a daemon whose user memory did not open
    let dir = TempDir::new().unwrap();
    let state = app_state(dir.path().to_path_buf());

    // WHEN a client reads and writes the preference
    let (read, refused) = call(state.clone(), "GET", "/api/v1/companion/enabled", None).await;
    let (write, _) = call(
        state,
        "PUT",
        "/api/v1/companion/enabled",
        Some(json!({ "enabled": false })),
    )
    .await;

    // THEN both answer 503 with the message the embedded desktop shows
    assert_eq!(read, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(write, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(refused["error"], "UserMemory database not initialized");
}

#[tokio::test]
async fn test_a_session_opens_on_the_guide_and_is_recorded_for_the_onboarding() {
    // GIVEN a daemon whose chat runs and whose registry holds the guide
    let dir = TempDir::new().unwrap();
    let state = state_with_chat(&dir);
    state
        .registry_handle
        .register(manifest(crate::companion::GUIDE_AGENT))
        .await
        .unwrap();

    // WHEN a client opens a companion session from the agents page
    let (status, opened) = call(
        state.clone(),
        "POST",
        "/api/v1/companion/sessions",
        Some(json!({ "context": "agents" })),
    )
    .await;

    // THEN a guide session is created in Agent mode, and its identifier is
    // where the onboarding state reads it
    assert_eq!(status, StatusCode::CREATED, "{opened}");
    let id = opened["session_id"].as_str().unwrap().to_owned();
    let (_, session) = call(
        state.clone(),
        "GET",
        &format!("/api/v1/sessions/{id}"),
        None,
    )
    .await;
    assert_eq!(
        session["session"]["agent_name"],
        crate::companion::GUIDE_AGENT,
        "{session}"
    );
    assert_eq!(session["session"]["mode"], "Agent");
    let recorded = state
        .user_memory
        .as_ref()
        .unwrap()
        .lock()
        .unwrap()
        .get_internal("onboarding_companion_session_id")
        .unwrap();
    assert_eq!(recorded.as_deref(), Some(id.as_str()));
}

#[tokio::test]
async fn test_a_session_without_the_guide_or_the_chat_is_refused_with_its_words() {
    // GIVEN a daemon whose chat runs without the guide, and one without chat
    let dir = TempDir::new().unwrap();
    let without_guide = state_with_chat(&dir);
    let other = TempDir::new().unwrap();
    let without_chat = state_with_memory(&other);

    // WHEN a client opens a companion session on each
    let (missing, refused) = call(
        without_guide,
        "POST",
        "/api/v1/companion/sessions",
        Some(json!({})),
    )
    .await;
    let (down, unavailable) = call(
        without_chat,
        "POST",
        "/api/v1/companion/sessions",
        Some(json!({ "context": null })),
    )
    .await;

    // THEN the first is 404 with the install hint the panel recognises, the
    // second 503 naming the chat
    assert_eq!(missing, StatusCode::NOT_FOUND);
    assert!(
        refused["error"]
            .as_str()
            .unwrap()
            .starts_with("apollia-guide agent not found"),
        "{refused}"
    );
    assert_eq!(down, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(unavailable["error"], "chat subsystem not available");
}
