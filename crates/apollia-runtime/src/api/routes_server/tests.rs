use std::path::PathBuf;
use std::sync::Arc;

use axum::http::StatusCode;
use tempfile::TempDir;

use crate::api::server::AppState;
use crate::api::test_support::{app_state, call, MockBackend, StemLoader};
use crate::compat_server::tests::{
    file_enabled, free_port, held_port, listens, settings_file, spawn_server, spawn_server_under,
};

/// A daemon whose compatible server reads `[server]` from a file holding
/// `content`, with `{port}` replaced by a port free at the time of the call.
fn daemon_with_server(content: &str) -> (TempDir, PathBuf, u16, AppState<MockBackend>) {
    let port = free_port();
    let (dir, path) = settings_file(&content.replace("{port}", &port.to_string()));
    let mut state = app_state(
        Arc::new(StemLoader { refusal: None }),
        dir.path().to_path_buf(),
    );
    state.compat_server = Some(spawn_server(&path, &state.event_sender));
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
async fn test_restarting_over_http_opens_the_network_and_the_native_api_still_answers() {
    // GIVEN a daemon whose compatible server runs on the machine alone
    let (_dir, path, port, state) = daemon_with_server("[server]\nenabled = true\nport = {port}\n");
    let (_, before) = call(state.clone(), "GET", "/api/v1/server", None).await;
    assert_eq!(before["serveOnLan"], false);
    assert_eq!(before["address"], format!("http://127.0.0.1:{port}"));

    // WHEN the local network is switched on and a client restarts the server
    apollia_core::config::write_setting(
        &path,
        "server.serve_on_lan",
        serde_json::Value::Bool(true),
        &apollia_core::config::PolicySettings::none(),
    )
    .unwrap();
    let (status, body) = call(state.clone(), "POST", "/api/v1/server/restart", None).await;

    // THEN it runs on the network on the same port, the state gives the
    // address to copy, and the native API keeps answering
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["running"], true);
    assert_eq!(body["serveOnLan"], true);
    assert_eq!(body["port"], port);
    assert_eq!(
        body["address"],
        crate::compat_server::reachable_address(true, port)
    );
    assert!(listens(port).await);
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

// ── Tokens ───────────────────────────────────────────────────────────────

/// Every cell of every row of the token table, as text.
fn token_table_cells(settings: &std::path::Path) -> Vec<String> {
    let conn = rusqlite::Connection::open(settings.with_file_name("system.db")).unwrap();
    let mut stmt = conn
        .prepare(
            "SELECT id, name, hash, last_four, created_at,
                    COALESCE(last_used_at, ''), COALESCE(revoked_at, '')
             FROM server_tokens",
        )
        .unwrap();
    let rows = stmt
        .query_map([], |row| {
            Ok([
                row.get::<_, i64>(0)?.to_string(),
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
            ])
        })
        .unwrap();
    rows.flat_map(Result::unwrap).collect()
}

#[tokio::test]
async fn test_a_created_token_is_shown_once_and_no_column_holds_its_value() {
    // GIVEN a daemon with a compatible server and no token
    let (_dir, path, _, state) = daemon_with_server("[server]\nport = {port}\n");

    // WHEN a client creates a token named editor
    let (status, body) = call(
        state.clone(),
        "POST",
        "/api/v1/server/tokens",
        Some(serde_json::json!({ "name": "editor" })),
    )
    .await;

    // THEN the value is answered once, prefixed apo_, ending with the four
    // characters the token shows; no column of the table holds the value nor
    // its random part; the list never carries it
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let value = body["value"].as_str().unwrap().to_owned();
    assert!(value.starts_with("apo_") && value.len() == 68, "{value}");
    assert_eq!(body["name"], "editor");
    assert_eq!(body["lastFour"], &value[64..]);
    let secret = &value[4..];
    let cells = token_table_cells(&path);
    assert!(!cells.is_empty());
    assert!(
        cells.iter().all(|cell| !cell.contains(secret)),
        "a column holds the value"
    );
    let (_, list) = call(state, "GET", "/api/v1/server/tokens", None).await;
    assert!(!list.to_string().contains(secret));
    assert!(list[0].get("value").is_none());
}

#[tokio::test]
async fn test_tokens_list_without_values_and_revoke_by_name_or_identifier() {
    // GIVEN two tokens
    let (_dir, _path, _, state) = daemon_with_server("[server]\nport = {port}\n");
    for name in ["editor", "notebook"] {
        let (status, _) = call(
            state.clone(),
            "POST",
            "/api/v1/server/tokens",
            Some(serde_json::json!({ "name": name })),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED);
    }
    let (_, before) = call(state.clone(), "GET", "/api/v1/server", None).await;

    // WHEN one is revoked by name and the other by identifier
    let (by_name, revoked) = call(
        state.clone(),
        "DELETE",
        "/api/v1/server/tokens/editor",
        None,
    )
    .await;
    let (_, list) = call(state.clone(), "GET", "/api/v1/server/tokens", None).await;
    let id = list[1]["id"].as_i64().unwrap();
    let (by_id, _) = call(
        state.clone(),
        "DELETE",
        &format!("/api/v1/server/tokens/{id}"),
        None,
    )
    .await;

    // THEN the state counted two tokens in use, then none; the list keeps both
    // rows, revoked, with their name, last four characters and dates
    assert_eq!(before["activeTokens"], 2);
    assert_eq!(by_name, StatusCode::OK, "{revoked}");
    assert_eq!(revoked["revoked"], true);
    assert_eq!(by_id, StatusCode::OK);
    let (_, after) = call(state.clone(), "GET", "/api/v1/server", None).await;
    assert_eq!(after["activeTokens"], 0);
    let (_, list) = call(state, "GET", "/api/v1/server/tokens", None).await;
    let rows = list.as_array().unwrap();
    assert_eq!(rows.len(), 2);
    for row in rows {
        assert_eq!(row["revoked"], true);
        assert_eq!(row["lastFour"].as_str().unwrap().len(), 4);
        assert!(row["createdAt"].is_string());
        assert!(row.get("lastUsedAt").is_some());
    }
}

#[tokio::test]
async fn test_a_bad_name_a_taken_name_and_an_unknown_token_are_refused() {
    // GIVEN a token named editor
    let (_dir, _path, _, state) = daemon_with_server("[server]\nport = {port}\n");
    let create = |name: &str| {
        call(
            state.clone(),
            "POST",
            "/api/v1/server/tokens",
            Some(serde_json::json!({ "name": name })),
        )
    };
    assert_eq!(create("editor").await.0, StatusCode::CREATED);

    // WHEN a digits-only name, the same name, and an unknown token are asked
    let (numeric, _) = create("42").await;
    let (taken, _) = create("editor").await;
    let (unknown, _) = call(
        state.clone(),
        "DELETE",
        "/api/v1/server/tokens/nobody",
        None,
    )
    .await;

    // THEN each is refused with its own status
    assert_eq!(numeric, StatusCode::BAD_REQUEST);
    assert_eq!(taken, StatusCode::CONFLICT);
    assert_eq!(unknown, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_a_server_token_does_not_open_the_native_api() {
    // GIVEN the native API behind its internal token, and a token of the
    // compatible server
    let (_dir, _path, _, state) = daemon_with_server("[server]\nport = {port}\n");
    let created = state
        .compat_server
        .as_ref()
        .unwrap()
        .tokens()
        .create("editor".to_owned())
        .await
        .unwrap();
    let internal = crate::api::middleware::generate_hex_token();
    let native = crate::api::server::APIServer::build_router_for_test(state)
        .layer(crate::api::TokenAuthLayer::new(internal.as_str()));
    let ask = |token: &str| {
        axum::http::Request::builder()
            .uri("/api/v1/server")
            .header("authorization", format!("Bearer {token}"))
            .body(axum::body::Body::empty())
            .unwrap()
    };

    // WHEN the native API is called with each
    use tower::ServiceExt;
    let with_server_token = native.clone().oneshot(ask(&created.value)).await.unwrap();
    let with_internal = native.oneshot(ask(&internal)).await.unwrap();

    // THEN the server token is refused and the internal one accepted
    assert_eq!(with_server_token.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(with_internal.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_the_state_route_says_which_settings_the_policy_imposes() {
    // GIVEN a daemon without a policy, and one whose policy keeps the server
    // on the machine and requires its token
    let (_free_dir, _, _, free) = daemon_with_server("[server]\nport = {port}\n");
    let (dir, path, _, mut governed) = daemon_with_server("[server]\nport = {port}\n");
    let policy = apollia_core::config::PolicySettings::none()
        .impose("server.serve_on_lan", serde_json::json!(false))
        .impose("server.require_auth", serde_json::json!(true));
    governed.compat_server = Some(spawn_server_under(
        &path,
        &governed.event_sender,
        policy,
        None,
    ));

    // WHEN a client reads the state of each
    let (_, free) = call(free, "GET", "/api/v1/server", None).await;
    let (status, body) = call(governed, "GET", "/api/v1/server", None).await;

    // THEN each of the three settings says whether the policy imposes it
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        free["imposedByPolicy"],
        serde_json::json!({"serveOnLan": false, "requireAuth": false, "allowRequestMcp": false})
    );
    assert_eq!(
        body["imposedByPolicy"],
        serde_json::json!({"serveOnLan": true, "requireAuth": true, "allowRequestMcp": false})
    );
    drop(dir);
}
