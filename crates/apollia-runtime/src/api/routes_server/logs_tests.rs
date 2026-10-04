use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use futures::StreamExt;
use tempfile::TempDir;
use tower::ServiceExt;

use crate::api::server::{APIServer, AppState};
use crate::api::test_support::{app_state, call, MockBackend, StemLoader};
use crate::audit_journal::{AuditJournalHandle, JournalEntryDraft, JournalEntryKind};
use crate::compat_server::tests::{settings_file, spawn_server};
use crate::compat_server::{ServerLogLevel, ServerLogsHandle};

/// A daemon whose compatible server is off, its logs empty.
fn daemon() -> (TempDir, AppState<MockBackend>) {
    let (dir, path) = settings_file("[server]\n");
    let mut state = app_state(
        Arc::new(StemLoader { refusal: None }),
        dir.path().to_path_buf(),
    );
    state.compat_server = Some(spawn_server(&path, &state.event_sender));
    (dir, state)
}

fn logs(state: &AppState<MockBackend>) -> ServerLogsHandle {
    state.compat_server.as_ref().unwrap().logs().clone()
}

/// The `log` frames of an event stream, until `count` of them arrived.
async fn frames(body: Body, count: usize) -> Vec<serde_json::Value> {
    let mut stream = body.into_data_stream();
    let mut text = String::new();
    while text.matches("event: log").count() < count {
        let chunk = stream.next().await.unwrap().unwrap();
        text.push_str(std::str::from_utf8(&chunk).unwrap());
    }
    text.lines()
        .filter_map(|line| line.strip_prefix("data: "))
        .map(|data| serde_json::from_str(data).unwrap())
        .collect()
}

#[tokio::test]
async fn test_the_stream_sends_the_held_lines_at_the_level_then_the_live_ones() {
    // GIVEN a daemon whose server logs hold an information line and an error
    let (_dir, state) = daemon();
    let logs = logs(&state);
    logs.engine_line("main: server is listening");
    logs.engine_line("error: failed to load the model");

    // WHEN a client follows the logs at the warning level, and two more lines
    // arrive
    let router = APIServer::build_router_for_test(state);
    let request = Request::get("/api/v1/server/logs?level=warn")
        .body(Body::empty())
        .unwrap();
    let response = router.oneshot(request).await.unwrap();
    let content_type = response.headers()["content-type"].clone();
    logs.engine_line("slot launch_slot_: id 0");
    logs.engine_line("warning: the context was shifted");

    // THEN it is an event stream whose first frame is the held error, then
    // the live warning; the information lines are filtered out
    assert_eq!(response.status(), StatusCode::OK);
    assert!(content_type
        .to_str()
        .unwrap()
        .starts_with("text/event-stream"));
    let frames = frames(response.into_body(), 2).await;
    assert_eq!(frames.len(), 2, "{frames:?}");
    assert_eq!(frames[0]["message"], "error: failed to load the model");
    assert_eq!(frames[0]["level"], "error");
    assert_eq!(frames[0]["source"], "engine");
    assert_eq!(frames[1]["message"], "warning: the context was shifted");
    assert_eq!(frames[1]["level"], "warn");
}

#[tokio::test]
async fn test_an_unknown_level_is_refused() {
    // GIVEN a daemon with a compatible server
    let (_dir, state) = daemon();

    // WHEN a client asks the logs at a level that does not exist
    let (status, _) = call(state, "GET", "/api/v1/server/logs?level=loud", None).await;

    // THEN the request is refused
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_clearing_the_logs_empties_them_and_leaves_the_journal_unchanged() {
    // GIVEN a daemon whose server logs hold two lines and whose journal holds
    // two entries
    let (dir, mut state) = daemon();
    let journal = AuditJournalHandle::open(&dir.path().join("audit_journal.db"))
        .await
        .unwrap();
    for (kind, n) in [
        (JournalEntryKind::AgentStarted, 1),
        (JournalEntryKind::AgentStopped, 2),
    ] {
        journal.append(JournalEntryDraft {
            run_id: "run-1".to_owned(),
            ts: "2026-10-03T00:00:00Z".to_owned(),
            kind,
            payload: serde_json::json!({ "n": n }),
        });
    }
    let before = journal.query_page(100, 0).await;
    assert_eq!(before.len(), 2);
    state.audit_journal = Some(journal.clone());
    let logs = logs(&state);
    logs.engine_line("one");
    logs.engine_line("two");

    // WHEN a client clears the logs
    let (status, body) = call(state, "DELETE", "/api/v1/server/logs", None).await;

    // THEN the two lines were cleared and the logs are empty, while the
    // journal holds the same entries, its chain intact
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["cleared"], 2);
    let held = logs.subscribe(ServerLogLevel::Debug).await.unwrap().backlog;
    assert!(held.is_empty(), "{held:?}");
    assert_eq!(journal.query_page(100, 0).await, before);
    let report = journal.verify_journal().await.unwrap();
    assert!(report.ok);
    assert_eq!(report.entries_checked, 2);
}

#[tokio::test]
async fn test_the_logs_routes_answer_503_without_a_compatible_server() {
    // GIVEN a daemon built without a compatible server
    let dir = TempDir::new().unwrap();
    let state = app_state(
        Arc::new(StemLoader { refusal: None }),
        dir.path().to_path_buf(),
    );

    // WHEN a client clears the logs
    let (status, _) = call(state, "DELETE", "/api/v1/server/logs", None).await;

    // THEN the route says the server is not available
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
}
