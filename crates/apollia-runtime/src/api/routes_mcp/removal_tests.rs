//! Removing a server that never started, and a change of configuration that
//! fails to start.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;

use super::governance_tests::{harness, mock_server, persisted};
use super::{remove_server, update_server_config};

// GIVEN a server stored in mcp.db that has no session, as after a failed start
// WHEN it is removed through the route
// THEN the route answers it removed and mcp.db no longer holds it
#[tokio::test]
async fn test_a_server_that_never_started_is_removed() {
    // GIVEN
    let h = harness(Arc::new(crate::governance::NoPolicyAdmission)).await;
    h.state
        .mcp_server_repo
        .as_ref()
        .unwrap()
        .lock()
        .unwrap()
        .save(&mock_server("calc"))
        .unwrap();
    assert!(h.manager.status().await.is_empty());

    // WHEN
    let Json(removed) = remove_server(State(h.state.clone()), Path("calc".to_string()))
        .await
        .expect("remove");

    // THEN
    assert_eq!(removed["removed"], "calc");
    assert!(persisted(&h).is_empty());
    h.manager.shutdown().await;
}

// GIVEN no server running or stored under a name
// WHEN it is removed through the route
// THEN the route answers 404
#[tokio::test]
async fn test_an_unknown_server_is_not_found() {
    // GIVEN
    let h = harness(Arc::new(crate::governance::NoPolicyAdmission)).await;

    // WHEN
    let (code, _) = remove_server(State(h.state.clone()), Path("absent".to_string()))
        .await
        .unwrap_err();

    // THEN
    assert_eq!(code, StatusCode::NOT_FOUND);
    h.manager.shutdown().await;
}

// GIVEN a running server, stored
// WHEN its configuration is changed to a command that cannot start
// THEN the route answers 400, and the server is still running and stored
//      under its previous configuration
#[tokio::test]
async fn test_a_change_that_fails_to_start_keeps_the_server() {
    // GIVEN
    let h = harness(Arc::new(crate::governance::NoPolicyAdmission)).await;
    let (code, _) = super::add_server(State(h.state.clone()), Json(mock_server("calc")))
        .await
        .expect("add");
    assert_eq!(code, StatusCode::CREATED);

    // WHEN
    let (code, _) = update_server_config(
        State(h.state.clone()),
        Path("calc".to_string()),
        Json(serde_json::json!({ "command": "/nonexistent/apollia-no-such-server" })),
    )
    .await
    .unwrap_err();

    // THEN
    assert_eq!(code, StatusCode::BAD_REQUEST);
    let status = h.manager.status().await;
    assert_eq!(status.len(), 1);
    assert_eq!(status[0].name, "calc");
    assert_eq!(persisted(&h)[0].command, "python3");
    h.manager.shutdown().await;
}
