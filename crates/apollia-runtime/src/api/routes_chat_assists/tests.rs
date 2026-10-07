use std::sync::Arc;

use apollia_llm::LlmRouter;
use axum::http::StatusCode;
use serde_json::json;
use tempfile::TempDir;

use crate::api::server::{shared_llm_router_from, AppState};
use crate::api::test_support::{app_state, call, MockBackend};

fn state_in(dir: &TempDir) -> AppState<MockBackend> {
    app_state(dir.path().to_path_buf())
}

#[tokio::test]
async fn test_a_rewrite_on_a_daemon_without_backend_returns_the_text_and_why() {
    // GIVEN a daemon whose router holds no backend
    let dir = TempDir::new().unwrap();
    let mut state = state_in(&dir);
    state.llm_router = shared_llm_router_from(Some(Arc::new(LlmRouter::empty())));

    // WHEN a client asks for a rewrite
    let (status, reply) = call(
        state,
        "POST",
        "/api/v1/assists/rewrite",
        Some(json!({ "originalText": "fix it" })),
    )
    .await;

    // THEN the original text comes back, flagged as having no backend
    assert_eq!(status, StatusCode::OK, "{reply}");
    assert_eq!(reply["rewrittenText"], "fix it");
    assert_eq!(reply["fallback"], "noBackend");
}

#[tokio::test]
async fn test_a_rewrite_without_text_is_refused() {
    // GIVEN a daemon
    let dir = TempDir::new().unwrap();

    // WHEN a client sends a rewrite request with no text
    let (status, _) = call(
        state_in(&dir),
        "POST",
        "/api/v1/assists/rewrite",
        Some(json!({ "workContext": null })),
    )
    .await;

    // THEN the body is refused before any model is called
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn test_next_steps_on_a_daemon_without_router_come_from_the_heuristic() {
    // GIVEN a daemon with no router mounted
    let dir = TempDir::new().unwrap();

    // WHEN a client asks for the next steps of the dashboard
    let (status, reply) = call(
        state_in(&dir),
        "POST",
        "/api/v1/assists/next-steps",
        Some(json!({ "context": "global_context", "developerMode": false, "facts": {} })),
    )
    .await;

    // THEN the heuristic cards come back, flagged as such
    assert_eq!(status, StatusCode::OK, "{reply}");
    assert_eq!(reply["fromLlm"], false);
    assert!(!reply["steps"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn test_next_steps_of_an_unknown_panel_are_refused() {
    // GIVEN a daemon
    let dir = TempDir::new().unwrap();

    // WHEN a client names a panel the runtime does not know
    let (status, _) = call(
        state_in(&dir),
        "POST",
        "/api/v1/assists/next-steps",
        Some(json!({ "context": "sidebar" })),
    )
    .await;

    // THEN the body is refused
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn test_an_automation_in_words_is_parsed_by_the_daemon() {
    // GIVEN a daemon and one known agent
    let dir = TempDir::new().unwrap();

    // WHEN a client describes a daily automation
    let (status, parsed) = call(
        state_in(&dir),
        "POST",
        "/api/v1/assists/automation",
        Some(json!({
            "input": "every day at 9am summarise my emails",
            "known_agents": ["mail-digest"],
        })),
    )
    .await;

    // THEN a schedule comes back with the confidence the parser gave it
    assert_eq!(status, StatusCode::OK, "{parsed}");
    assert!(parsed["schedule"].is_object(), "{parsed}");
    assert!(parsed["confidence"].is_string(), "{parsed}");
}

#[tokio::test]
async fn test_an_automation_without_input_is_refused() {
    // GIVEN a daemon
    let dir = TempDir::new().unwrap();

    // WHEN a client sends only the known agents
    let (status, _) = call(
        state_in(&dir),
        "POST",
        "/api/v1/assists/automation",
        Some(json!({ "known_agents": [] })),
    )
    .await;

    // THEN the body is refused
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn test_a_known_connector_is_coached_by_the_daemon() {
    // GIVEN a daemon
    let dir = TempDir::new().unwrap();

    // WHEN a client asks for examples for a Notion server with a title
    let (status, cards) = call(
        state_in(&dir),
        "POST",
        "/api/v1/assists/coaching",
        Some(json!({ "serverName": "notion", "serverTitle": "Team Notion" })),
    )
    .await;

    // THEN the Notion cards come back, the first naming the title
    assert_eq!(status, StatusCode::OK, "{cards}");
    let cards = cards.as_array().unwrap();
    assert_eq!(cards.len(), 2);
    assert_eq!(cards[0]["title"], "Summarise my latest Team Notion page");
}

#[tokio::test]
async fn test_coaching_without_a_server_name_is_refused() {
    // GIVEN a daemon
    let dir = TempDir::new().unwrap();

    // WHEN a client sends the snake_case field the route does not read
    let (status, _) = call(
        state_in(&dir),
        "POST",
        "/api/v1/assists/coaching",
        Some(json!({ "server_name": "notion" })),
    )
    .await;

    // THEN the body is refused
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}
