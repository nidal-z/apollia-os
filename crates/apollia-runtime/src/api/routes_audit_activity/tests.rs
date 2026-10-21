use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use chrono::{DateTime, Duration, SecondsFormat, Utc};
use serde_json::{json, Value};
use tower::ServiceExt;

use crate::api::server::APIServer;
use crate::api::test_support::app_state;
use crate::audit_journal::{AuditJournalHandle, JournalEntryDraft, JournalEntryKind as K};

/// The instant `days` days and `hours` hours before `now`, RFC 3339 UTC.
fn ago(now: DateTime<Utc>, days: i64, hours: i64) -> String {
    (now - Duration::days(days) - Duration::hours(hours)).to_rfc3339_opts(SecondsFormat::Secs, true)
}

/// The payload of a tool call of `tool` by `agent`: its start, or its end.
fn call(ended: bool, tool: &str, agent: &str, id: &str) -> Value {
    let mut payload = json!({
        "tool_name": tool, "agent_id": agent, "tool_call_id": id, "family": "read",
    });
    if ended {
        payload["success"] = json!(true);
    }
    payload
}

/// A journal of the last ten days: three searches in Notion by `quotes` and
/// one by `invoices` within the week, one by `quotes` ten days ago, one call
/// of GitHub, one native write, and a secret set on the Notion connection.
async fn journal(dir: &tempfile::TempDir, now: DateTime<Utc>) -> AuditJournalHandle {
    let journal = AuditJournalHandle::open(&dir.path().join("journal.db"))
        .await
        .expect("open journal for test");
    let notion = "mcp:notion/search";
    let drafts = [
        (
            "run-0",
            ago(now, 10, 0),
            K::ToolCallCompleted,
            call(true, notion, "quotes", "c0"),
        ),
        (
            "run-1",
            ago(now, 6, 0),
            K::ToolCallStarted,
            call(false, notion, "quotes", "c1"),
        ),
        (
            "run-1",
            ago(now, 6, 0),
            K::ToolCallCompleted,
            call(true, notion, "quotes", "c1"),
        ),
        (
            "run-1",
            ago(now, 4, 0),
            K::ToolCallCompleted,
            call(true, notion, "quotes", "c2"),
        ),
        (
            "run-2",
            ago(now, 3, 0),
            K::ToolCallCompleted,
            call(true, "mcp:github/list", "quotes", "g1"),
        ),
        (
            "run-3",
            ago(now, 2, 0),
            K::ToolCallCompleted,
            call(true, notion, "invoices", "c3"),
        ),
        (
            "run-1",
            ago(now, 1, 0),
            K::ToolCallCompleted,
            call(true, notion, "quotes", "c4"),
        ),
        (
            "run-4",
            ago(now, 0, 5),
            K::ToolCallCompleted,
            call(true, "file_write", "quotes", "w1"),
        ),
        (
            "connection:mcp:notion",
            ago(now, 0, 4),
            K::ConnectionSecretSet,
            json!({ "event": "secret_set", "connection_id": "mcp:notion", "key": "NOTION_TOKEN", "actor": "api" }),
        ),
    ];
    for (run, ts, kind, payload) in drafts {
        journal.append(JournalEntryDraft {
            run_id: run.to_owned(),
            ts,
            kind,
            payload,
        });
    }
    journal
}

/// GETs `uri` on a router over `journal`; the status and the JSON body.
async fn get(
    dir: &tempfile::TempDir,
    journal: Option<AuditJournalHandle>,
    uri: &str,
) -> (StatusCode, Value) {
    let repo = apollia_tools::TaskRepository::open(&dir.path().join("tasks.db"))
        .await
        .expect("open task repository for test");
    let mut state = app_state(dir.path().to_path_buf());
    state.audit_journal = journal;
    state.task_repository = Some(Arc::new(repo));
    let router = APIServer::build_router_for_test(state);
    let request = Request::builder().uri(uri).body(Body::empty()).unwrap();
    let response = router.oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
        .await
        .unwrap();
    (status, serde_json::from_slice(&bytes).unwrap())
}

#[tokio::test]
async fn the_journal_pages_the_entries_of_one_connection_only() {
    // GIVEN the journal of the last ten days
    let dir = tempfile::tempdir().unwrap();
    let now = Utc::now();
    let journal = journal(&dir, now).await;
    // WHEN the page is asked for the tools of the Notion connection
    let (status, page) = get(
        &dir,
        Some(journal),
        "/api/v1/audit/journal?limit=50&tool_prefix=mcp%3Anotion%2F",
    )
    .await;
    // THEN it holds the six Notion entries and nothing else: neither GitHub,
    // nor the native write, nor the secret, which names no tool
    assert_eq!(status, StatusCode::OK);
    let tools: Vec<&str> = page["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["summary"]["tool"].as_str().unwrap_or("<none>"))
        .collect();
    assert_eq!(tools, ["mcp:notion/search"; 6]);
    assert_eq!(page["count"], 6);
}

#[tokio::test]
async fn the_activity_of_a_connection_over_seven_days_counts_per_agent() {
    // GIVEN the journal of the last ten days
    let dir = tempfile::tempdir().unwrap();
    let now = Utc::now();
    let journal = journal(&dir, now).await;
    // WHEN the activity of Notion is asked with no period
    let (status, activity) = get(
        &dir,
        Some(journal),
        "/api/v1/audit/activity?tool_prefix=mcp%3Anotion%2F",
    )
    .await;
    // THEN the seven last days count: three ended calls by `quotes`, one by
    // `invoices`; the call of ten days ago, its start, GitHub and the native
    // write do not
    assert_eq!(status, StatusCode::OK, "{activity}");
    assert_eq!(activity["toolPrefix"], "mcp:notion/");
    assert_eq!(activity["actions"], 4);
    assert_eq!(activity["lastUsedAt"].as_str().unwrap(), ago(now, 1, 0));
    assert_eq!(
        activity["agents"],
        json!([
            { "agent": "quotes", "actions": 3, "lastUsedAt": ago(now, 1, 0) },
            { "agent": "invoices", "actions": 1, "lastUsedAt": ago(now, 2, 0) },
        ])
    );
    let since = chrono::DateTime::parse_from_rfc3339(activity["since"].as_str().unwrap()).unwrap();
    let days = (Utc::now() - since.with_timezone(&Utc)).num_hours();
    assert!((7 * 24 - 1..=7 * 24).contains(&days), "since {since}");
    assert!(activity["until"].is_null());
}

#[tokio::test]
async fn a_period_narrows_the_activity_and_a_quiet_connection_has_no_last_use() {
    // GIVEN the journal of the last ten days
    let dir = tempfile::tempdir().unwrap();
    let now = Utc::now();
    let journal = journal(&dir, now).await;
    // WHEN the activity of Notion is asked over the last three days, and that
    // of a connection no agent used
    let since = ago(now, 3, 0);
    let uri = format!(
        "/api/v1/audit/activity?tool_prefix=mcp%3Anotion%2F&since={}",
        since.replace(':', "%3A")
    );
    let (_, recent) = get(&dir, Some(journal.clone()), &uri).await;
    let (status, quiet) = get(
        &dir,
        Some(journal),
        "/api/v1/audit/activity?tool_prefix=mcp%3Asqlite%2F",
    )
    .await;
    // THEN the period keeps two calls, and the quiet connection counts none
    assert_eq!(recent["actions"], 2);
    assert_eq!(recent["since"], since);
    assert_eq!(status, StatusCode::OK);
    assert_eq!(quiet["actions"], 0);
    assert!(quiet["lastUsedAt"].is_null());
    assert_eq!(quiet["agents"], json!([]));
}

#[tokio::test]
async fn the_activity_refuses_a_missing_prefix_and_an_absent_journal() {
    // GIVEN a journal, and a runtime without one
    let dir = tempfile::tempdir().unwrap();
    let now = Utc::now();
    let journal = journal(&dir, now).await;
    // WHEN the activity is asked without a prefix, with a bad date, and with no journal
    let (missing, _) = get(&dir, Some(journal.clone()), "/api/v1/audit/activity").await;
    let (bad_date, _) = get(
        &dir,
        Some(journal),
        "/api/v1/audit/activity?tool_prefix=mcp%3Anotion%2F&since=yesterday",
    )
    .await;
    let (absent, _) = get(
        &dir,
        None,
        "/api/v1/audit/activity?tool_prefix=mcp%3Anotion%2F",
    )
    .await;
    // THEN the first two are refused as bad requests, the last as unavailable
    assert_eq!(missing, StatusCode::BAD_REQUEST);
    assert_eq!(bad_date, StatusCode::BAD_REQUEST);
    assert_eq!(absent, StatusCode::SERVICE_UNAVAILABLE);
}
