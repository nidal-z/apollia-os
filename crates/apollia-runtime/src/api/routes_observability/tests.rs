use axum::http::StatusCode;
use tempfile::TempDir;

use crate::api::server::AppState;
use crate::api::test_support::{app_state, call, MockBackend};

fn state_in(dir: &TempDir) -> AppState<MockBackend> {
    app_state(dir.path().to_path_buf())
}

/// Seeds `audit.db` with one tool invocation started a minute ago.
fn seed_audit(dir: &TempDir) {
    let path = dir
        .path()
        .join(apollia_core::paths::DataFile::Audit.file_name());
    let conn = rusqlite::Connection::open(path).unwrap();
    conn.execute_batch(
        "CREATE TABLE tool_invocations (
            id TEXT, agent_id TEXT, task_id TEXT, tool_name TEXT, started_at TEXT,
            duration_ms INTEGER, exit_code INTEGER, success INTEGER, error_code TEXT
        );",
    )
    .unwrap();
    let started = (chrono::Utc::now() - chrono::Duration::minutes(1))
        .format("%Y-%m-%dT%H:%M:%SZ")
        .to_string();
    conn.execute(
        "INSERT INTO tool_invocations VALUES ('inv-1','scribe','t-1','file_io',?1,42,0,1,NULL)",
        [started],
    )
    .unwrap();
}

/// Seeds `mailbox.db` with two messages of increasing `seq`.
fn seed_mailbox(dir: &TempDir) {
    let path = dir
        .path()
        .join(apollia_core::paths::DataFile::Mailbox.file_name());
    let conn = rusqlite::Connection::open(path).unwrap();
    conn.execute_batch(
        "CREATE TABLE mailbox_messages (
            message_id TEXT PRIMARY KEY, to_agent TEXT NOT NULL, from_agent TEXT NOT NULL,
            payload TEXT NOT NULL, sent_at TEXT NOT NULL, created_unix INTEGER NOT NULL,
            state TEXT NOT NULL, lease_until_unix INTEGER, lease_owner TEXT, seq INTEGER NOT NULL
        );
        INSERT INTO mailbox_messages VALUES
            ('m-1','b','a','{}','2026-07-01T09:00:00Z',1,'pending',NULL,NULL,1);
        INSERT INTO mailbox_messages VALUES
            ('m-2','a','b','{\"kind\":\"ping\"}','2026-07-01T09:01:00Z',2,'in_flight',NULL,NULL,2);",
    )
    .unwrap();
}

#[tokio::test]
async fn test_the_timeline_of_a_window_is_read_from_the_data_directory() {
    // GIVEN a daemon whose audit store holds one tool call of the last minute
    let dir = TempDir::new().unwrap();
    seed_audit(&dir);

    // WHEN a client reads the timeline of the last thirty minutes
    let (status, events) = call(
        state_in(&dir),
        "GET",
        "/api/v1/observability/timeline?window_minutes=30",
        None,
    )
    .await;

    // THEN the tool call comes back as the view draws it
    assert_eq!(status, StatusCode::OK, "{events}");
    let events = events.as_array().unwrap();
    assert_eq!(events.len(), 1, "{events:?}");
    assert_eq!(events[0]["event_type"], "tool");
    assert_eq!(events[0]["summary"], "[scribe] Tool: file_io (42ms)");
    assert_eq!(events[0]["detail"]["id"], "inv-1");
}

#[tokio::test]
async fn test_a_timeline_without_a_valid_window_is_refused() {
    // GIVEN a daemon with an empty data directory
    let dir = TempDir::new().unwrap();

    // WHEN a client asks for a window of zero minutes, then for none
    let (zero, body) = call(
        state_in(&dir),
        "GET",
        "/api/v1/observability/timeline?window_minutes=0",
        None,
    )
    .await;
    let (missing, _) = call(
        state_in(&dir),
        "GET",
        "/api/v1/observability/timeline",
        None,
    )
    .await;

    // THEN both are refused with a message naming the bound
    assert_eq!(zero, StatusCode::BAD_REQUEST);
    assert_eq!(missing, StatusCode::BAD_REQUEST);
    assert_eq!(
        body["error"], "window_minutes must be between 1 and 10080",
        "{body}"
    );
}

#[tokio::test]
async fn test_the_mailbox_messages_are_listed_newest_first() {
    // GIVEN a daemon whose mailbox store holds two messages
    let dir = TempDir::new().unwrap();
    seed_mailbox(&dir);

    // WHEN a client lists the mailbox
    let (status, rows) = call(
        state_in(&dir),
        "GET",
        "/api/v1/mailbox/messages?limit=100",
        None,
    )
    .await;

    // THEN the newest message comes first, its payload as stored
    assert_eq!(status, StatusCode::OK, "{rows}");
    let rows = rows.as_array().unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["message_id"], "m-2");
    assert_eq!(rows[0]["payload"], r#"{"kind":"ping"}"#);
    assert_eq!(rows[0]["state"], "in_flight");
    assert_eq!(rows[1]["message_id"], "m-1");
}

#[tokio::test]
async fn test_a_mailbox_limit_of_zero_is_refused() {
    // GIVEN a daemon whose mailbox store holds two messages
    let dir = TempDir::new().unwrap();
    seed_mailbox(&dir);

    // WHEN a client asks for zero rows
    let (status, body) = call(
        state_in(&dir),
        "GET",
        "/api/v1/mailbox/messages?limit=0",
        None,
    )
    .await;

    // THEN the request is refused with its reason
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "limit must be at least 1");
}
