use axum::http::StatusCode;
use tempfile::TempDir;

use crate::api::server::AppState;
use crate::api::test_support::{app_state, call, MockBackend};

fn state_in(data_dir: std::path::PathBuf) -> AppState<MockBackend> {
    app_state(data_dir)
}

/// A daemon whose data directory is a regular file: `artifacts.db` cannot
/// be created under it.
fn broken_state(dir: &TempDir) -> AppState<MockBackend> {
    let file = dir.path().join("not-a-dir");
    std::fs::write(&file, b"").unwrap();
    state_in(file)
}

fn captured(session: &str, message: Option<&str>) -> serde_json::Value {
    serde_json::json!({
        "session_id": session,
        "source_message_id": message,
        "kind": "code",
        "language": "rust",
        "source_tool": "file_write",
        "title": "main.rs",
        "content": "fn main() {}",
    })
}

#[tokio::test]
async fn test_an_artifact_saved_over_http_is_listed_for_its_session_only() {
    // GIVEN a daemon with an empty data directory
    let dir = TempDir::new().unwrap();
    let state = state_in(dir.path().to_path_buf());

    // WHEN a client saves one artifact in session "s 1", then lists that
    // session and another one
    let (saved, artifact) = call(
        state.clone(),
        "POST",
        "/api/v1/artifacts",
        Some(captured("s 1", Some("m-1"))),
    )
    .await;
    let (listed, rows) = call(
        state.clone(),
        "GET",
        "/api/v1/sessions/s%201/artifacts",
        None,
    )
    .await;
    let (_, other) = call(state, "GET", "/api/v1/sessions/s-2/artifacts", None).await;

    // THEN the artifact comes back as stored, is listed for its own session,
    // and the file sits in the daemon's data directory
    assert_eq!(saved, StatusCode::OK, "{artifact}");
    assert_eq!(artifact["session_id"], "s 1");
    assert_eq!(artifact["title"], "main.rs");
    assert_eq!(listed, StatusCode::OK, "{rows}");
    assert_eq!(rows.as_array().unwrap().len(), 1);
    assert_eq!(rows[0]["id"], artifact["id"]);
    assert_eq!(other, serde_json::json!([]));
    assert!(dir.path().join("artifacts.db").exists());
}

#[tokio::test]
async fn test_a_second_save_of_the_same_message_returns_the_first_artifact() {
    // GIVEN an artifact captured from message m-1
    let dir = TempDir::new().unwrap();
    let state = state_in(dir.path().to_path_buf());
    let body = Some(captured("s-1", Some("m-1")));
    let (_, first) = call(state.clone(), "POST", "/api/v1/artifacts", body.clone()).await;

    // WHEN the chat detects the same tool output again
    let (status, second) = call(state.clone(), "POST", "/api/v1/artifacts", body).await;

    // THEN the stored artifact comes back and no duplicate is listed
    assert_eq!(status, StatusCode::OK);
    assert_eq!(second["id"], first["id"]);
    let (_, rows) = call(state, "GET", "/api/v1/sessions/s-1/artifacts", None).await;
    assert_eq!(rows.as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn test_a_patch_changes_only_the_fields_it_carries() {
    // GIVEN a stored artifact
    let dir = TempDir::new().unwrap();
    let state = state_in(dir.path().to_path_buf());
    let (_, artifact) = call(
        state.clone(),
        "POST",
        "/api/v1/artifacts",
        Some(captured("s-1", None)),
    )
    .await;
    let id = artifact["id"].as_str().unwrap().to_owned();

    // WHEN a client changes its content only
    let (status, changed) = call(
        state,
        "PATCH",
        &format!("/api/v1/artifacts/{id}"),
        Some(serde_json::json!({ "content": "fn main() { run(); }" })),
    )
    .await;

    // THEN the content is replaced and the title kept
    assert_eq!(status, StatusCode::OK, "{changed}");
    assert_eq!(changed["content"], "fn main() { run(); }");
    assert_eq!(changed["title"], "main.rs");
    assert_eq!(changed["created_at"], artifact["created_at"]);
}

#[tokio::test]
async fn test_a_patch_of_an_unknown_artifact_is_not_found_with_the_embedded_words() {
    // GIVEN a daemon with no artifact
    let dir = TempDir::new().unwrap();
    let state = state_in(dir.path().to_path_buf());

    // WHEN a client changes an artifact that does not exist
    let (status, refusal) = call(
        state,
        "PATCH",
        "/api/v1/artifacts/ghost",
        Some(serde_json::json!({ "title": "x" })),
    )
    .await;

    // THEN the daemon answers 404 with the words the desktop showed when it
    // ran the runtime itself
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(refusal["error"], "artifact not found: ghost");
}

#[tokio::test]
async fn test_a_deleted_artifact_is_gone_and_a_second_delete_still_answers() {
    // GIVEN a stored artifact
    let dir = TempDir::new().unwrap();
    let state = state_in(dir.path().to_path_buf());
    let (_, artifact) = call(
        state.clone(),
        "POST",
        "/api/v1/artifacts",
        Some(captured("s-1", None)),
    )
    .await;
    let route = format!("/api/v1/artifacts/{}", artifact["id"].as_str().unwrap());

    // WHEN it is deleted twice
    let (first, deleted) = call(state.clone(), "DELETE", &route, None).await;
    let (second, _) = call(state.clone(), "DELETE", &route, None).await;

    // THEN both succeed, as the embedded command did, and the session is empty
    assert_eq!(first, StatusCode::OK);
    assert_eq!(deleted["deleted"], artifact["id"]);
    assert_eq!(second, StatusCode::OK);
    let (_, rows) = call(state, "GET", "/api/v1/sessions/s-1/artifacts", None).await;
    assert_eq!(rows, serde_json::json!([]));
}

#[tokio::test]
async fn test_a_store_that_cannot_open_is_refused_on_every_route_with_its_reason() {
    // GIVEN a daemon whose data directory cannot hold artifacts.db
    let dir = TempDir::new().unwrap();
    let state = broken_state(&dir);

    // WHEN a client lists, saves, changes and deletes
    let (listed, list_refusal) =
        call(state.clone(), "GET", "/api/v1/sessions/s-1/artifacts", None).await;
    let (saved, save_refusal) = call(
        state.clone(),
        "POST",
        "/api/v1/artifacts",
        Some(captured("s-1", None)),
    )
    .await;
    let (changed, _) = call(
        state.clone(),
        "PATCH",
        "/api/v1/artifacts/a-1",
        Some(serde_json::json!({ "title": "x" })),
    )
    .await;
    let (deleted, _) = call(state, "DELETE", "/api/v1/artifacts/a-1", None).await;

    // THEN each answers 500 and names the directory it could not create
    for status in [listed, saved, changed, deleted] {
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    }
    for refusal in [list_refusal, save_refusal] {
        let message = refusal["error"].as_str().unwrap();
        assert!(
            message.starts_with("failed to create artifacts dir"),
            "{message}"
        );
    }
}
