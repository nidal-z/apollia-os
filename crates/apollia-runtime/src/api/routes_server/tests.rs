use std::path::PathBuf;
use std::sync::Arc;

use axum::http::StatusCode;
use tempfile::TempDir;

use crate::api::server::AppState;
use crate::api::test_support::{app_state, call, MockBackend, StemLoader};
use crate::compat_server::tests::{file_enabled, free_port, held_port, listens, settings_file};
use crate::compat_server::CompatServerHandle;

/// A daemon whose compatible server reads `[server]` from a file holding
/// `content`, with `{port}` replaced by a port free at the time of the call.
fn daemon_with_server(content: &str) -> (TempDir, PathBuf, u16, AppState<MockBackend>) {
    let port = free_port();
    let (dir, path) = settings_file(&content.replace("{port}", &port.to_string()));
    let mut state = app_state(
        Arc::new(StemLoader { refusal: None }),
        dir.path().to_path_buf(),
    );
    state.compat_server = Some(CompatServerHandle::spawn(path.clone(), &state.event_sender));
    (dir, path, port, state)
}

#[tokio::test]
async fn test_starting_over_http_opens_the_port_and_writes_the_switch_on() {
    // GIVEN a daemon whose compatible server is off
    let (_dir, path, port, state) = daemon_with_server("[server]\nport = {port}\n");

    // WHEN a client starts it
    let (status, body) = call(state.clone(), "POST", "/api/v1/server/start", None).await;

    // THEN it listens on its port, the file says on, and the state reads it
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["running"], true);
    assert_eq!(body["port"], port);
    assert!(listens(port).await);
    assert!(file_enabled(&path));
    let (_, read) = call(state, "GET", "/api/v1/server", None).await;
    assert_eq!(read["running"], true);
    assert_eq!(read["lastError"], serde_json::Value::Null);
}

#[tokio::test]
async fn test_stopping_over_http_refuses_connections_and_the_native_api_still_answers() {
    // GIVEN a daemon whose compatible server is on
    let (_dir, path, port, state) = daemon_with_server("[server]\nenabled = true\nport = {port}\n");
    let (_, before) = call(state.clone(), "GET", "/api/v1/server", None).await;
    assert_eq!(before["running"], true);

    // WHEN a client stops it
    let (status, body) = call(state.clone(), "POST", "/api/v1/server/stop", None).await;

    // THEN its port refuses connections, the file says off, and the native API
    // keeps answering
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["running"], false);
    assert!(!listens(port).await);
    assert!(!file_enabled(&path));
    let (health, _) = call(state, "GET", "/api/v1/health", None).await;
    assert_eq!(health, StatusCode::OK);
}

#[tokio::test]
async fn test_a_taken_port_is_refused_as_a_conflict_and_the_state_keeps_the_reason() {
    // GIVEN a daemon whose configured port another program holds
    let (_holder, held) = held_port();
    let (_dir, path, _, state) = daemon_with_server(&format!("[server]\nport = {held}\n"));

    // WHEN a client starts the server
    let (status, body) = call(state.clone(), "POST", "/api/v1/server/start", None).await;

    // THEN the start is refused as a conflict naming the port, the file is not
    // written, and the state route returns the reason
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    let reason = body["error"].as_str().unwrap();
    assert!(
        reason.contains(&held.to_string()) && reason.contains("already in use"),
        "{reason}"
    );
    assert!(!file_enabled(&path));
    let (_, read) = call(state, "GET", "/api/v1/server", None).await;
    assert_eq!(read["running"], false);
    assert_eq!(read["lastError"], reason);
}

#[tokio::test]
async fn test_a_runtime_without_a_compatible_server_answers_503() {
    // GIVEN a daemon state with no compatible server
    let dir = TempDir::new().unwrap();
    let state = app_state(
        Arc::new(StemLoader { refusal: None }),
        dir.path().to_path_buf(),
    );

    // WHEN a client reads its state
    let (status, body) = call(state, "GET", "/api/v1/server", None).await;

    // THEN the route says it is not available
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
}
