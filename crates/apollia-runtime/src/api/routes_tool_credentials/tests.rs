use axum::http::StatusCode;
use tempfile::TempDir;

use crate::api::server::AppState;
use crate::api::test_support::{app_state, call, MemorySecrets, MockBackend};

fn state_in(dir: &std::path::Path) -> AppState<MockBackend> {
    let mut state = app_state(dir.to_path_buf());
    state.secret_store = MemorySecrets::default().opener();
    state
}

/// A daemon whose data directory is a plain file, so no store opens.
fn broken_state(dir: &TempDir) -> AppState<MockBackend> {
    let file = dir.path().join("not-a-directory");
    std::fs::write(&file, b"x").unwrap();
    state_in(&file)
}

#[tokio::test]
async fn test_stored_credentials_are_listed_by_tool_without_their_values() {
    // GIVEN a daemon holding a credential for two tools
    let dir = TempDir::new().unwrap();
    let state = state_in(dir.path());
    for (tool, key) in [("web_search", "brave.api_key"), ("mailer", "smtp.password")] {
        let (status, stored) = call(
            state.clone(),
            "PUT",
            &format!("/api/v1/tool-credentials/{tool}/{key}"),
            Some(serde_json::json!({ "value": "s3cret-value" })),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{stored}");
        assert_eq!(stored["key_name"], key);
    }

    // WHEN a client lists the credentials of one tool, then of all
    let (status, one) = call(
        state.clone(),
        "GET",
        "/api/v1/tool-credentials?tool_name=web_search",
        None,
    )
    .await;
    let (_, all) = call(state, "GET", "/api/v1/tool-credentials", None).await;

    // THEN the filter keeps the one tool, and no answer carries the value
    assert_eq!(status, StatusCode::OK, "{one}");
    let one = one.as_array().unwrap();
    assert_eq!(one.len(), 1);
    assert_eq!(one[0]["tool_name"], "web_search");
    assert_eq!(one[0]["key_name"], "brave.api_key");
    assert!(one[0]["created_at"].as_str().is_some_and(|d| !d.is_empty()));
    assert_eq!(all.as_array().map(Vec::len), Some(2));
    assert!(!all.to_string().contains("s3cret-value"));
}

#[tokio::test]
async fn test_a_list_over_a_store_that_does_not_open_is_refused_with_its_cause() {
    // GIVEN a daemon whose data directory cannot hold governance.db
    let dir = TempDir::new().unwrap();
    let state = broken_state(&dir);

    // WHEN a client lists the credentials
    let (status, refusal) = call(state, "GET", "/api/v1/tool-credentials", None).await;

    // THEN the refusal names the database that did not open
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert!(refusal["error"]
        .as_str()
        .is_some_and(|e| e.starts_with("failed to open governance database")));
}

#[tokio::test]
async fn test_an_empty_value_or_a_blank_key_is_refused_before_anything_is_written() {
    // GIVEN an empty daemon
    let dir = TempDir::new().unwrap();
    let state = state_in(dir.path());

    // WHEN a client stores an empty value, then a value under a blank key
    let (empty, empty_refusal) = call(
        state.clone(),
        "PUT",
        "/api/v1/tool-credentials/web_search/brave.api_key",
        Some(serde_json::json!({ "value": "" })),
    )
    .await;
    let (blank, blank_refusal) = call(
        state.clone(),
        "PUT",
        "/api/v1/tool-credentials/web_search/%20",
        Some(serde_json::json!({ "value": "k" })),
    )
    .await;
    let (_, listed) = call(state, "GET", "/api/v1/tool-credentials", None).await;

    // THEN both are 400 with the words the embedded desktop showed, and the
    // store stays empty
    assert_eq!(empty, StatusCode::BAD_REQUEST);
    assert_eq!(empty_refusal["error"], "credential value must not be empty");
    assert_eq!(blank, StatusCode::BAD_REQUEST);
    assert_eq!(
        blank_refusal["error"],
        "tool_name and key_name must not be empty"
    );
    assert_eq!(listed.as_array().map(Vec::len), Some(0));
}

#[tokio::test]
async fn test_a_deleted_credential_is_gone_and_a_second_delete_removes_nothing() {
    // GIVEN a daemon holding one credential
    let dir = TempDir::new().unwrap();
    let state = state_in(dir.path());
    let route = "/api/v1/tool-credentials/web_search/brave.api_key";
    call(
        state.clone(),
        "PUT",
        route,
        Some(serde_json::json!({ "value": "k" })),
    )
    .await;

    // WHEN it is deleted twice
    let (first, deleted) = call(state.clone(), "DELETE", route, None).await;
    let (second, again) = call(state.clone(), "DELETE", route, None).await;
    let (_, listed) = call(state, "GET", "/api/v1/tool-credentials", None).await;

    // THEN the first removes it, the second answers that nothing was there
    assert_eq!(first, StatusCode::OK);
    assert_eq!(deleted["removed"], true);
    assert_eq!(second, StatusCode::OK);
    assert_eq!(again["removed"], false);
    assert_eq!(listed.as_array().map(Vec::len), Some(0));
}

#[tokio::test]
async fn test_a_delete_over_a_store_that_does_not_open_is_refused() {
    // GIVEN a daemon whose data directory cannot hold governance.db
    let dir = TempDir::new().unwrap();
    let state = broken_state(&dir);

    // WHEN a client deletes a credential
    let (status, refusal) = call(
        state,
        "DELETE",
        "/api/v1/tool-credentials/web_search/brave.api_key",
        None,
    )
    .await;

    // THEN the daemon refuses with the cause
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert!(refusal["error"].as_str().is_some());
}

#[tokio::test]
async fn test_a_web_search_test_without_a_key_answers_not_configured() {
    // GIVEN a daemon with no Brave key stored
    let dir = TempDir::new().unwrap();
    let state = state_in(dir.path());

    // WHEN a client tests the web search credential
    let (status, outcome) = call(
        state,
        "POST",
        "/api/v1/tool-credential-tests/web_search",
        None,
    )
    .await;

    // THEN the answer is a failed test that says the key is missing, made
    // without any call out
    assert_eq!(status, StatusCode::OK, "{outcome}");
    assert_eq!(outcome["ok"], false);
    assert_eq!(outcome["latency_ms"], serde_json::Value::Null);
    assert_eq!(outcome["error"], "brave.api_key is not configured");
}

#[tokio::test]
async fn test_a_tool_without_a_live_test_is_refused_by_name() {
    // GIVEN an empty daemon
    let dir = TempDir::new().unwrap();
    let state = state_in(dir.path());

    // WHEN a client tests a tool that has no live test
    let (status, refusal) = call(state, "POST", "/api/v1/tool-credential-tests/mailer", None).await;

    // THEN the daemon refuses with the embedded desktop's words
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        refusal["error"],
        "no credential test implemented for tool 'mailer'"
    );
}
