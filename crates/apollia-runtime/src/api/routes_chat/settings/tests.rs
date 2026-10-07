use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{json, Value};
use tower::ServiceExt;

use super::super::tests::chat_state;
use crate::api::server::APIServer;

async fn call(
    router: &axum::Router,
    method: &str,
    uri: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut request = Request::builder().method(method).uri(uri);
    let payload = match body {
        Some(body) => {
            request = request.header("content-type", "application/json");
            Body::from(serde_json::to_vec(&body).unwrap())
        }
        None => Body::empty(),
    };
    let response = router
        .clone()
        .oneshot(request.body(payload).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

async fn new_session(router: &axum::Router) -> String {
    let (status, created) = call(
        router,
        "POST",
        "/api/v1/sessions",
        Some(json!({ "mode": "libre" })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    created["id"].as_str().unwrap().to_owned()
}

#[tokio::test]
async fn test_settings_are_written_and_read_back_for_one_conversation() {
    // GIVEN two conversations
    let dir = tempfile::tempdir().unwrap();
    let router = APIServer::build_router_for_test(chat_state(&dir));
    let tuned = new_session(&router).await;
    let other = new_session(&router).await;

    // WHEN the settings of the first are written
    let (written, stored) = call(
        &router,
        "PUT",
        &format!("/api/v1/sessions/{tuned}/settings"),
        Some(json!({
            "temperature": 0.2,
            "max_tokens": 512,
            "context_overflow": "sliding_window",
            "instructions": "  Answer in one sentence.  ",
            "disabled_accesses": ["web_fetch", "web_fetch"],
        })),
    )
    .await;
    let (read, settings) = call(
        &router,
        "GET",
        &format!("/api/v1/sessions/{tuned}/settings"),
        None,
    )
    .await;
    let (_, untouched) = call(
        &router,
        "GET",
        &format!("/api/v1/sessions/{other}/settings"),
        None,
    )
    .await;

    // THEN they come back as stored, tidied, and the other conversation
    // still inherits everything
    assert_eq!(written, StatusCode::OK, "{stored}");
    assert_eq!(read, StatusCode::OK);
    assert_eq!(settings, stored);
    assert_eq!(settings["max_tokens"], 512);
    assert_eq!(settings["context_overflow"], "sliding_window");
    assert_eq!(settings["instructions"], "Answer in one sentence.");
    assert_eq!(settings["disabled_accesses"], json!(["web_fetch"]));
    assert_eq!(untouched, json!({}));
}

#[tokio::test]
async fn test_settings_refuse_an_unknown_field_and_a_value_out_of_range() {
    // GIVEN a conversation
    let dir = tempfile::tempdir().unwrap();
    let router = APIServer::build_router_for_test(chat_state(&dir));
    let session = new_session(&router).await;
    let uri = format!("/api/v1/sessions/{session}/settings");

    // WHEN a field nobody declared is sent, then a temperature of 5, then
    // instructions over the cap
    let (unknown, unknown_body) = call(
        &router,
        "PUT",
        &uri,
        Some(json!({ "system_prompt": "You are someone else." })),
    )
    .await;
    let (hot, _) = call(&router, "PUT", &uri, Some(json!({ "temperature": 5.0 }))).await;
    let (long, _) = call(
        &router,
        "PUT",
        &uri,
        Some(json!({ "instructions": "a".repeat(4001) })),
    )
    .await;
    let (_, settings) = call(&router, "GET", &uri, None).await;

    // THEN each is refused, the first by the name of the field, and nothing
    // was stored
    assert_eq!(unknown, StatusCode::BAD_REQUEST);
    assert!(
        unknown_body["error"]
            .as_str()
            .unwrap()
            .contains("system_prompt"),
        "{unknown_body}"
    );
    assert_eq!(hot, StatusCode::BAD_REQUEST);
    assert_eq!(long, StatusCode::BAD_REQUEST);
    assert_eq!(settings, json!({}));
}

#[tokio::test]
async fn test_notes_are_written_and_read_back() {
    // GIVEN a conversation
    let dir = tempfile::tempdir().unwrap();
    let router = APIServer::build_router_for_test(chat_state(&dir));
    let session = new_session(&router).await;
    let uri = format!("/api/v1/sessions/{session}/notes");

    // WHEN its notes are read, written, then read again
    let (_, empty) = call(&router, "GET", &uri, None).await;
    let (written, _) = call(
        &router,
        "PUT",
        &uri,
        Some(json!({ "notes": "Ask for the March figures.\n" })),
    )
    .await;
    let (_, notes) = call(&router, "GET", &uri, None).await;
    let (too_long, _) = call(
        &router,
        "PUT",
        &uri,
        Some(json!({ "notes": "n".repeat(20_001) })),
    )
    .await;

    // THEN they are born empty, kept as typed, and refused beyond their cap
    assert_eq!(empty, json!({ "notes": "" }));
    assert_eq!(written, StatusCode::OK);
    assert_eq!(notes, json!({ "notes": "Ask for the March figures.\n" }));
    assert_eq!(too_long, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_a_closed_conversation_is_read_and_no_longer_written() {
    // GIVEN a conversation with settings and notes, then closed
    let dir = tempfile::tempdir().unwrap();
    let router = APIServer::build_router_for_test(chat_state(&dir));
    let session = new_session(&router).await;
    let settings_uri = format!("/api/v1/sessions/{session}/settings");
    let notes_uri = format!("/api/v1/sessions/{session}/notes");
    call(
        &router,
        "PUT",
        &settings_uri,
        Some(json!({ "max_tokens": 64 })),
    )
    .await;
    call(&router, "PUT", &notes_uri, Some(json!({ "notes": "kept" }))).await;
    let (closed, _) = call(
        &router,
        "DELETE",
        &format!("/api/v1/sessions/{session}"),
        None,
    )
    .await;
    assert_eq!(closed, StatusCode::OK);

    // WHEN its settings and notes are written, then read
    let (settings_write, _) = call(
        &router,
        "PUT",
        &settings_uri,
        Some(json!({ "max_tokens": 8 })),
    )
    .await;
    let (notes_write, _) = call(&router, "PUT", &notes_uri, Some(json!({ "notes": "new" }))).await;
    let (_, settings) = call(&router, "GET", &settings_uri, None).await;
    let (_, notes) = call(&router, "GET", &notes_uri, None).await;

    // THEN the writes are refused as a conflict and what was stored is read
    assert_eq!(settings_write, StatusCode::CONFLICT);
    assert_eq!(notes_write, StatusCode::CONFLICT);
    assert_eq!(settings, json!({ "max_tokens": 64 }));
    assert_eq!(notes, json!({ "notes": "kept" }));
}

#[tokio::test]
async fn test_an_unknown_conversation_answers_not_found() {
    // GIVEN a runtime with no conversation
    let dir = tempfile::tempdir().unwrap();
    let router = APIServer::build_router_for_test(chat_state(&dir));

    // WHEN the settings and the notes of a conversation nobody created are
    // read and written
    let (read, _) = call(&router, "GET", "/api/v1/sessions/ghost/settings", None).await;
    let (write, _) = call(
        &router,
        "PUT",
        "/api/v1/sessions/ghost/settings",
        Some(json!({})),
    )
    .await;
    let (notes, _) = call(
        &router,
        "PUT",
        "/api/v1/sessions/ghost/notes",
        Some(json!({ "notes": "x" })),
    )
    .await;

    // THEN each answers 404
    assert_eq!(read, StatusCode::NOT_FOUND);
    assert_eq!(write, StatusCode::NOT_FOUND);
    assert_eq!(notes, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_a_fork_starts_from_the_settings_of_its_parent_and_from_empty_notes() {
    // GIVEN a conversation with settings and notes
    let dir = tempfile::tempdir().unwrap();
    let router = APIServer::build_router_for_test(chat_state(&dir));
    let parent = new_session(&router).await;
    call(
        &router,
        "PUT",
        &format!("/api/v1/sessions/{parent}/settings"),
        Some(json!({ "temperature": 0.1, "instructions": "Be brief." })),
    )
    .await;
    call(
        &router,
        "PUT",
        &format!("/api/v1/sessions/{parent}/notes"),
        Some(json!({ "notes": "about the parent" })),
    )
    .await;

    // WHEN it is forked
    let (forked, child) = call(
        &router,
        "POST",
        &format!("/api/v1/sessions/{parent}/fork"),
        Some(json!({})),
    )
    .await;
    assert_eq!(forked, StatusCode::CREATED, "{child}");
    let child = child["id"].as_str().unwrap().to_owned();
    let (_, settings) = call(
        &router,
        "GET",
        &format!("/api/v1/sessions/{child}/settings"),
        None,
    )
    .await;
    let (_, notes) = call(
        &router,
        "GET",
        &format!("/api/v1/sessions/{child}/notes"),
        None,
    )
    .await;

    // THEN the child carries the settings and none of the notes
    assert_eq!(
        settings,
        json!({ "temperature": 0.1, "instructions": "Be brief." })
    );
    assert_eq!(notes, json!({ "notes": "" }));
}

#[tokio::test]
async fn test_a_conversation_created_limited_to_its_tools_reads_the_limit_back() {
    // GIVEN the chat routes
    let dir = tempfile::tempdir().unwrap();
    let router = APIServer::build_router_for_test(chat_state(&dir));

    // WHEN a conversation is created limited to two tools, then one limited
    // to none
    let (created, session) = call(
        &router,
        "POST",
        "/api/v1/sessions",
        Some(json!({
            "mode": "libre",
            "tools": ["mcp:notion/search", "mcp:notion/fetch"],
            "limited_to_tools": true,
        })),
    )
    .await;
    let (refused, refusal) = call(
        &router,
        "POST",
        "/api/v1/sessions",
        Some(json!({ "mode": "libre", "limited_to_tools": true })),
    )
    .await;
    let (_, sessions) = call(&router, "GET", "/api/v1/sessions", None).await;

    // THEN the first carries the limit in its settings, and the second is
    // refused without leaving a conversation behind
    assert_eq!(created, StatusCode::CREATED, "{session}");
    let id = session["id"].as_str().unwrap();
    let (_, settings) = call(
        &router,
        "GET",
        &format!("/api/v1/sessions/{id}/settings"),
        None,
    )
    .await;
    assert_eq!(
        settings,
        json!({ "only_accesses": ["mcp:notion/search", "mcp:notion/fetch"] })
    );
    assert_eq!(refused, StatusCode::BAD_REQUEST, "{refusal}");
    assert_eq!(sessions.as_array().map(Vec::len), Some(1), "{sessions}");
}
