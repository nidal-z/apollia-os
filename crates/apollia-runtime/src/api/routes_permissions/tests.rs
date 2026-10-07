use axum::http::StatusCode;
use serde_json::json;
use tempfile::TempDir;

use crate::api::server::AppState;
use crate::api::test_support::{app_state, call, MockBackend};
use crate::permission_rules::seed_proposals;

fn state(dir: &TempDir) -> AppState<MockBackend> {
    app_state(dir.path().to_path_buf())
}

#[tokio::test]
async fn test_the_proposals_are_listed_and_one_applied_over_http() {
    // GIVEN a daemon whose onboarding agent proposed two rules
    let dir = TempDir::new().unwrap();
    seed_proposals(
        dir.path(),
        &json!([
            {"tool_name": "file_read", "action": "allow", "scope": "global"},
            {"tool_name": "http_fetch", "action": "deny", "arg_prefix": "http://"}
        ]),
    );
    let state = state(&dir);

    // WHEN a client lists them, applies the second, then lists again
    let (listed, before) = call(state.clone(), "GET", "/api/v1/permissions/proposals", None).await;
    let (applied, rule) = call(
        state.clone(),
        "POST",
        "/api/v1/permissions/proposals/1/apply",
        None,
    )
    .await;
    let (_, after) = call(state, "GET", "/api/v1/permissions/proposals", None).await;

    // THEN both were listed as the card draws them, the rule has an id, and
    // the first proposal alone remains
    assert_eq!(listed, StatusCode::OK, "{before}");
    assert_eq!(before.as_array().unwrap().len(), 2);
    assert_eq!(before[1]["tool_name"], "http_fetch");
    assert_eq!(before[1]["arg_prefix"], "http://");
    assert_eq!(before[1]["scope"], "global");
    assert_eq!(applied, StatusCode::OK, "{rule}");
    assert!(rule["rule_id"].as_i64().unwrap() > 0);
    assert_eq!(after.as_array().unwrap().len(), 1);
    assert_eq!(after[0]["tool_name"], "file_read");
}

#[tokio::test]
async fn test_listing_over_a_namespace_database_that_does_not_open_is_refused() {
    // GIVEN a daemon whose onboarding namespace database is a directory, so
    // the store cannot open it
    let dir = TempDir::new().unwrap();
    let unreadable = dir.path().join("memory").join("onboarding.db");
    std::fs::create_dir_all(&unreadable).unwrap();

    // WHEN a client lists the proposals
    let (status, body) = call(state(&dir), "GET", "/api/v1/permissions/proposals", None).await;

    // THEN it is a 500 that names the database, as the embedded desktop says it
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{body}");
    let error = body["error"].as_str().unwrap();
    assert!(error.starts_with("failed to open "), "{error}");
    assert!(error.contains("onboarding.db"), "{error}");
}

#[tokio::test]
async fn test_applying_past_the_end_is_refused_with_the_embedded_words() {
    // GIVEN one proposal
    let dir = TempDir::new().unwrap();
    seed_proposals(
        dir.path(),
        &json!([{"tool_name": "file_read", "action": "allow"}]),
    );

    // WHEN a client applies the proposal at cursor 3
    let (status, body) = call(
        state(&dir),
        "POST",
        "/api/v1/permissions/proposals/3/apply",
        None,
    )
    .await;

    // THEN it is a 404 that says why, as the embedded desktop says it
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"], "proposal index 3 out of range (len=1)");
}

#[tokio::test]
async fn test_a_proposal_is_dismissed_over_http_and_a_second_dismissal_refused() {
    // GIVEN one proposal
    let dir = TempDir::new().unwrap();
    seed_proposals(
        dir.path(),
        &json!([{"tool_name": "file_read", "action": "allow"}]),
    );

    // WHEN a client dismisses it twice
    let (first, gone) = call(
        state(&dir),
        "DELETE",
        "/api/v1/permissions/proposals/0",
        None,
    )
    .await;
    let (second, refusal) = call(
        state(&dir),
        "DELETE",
        "/api/v1/permissions/proposals/0",
        None,
    )
    .await;

    // THEN the first removes it, the second finds nothing left
    assert_eq!(first, StatusCode::OK, "{gone}");
    assert_eq!(gone["dismissed"], 0);
    assert_eq!(second, StatusCode::NOT_FOUND);
    assert_eq!(refusal["error"], "no proposed rules in memory");
}

#[tokio::test]
async fn test_a_global_rule_is_added_over_http() {
    // GIVEN a daemon with no rule yet
    let dir = TempDir::new().unwrap();

    // WHEN a client adds a rule for every agent
    let (status, body) = call(
        state(&dir),
        "POST",
        "/api/v1/permissions/rules",
        Some(json!({
            "tool_name": "file_write",
            "arg_prefix": "/tmp/",
            "action": "allow",
            "scope": "global"
        })),
    )
    .await;

    // THEN the rule is stored and its id answered
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body["rule_id"].as_i64().unwrap() > 0);
}

#[tokio::test]
async fn test_a_rule_scoped_to_a_project_is_refused_over_http() {
    // GIVEN a daemon with no rule yet
    let dir = TempDir::new().unwrap();

    // WHEN a client adds a rule scoped to a project
    let (status, body) = call(
        state(&dir),
        "POST",
        "/api/v1/permissions/rules",
        Some(json!({"tool_name": "file_write", "action": "allow", "scope": "project"})),
    )
    .await;

    // THEN it is a 400 with the embedded words, and nothing was stored
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        body["error"],
        "unknown scope 'project', expected 'global' (a rule targets one agent or every agent)"
    );
    assert!(!dir
        .path()
        .join(apollia_tools::governance_db::GOVERNANCE_DB_FILENAME)
        .exists());
}

#[tokio::test]
async fn test_no_route_answers_a_file_request_any_more() {
    // GIVEN the daemon routes, where a write is consented to on the card of
    //       its family like any other tool
    let dir = TempDir::new().unwrap();

    // WHEN a client posts the answer the separate file prompt used to take
    let (status, _body) = call(
        state(&dir),
        "POST",
        "/api/v1/approvals/filesystem/req-1",
        Some(json!({ "decision": "approve" })),
    )
    .await;

    // THEN nothing is there to receive it
    assert_eq!(status, StatusCode::NOT_FOUND);
}
