use std::sync::{Arc, Mutex};

use apollia_core::StepBudgetConfig;
use apollia_llm::LlmRouter;
use apollia_memory::user_memory::UserMemoryRepository;
use apollia_tools::ToolRegistryHandle;
use axum::http::StatusCode;
use tempfile::TempDir;

use crate::api::server::AppState;
use crate::api::test_support::{app_state, call, manifest, MockBackend};
use crate::chat::manager::ChatSessionManagerHandle;

/// A daemon state whose user memory is open over `dir`.
fn state_with_memory(dir: &TempDir) -> AppState<MockBackend> {
    let mut state = app_state(dir.path().to_path_buf());
    let repo = UserMemoryRepository::new(&dir.path().join("user_memory.db")).unwrap();
    state.user_memory = Some(Arc::new(Mutex::new(repo)));
    state
}

/// The same state, with a chat manager running over `dir`.
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
async fn test_a_first_launch_reads_as_welcome_and_the_phases_advance_in_order() {
    // GIVEN a daemon whose user memory holds no onboarding yet
    let dir = TempDir::new().unwrap();
    let state = state_with_memory(&dir);

    // WHEN a client reads the state, then moves to the second phase
    let (read, initial) = call(state.clone(), "GET", "/api/v1/onboarding", None).await;
    let (moved, advanced) = call(
        state.clone(),
        "POST",
        "/api/v1/onboarding/phase",
        Some(serde_json::json!({ "phase": "ai_setup" })),
    )
    .await;
    let (_, reread) = call(state, "GET", "/api/v1/onboarding", None).await;

    // THEN the state starts at welcome, moves straight to the AI setup with no
    // usage profile in between, and reads back as stored
    assert_eq!(read, StatusCode::OK, "{initial}");
    assert_eq!(initial["phase"], "welcome");
    assert!(initial.get("profile").is_none(), "{initial}");
    assert_eq!(moved, StatusCode::OK, "{advanced}");
    assert_eq!(advanced["phase"], "ai_setup");
    assert!(advanced["started_at"].is_string());
    assert_eq!(reread["phase"], "ai_setup");
}

#[tokio::test]
async fn test_a_phase_out_of_sequence_an_unknown_phase_and_the_profile_route_are_refused() {
    // GIVEN a daemon at the welcome phase
    let dir = TempDir::new().unwrap();
    let state = state_with_memory(&dir);

    // WHEN a client jumps to the last phase, names an unknown phase, the
    // removed profile phase, and the removed profile route
    let (jump, jumped) = call(
        state.clone(),
        "POST",
        "/api/v1/onboarding/phase",
        Some(serde_json::json!({ "phase": "done" })),
    )
    .await;
    let (unknown, unknown_body) = call(
        state.clone(),
        "POST",
        "/api/v1/onboarding/phase",
        Some(serde_json::json!({ "phase": "nowhere" })),
    )
    .await;
    let (removed_phase, _) = call(
        state.clone(),
        "POST",
        "/api/v1/onboarding/phase",
        Some(serde_json::json!({ "phase": "profile_choice" })),
    )
    .await;
    let (profile, _) = call(
        state,
        "PUT",
        "/api/v1/onboarding/profile",
        Some(serde_json::json!({ "profile": "operator" })),
    )
    .await;

    // THEN each is refused with its status and the words the desktop shows
    assert_eq!(jump, StatusCode::CONFLICT, "{jumped}");
    assert!(jumped["error"].as_str().unwrap().contains("cannot advance"));
    assert_eq!(unknown, StatusCode::BAD_REQUEST, "{unknown_body}");
    assert_eq!(
        unknown_body["error"],
        "persistence error: unknown phase: nowhere"
    );
    assert_eq!(removed_phase, StatusCode::BAD_REQUEST);
    assert!(
        matches!(
            profile,
            StatusCode::NOT_FOUND | StatusCode::METHOD_NOT_ALLOWED
        ),
        "{profile}"
    );
}

#[tokio::test]
async fn test_without_a_user_memory_the_state_and_the_dismissal_are_unavailable() {
    // GIVEN a daemon whose user memory did not open
    let dir = TempDir::new().unwrap();
    let state = app_state(dir.path().to_path_buf());

    // WHEN a client reads the state and dismisses the onboarding
    let (read, read_body) = call(state.clone(), "GET", "/api/v1/onboarding", None).await;
    let (dismiss, _) = call(state, "POST", "/api/v1/onboarding/dismiss", None).await;

    // THEN both answer that the service cannot serve them yet
    assert_eq!(read, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(read_body["error"], "UserMemory database not initialized");
    assert_eq!(dismiss, StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn test_a_dismissal_reads_back_as_skipped() {
    // GIVEN a daemon at the welcome phase
    let dir = TempDir::new().unwrap();
    let state = state_with_memory(&dir);

    // WHEN a client dismisses the onboarding
    let (status, body) = call(state.clone(), "POST", "/api/v1/onboarding/dismiss", None).await;

    // THEN the dismissal is acknowledged and the user memory holds the mark
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["dismissed"], true);
    let repo = state.user_memory.as_ref().unwrap().lock().unwrap();
    assert_eq!(
        repo.get_internal("onboarding_skipped").unwrap().as_deref(),
        Some("true")
    );
}

#[tokio::test]
async fn test_the_completion_is_absent_then_recorded_in_the_daemon_memory() {
    // GIVEN a daemon whose onboarding agent never ran
    let dir = TempDir::new().unwrap();
    let state = state_with_memory(&dir);

    // WHEN a client asks whether it finished, finalizes, and asks again
    let (_, before) = call(state.clone(), "GET", "/api/v1/onboarding/finalized", None).await;
    let (status, finalized) =
        call(state.clone(), "POST", "/api/v1/onboarding/finalize", None).await;
    let (_, after) = call(state, "GET", "/api/v1/onboarding/finalized", None).await;

    // THEN the completion lands in the memory directory of the daemon
    assert_eq!(before["finalized"], false);
    assert_eq!(status, StatusCode::OK, "{finalized}");
    assert_eq!(after["finalized"], true);
    assert!(dir.path().join("memory").join("onboarding.db").exists());
}

#[tokio::test]
async fn test_a_finalization_that_cannot_write_is_refused() {
    // GIVEN a daemon whose memory directory is a file, so nothing can be
    // created under it
    let dir = TempDir::new().unwrap();
    std::fs::write(dir.path().join("memory"), b"not a directory").unwrap();
    let state = state_with_memory(&dir);

    // WHEN a client finalizes the conversation
    let (status, body) = call(state, "POST", "/api/v1/onboarding/finalize", None).await;

    // THEN the refusal says why
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{body}");
    assert!(body["error"]
        .as_str()
        .unwrap()
        .starts_with("cannot create memory dir"));
}

#[tokio::test]
async fn test_an_onboarding_conversation_opens_and_resumes_with_the_agent_installed() {
    // GIVEN a daemon whose chat runs and whose onboarding agent is registered
    let dir = TempDir::new().unwrap();
    let state = state_with_chat(&dir);
    state
        .registry_handle
        .register(manifest("onboarding-agent"))
        .await
        .unwrap();

    // WHEN a client starts a full onboarding, then resumes it
    let (started, trigger) = call(
        state.clone(),
        "POST",
        "/api/v1/onboarding/sessions",
        Some(serde_json::json!({})),
    )
    .await;
    let (resumed, resume) = call(state, "POST", "/api/v1/onboarding/resume", None).await;

    // THEN each opens its own conversation in its mode
    assert_eq!(started, StatusCode::OK, "{trigger}");
    assert_eq!(trigger["mode"], "full");
    assert!(trigger["session_id"].is_string());
    assert_eq!(resumed, StatusCode::OK, "{resume}");
    assert_eq!(resume["mode"], "resume");
    assert_ne!(trigger["session_id"], resume["session_id"]);
}

#[tokio::test]
async fn test_an_onboarding_without_its_agent_or_with_a_bad_topic_is_refused() {
    // GIVEN a daemon whose chat runs but whose onboarding agent is absent
    let dir = TempDir::new().unwrap();
    let state = state_with_chat(&dir);

    // WHEN a client starts the onboarding, and one topic that does not exist
    let (missing, missing_body) = call(
        state.clone(),
        "POST",
        "/api/v1/onboarding/sessions",
        Some(serde_json::json!({})),
    )
    .await;
    let (topic, topic_body) = call(
        state.clone(),
        "POST",
        "/api/v1/onboarding/sessions",
        Some(serde_json::json!({ "topic": "weather" })),
    )
    .await;
    let (resume, _) = call(state, "POST", "/api/v1/onboarding/resume", None).await;

    // THEN the agent is reported missing and the topic is refused by name
    assert_eq!(missing, StatusCode::NOT_FOUND, "{missing_body}");
    assert!(missing_body["error"]
        .as_str()
        .unwrap()
        .starts_with("onboarding-agent is not installed"));
    assert_eq!(topic, StatusCode::BAD_REQUEST, "{topic_body}");
    assert!(topic_body["error"].as_str().unwrap().contains("weather"));
    assert_eq!(resume, StatusCode::NOT_FOUND);
}
