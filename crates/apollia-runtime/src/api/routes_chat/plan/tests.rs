use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{json, Value};
use tower::ServiceExt;

use crate::api::routes_chat::tests::test_router_with_chat_plan_default;
use crate::api::test_support::{app_state, call};

/// Sends one request through `router` and reads the JSON answer.
async fn send(
    router: &axum::Router,
    method: &str,
    uri: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut request = Request::builder().method(method).uri(uri);
    let body = match body {
        Some(json) => {
            request = request.header("content-type", "application/json");
            Body::from(json.to_string())
        }
        None => Body::empty(),
    };
    let response = router
        .clone()
        .oneshot(request.body(body).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
        .await
        .unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

/// A libre session created through the API, its id.
async fn created_session(router: &axum::Router) -> String {
    let (status, body) = send(
        router,
        "POST",
        "/api/v1/sessions",
        Some(json!({ "mode": "libre" })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    body["id"].as_str().unwrap().to_owned()
}

/// Puts a submitted plan, awaiting approval, on `session_id` in the chat
/// database the manager reads, the way the agent's `plan_submit` leaves it.
async fn submitted_plan(dir: &tempfile::TempDir, session_id: &str) {
    let plan = crate::chat::plan_actor::spawn_plan_actor(
        rusqlite::Connection::open(dir.path().join("chat.db")).unwrap(),
        None,
    )
    .unwrap();
    let step: apollia_core::plan::PlanStep =
        serde_json::from_value(json!({ "step_id": "a", "description": "Read the folder" }))
            .unwrap();
    plan.propose(session_id, vec![step], None).await.unwrap();
    plan.submit(session_id).await.unwrap();
}

#[tokio::test]
async fn test_the_plan_mode_default_is_the_one_the_manager_was_spawned_with() {
    // GIVEN a runtime whose sessions start in plan mode
    let dir = tempfile::tempdir().unwrap();
    let router = test_router_with_chat_plan_default(&dir, true);

    // WHEN the default is read
    let (status, body) = send(&router, "GET", "/api/v1/sessions/plan-mode-default", None).await;

    // THEN it is the configured one
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, json!({ "enabled": true }));
}

#[tokio::test]
async fn test_the_plan_routes_answer_503_without_the_chat_subsystem() {
    // GIVEN a runtime started without the chat session manager
    let dir = tempfile::tempdir().unwrap();

    // WHEN each plan route is called
    for (method, uri, body) in [
        ("GET", "/api/v1/sessions/plan-mode-default", None),
        (
            "PUT",
            "/api/v1/sessions/s-1/plan-mode",
            Some(json!({ "enabled": true })),
        ),
        ("GET", "/api/v1/sessions/s-1/plan", None),
        ("GET", "/api/v1/sessions/s-1/plan/mutations", None),
        ("POST", "/api/v1/sessions/s-1/plan/approve", None),
        ("POST", "/api/v1/sessions/s-1/plan/reject", Some(json!({}))),
    ] {
        let state = app_state(dir.path().to_path_buf());
        let (status, answer) = call(state, method, uri, body).await;

        // THEN each says the subsystem is unavailable, in the embedded words
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{method} {uri}");
        assert_eq!(answer["error"], "chat subsystem not available");
    }
}

#[tokio::test]
async fn test_plan_mode_is_turned_on_and_the_plan_read_with_its_phase() {
    // GIVEN a session with no plan
    let dir = tempfile::tempdir().unwrap();
    let router = test_router_with_chat_plan_default(&dir, false);
    let id = created_session(&router).await;

    // WHEN plan mode is turned on, then the plan and its history are read
    let (set, set_body) = send(
        &router,
        "PUT",
        &format!("/api/v1/sessions/{id}/plan-mode"),
        Some(json!({ "enabled": true })),
    )
    .await;
    let (read, snapshot) = send(&router, "GET", &format!("/api/v1/sessions/{id}/plan"), None).await;
    let (listed, mutations) = send(
        &router,
        "GET",
        &format!("/api/v1/sessions/{id}/plan/mutations"),
        None,
    )
    .await;

    // THEN the session is in discovery, with no plan and no mutation yet
    assert_eq!(set, StatusCode::OK, "{set_body}");
    assert_eq!(set_body["status"], "plan_mode_on");
    assert_eq!(read, StatusCode::OK);
    assert_eq!(snapshot, json!({ "plan": null, "phase": "discovery" }));
    assert_eq!(listed, StatusCode::OK);
    assert_eq!(mutations, json!([]));
}

#[tokio::test]
async fn test_an_unknown_session_is_refused_with_the_manager_message() {
    // GIVEN a runtime with no session `ghost`
    let dir = tempfile::tempdir().unwrap();
    let router = test_router_with_chat_plan_default(&dir, false);

    // WHEN plan mode, the plan, its history, an approval and a rejection are
    // asked of it
    for (method, uri, body) in [
        (
            "PUT",
            "/api/v1/sessions/ghost/plan-mode",
            Some(json!({ "enabled": true })),
        ),
        ("GET", "/api/v1/sessions/ghost/plan", None),
        ("GET", "/api/v1/sessions/ghost/plan/mutations", None),
        ("POST", "/api/v1/sessions/ghost/plan/approve", None),
        (
            "POST",
            "/api/v1/sessions/ghost/plan/reject",
            Some(json!({})),
        ),
    ] {
        let (status, answer) = send(&router, method, uri, body).await;

        // THEN each is a 404 carrying the text the embedded command shows
        assert_eq!(status, StatusCode::NOT_FOUND, "{method} {uri}");
        assert_eq!(
            answer["error"], "session not found: ghost",
            "{method} {uri}"
        );
    }
}

#[tokio::test]
async fn test_a_plan_not_awaiting_approval_is_neither_approved_nor_rejected() {
    // GIVEN a session in plan mode whose agent has submitted nothing
    let dir = tempfile::tempdir().unwrap();
    let router = test_router_with_chat_plan_default(&dir, true);
    let id = created_session(&router).await;

    // WHEN the plan is approved, then rejected
    let (approved, approve_body) = send(
        &router,
        "POST",
        &format!("/api/v1/sessions/{id}/plan/approve"),
        None,
    )
    .await;
    let (rejected, reject_body) = send(
        &router,
        "POST",
        &format!("/api/v1/sessions/{id}/plan/reject"),
        Some(json!({ "reason": "too broad" })),
    )
    .await;

    // THEN both are refused as a conflict naming the phase
    assert_eq!(approved, StatusCode::CONFLICT);
    assert_eq!(rejected, StatusCode::CONFLICT);
    for body in [approve_body, reject_body] {
        let message = body["error"].as_str().unwrap();
        assert!(message.contains("is not awaiting approval"), "{message}");
    }
}

#[tokio::test]
async fn test_a_submitted_plan_is_rejected_then_approved() {
    // GIVEN a session in plan mode whose agent submitted a one-step plan
    let dir = tempfile::tempdir().unwrap();
    let router = test_router_with_chat_plan_default(&dir, true);
    let id = created_session(&router).await;
    submitted_plan(&dir, &id).await;

    // WHEN the plan is rejected with a reason, then approved
    let (rejected, reject_body) = send(
        &router,
        "POST",
        &format!("/api/v1/sessions/{id}/plan/reject"),
        Some(json!({ "reason": "add a step" })),
    )
    .await;
    let (approved, approve_body) = send(
        &router,
        "POST",
        &format!("/api/v1/sessions/{id}/plan/approve"),
        None,
    )
    .await;
    let (_, snapshot) = send(&router, "GET", &format!("/api/v1/sessions/{id}/plan"), None).await;

    // THEN both are accepted and the session moves to execution
    assert_eq!(rejected, StatusCode::OK, "{reject_body}");
    assert_eq!(reject_body["status"], "rejected");
    assert_eq!(approved, StatusCode::OK, "{approve_body}");
    assert_eq!(approve_body["status"], "approved");
    assert_eq!(snapshot["phase"], "executing");
    assert_eq!(snapshot["plan"]["steps"][0]["step_id"], "a");
}
