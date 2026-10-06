use std::sync::Arc;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use serde_json::{json, Value};
use tower::ServiceExt;

use crate::api::server::APIServer;
use crate::api::test_support::{app_state, StemLoader};
use crate::audit_journal::{AuditJournalHandle, JournalEntryDraft, JournalEntryKind as K};
use crate::governance::JOURNAL_EXPORT_RUN_ID;

/// A journal holding one write by `quotes` (asked, started, captured, ended),
/// one failed fetch by `invoices`, and a policy applied.
async fn journal(dir: &tempfile::TempDir) -> AuditJournalHandle {
    let journal = AuditJournalHandle::open(&dir.path().join("journal.db"))
        .await
        .expect("open journal for test");
    let drafts = [
        (
            "run-1",
            "2026-10-01T08:00:00Z",
            K::LlmCompletion,
            json!({
                "content": "", "model_id": "m",
                "tool_calls": [{ "id": "c0", "name": "file_write",
                    "arguments": { "path": "/home/u/a.md", "content": "SECRET-ARGUMENT" } }],
            }),
        ),
        (
            "run-1",
            "2026-10-01T08:00:01Z",
            K::ToolCallStarted,
            json!({
                "tool_name": "file_write", "agent_id": "quotes", "tool_call_id": "c0",
                "family": "write_or_delete", "target": "/home/u/a.md",
            }),
        ),
        (
            "run-1",
            "2026-10-01T08:00:02Z",
            K::ToolOutput,
            json!({
                "tool_call_id": "c0", "tool_name": "file_write",
                "output": "SECRET-OUTPUT", "status": "success",
            }),
        ),
        (
            "run-1",
            "2026-10-01T08:00:03Z",
            K::ToolCallCompleted,
            json!({
                "tool_name": "file_write", "agent_id": "quotes", "tool_call_id": "c0",
                "family": "write_or_delete", "target": "/home/u/a.md", "success": true,
            }),
        ),
        (
            "run-2",
            "2026-10-02T08:00:00Z",
            K::ToolCallCompleted,
            json!({
                "tool_name": "http_fetch", "agent_id": "invoices", "tool_call_id": "e9",
                "family": "send_external", "target": "example.org", "success": false,
            }),
        ),
        (
            "governance:policy:organisation",
            "2026-10-05T08:00:00Z",
            K::PolicyApplied,
            json!({
                "event": "policy_applied", "key_fingerprint": "ab", "policy_digest": "d1",
                "publisher": "Acme IT", "signed_at": "2026-10-01T00:00:00Z",
            }),
        ),
    ];
    for (run, ts, kind, payload) in drafts {
        journal.append(JournalEntryDraft {
            run_id: run.to_string(),
            ts: ts.to_string(),
            kind,
            payload,
        });
    }
    journal
}

/// What a GET of `uri` answered: its status, its two file headers, its body.
struct Answer {
    status: StatusCode,
    content_type: String,
    disposition: String,
    body: String,
}

/// GETs `uri` on a router over `journal`, with a task repository that knows
/// no task.
async fn get(dir: &tempfile::TempDir, journal: Option<AuditJournalHandle>, uri: &str) -> Answer {
    let repo = apollia_tools::TaskRepository::open(&dir.path().join("tasks.db"))
        .await
        .expect("open task repository for test");
    let mut state = app_state(
        Arc::new(StemLoader { refusal: None }),
        dir.path().to_path_buf(),
    );
    state.audit_journal = journal;
    state.task_repository = Some(Arc::new(repo));
    let router = APIServer::build_router_for_test(state);
    let request = Request::builder().uri(uri).body(Body::empty()).unwrap();
    let response = router.oneshot(request).await.unwrap();
    let header_text = |name: header::HeaderName| {
        response
            .headers()
            .get(name)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_owned()
    };
    let status = response.status();
    let content_type = header_text(header::CONTENT_TYPE);
    let disposition = header_text(header::CONTENT_DISPOSITION);
    let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
        .await
        .unwrap();
    Answer {
        status,
        content_type,
        disposition,
        body: String::from_utf8(bytes.to_vec()).unwrap(),
    }
}

/// The payloads of the exports the journal recorded, oldest first.
async fn recorded_exports(journal: &AuditJournalHandle) -> Vec<Value> {
    journal
        .query_run(JOURNAL_EXPORT_RUN_ID)
        .await
        .into_iter()
        .map(|entry| {
            assert_eq!(entry.kind, K::JournalExported);
            entry.payload
        })
        .collect()
}

#[tokio::test]
async fn test_the_csv_export_is_the_filtered_journal_in_summary() {
    // GIVEN a journal whose write carried content
    let dir = tempfile::tempdir().unwrap();
    let journal = journal(&dir).await;

    // WHEN the actions are exported as CSV, the detail not asked for
    let answer = get(
        &dir,
        Some(journal),
        "/api/v1/audit/export?format=csv&nature=action",
    )
    .await;

    // THEN the answer is a CSV file offered under the name of the journal
    assert_eq!(answer.status, StatusCode::OK);
    assert_eq!(answer.content_type, "text/csv; charset=utf-8");
    assert_eq!(answer.disposition, "attachment; filename=\"journal.csv\"");
    // AND it holds a header row and the three actions, newest first
    let rows: Vec<&str> = answer.body.split("\r\n").collect();
    assert_eq!(
        rows[0],
        "ts,agent,nature,family,what,tool,target,outcome,run_id,seq,kind,hash"
    );
    assert_eq!(rows.len(), 5, "{}", answer.body);
    assert!(rows[1].starts_with(
        "2026-10-02T08:00:00Z,invoices,action,send_external,Tool call finished,http_fetch,example.org,failure,run-2,0,"
    ));
    assert!(rows[2].starts_with("2026-10-01T08:00:03Z,quotes,action,write_or_delete,"));
    assert!(rows[3].starts_with("2026-10-01T08:00:01Z,quotes,action,write_or_delete,"));
    // AND neither the argument nor the output of the write is in the file
    assert!(!answer.body.contains("SECRET"), "{}", answer.body);
}

#[tokio::test]
async fn test_the_json_export_holds_the_detail_when_asked() {
    // GIVEN a journal whose write carried content
    let dir = tempfile::tempdir().unwrap();
    let journal = journal(&dir).await;

    // WHEN what names `quotes` is exported as JSON, once in summary and once
    // with the detail
    let summary = get(
        &dir,
        Some(journal.clone()),
        "/api/v1/audit/export?format=json&agents=quotes",
    )
    .await;
    let detailed = get(
        &dir,
        Some(journal),
        "/api/v1/audit/export?format=json&agents=quotes&detail=true",
    )
    .await;

    // THEN both are JSON files of the two entries that name the agent
    for answer in [&summary, &detailed] {
        assert_eq!(answer.status, StatusCode::OK);
        assert_eq!(answer.content_type, "application/json");
        assert_eq!(answer.disposition, "attachment; filename=\"journal.json\"");
    }
    let document: Value = serde_json::from_str(&summary.body).unwrap();
    assert_eq!(document["detail"], false);
    assert_eq!(document["count"], 2);
    // AND the summary holds no content
    assert!(!summary.body.contains("SECRET"), "{}", summary.body);
    assert!(document["entries"][0].get("detail").is_none());
    // AND the detail holds the arguments and the output of the write
    let document: Value = serde_json::from_str(&detailed.body).unwrap();
    assert_eq!(document["detail"], true);
    assert_eq!(document["count"], 2);
    let call = &document["entries"][0]["detail"]["tool_call"];
    assert_eq!(call["arguments"]["content"], "SECRET-ARGUMENT");
    assert_eq!(call["output"], "SECRET-OUTPUT");
}

#[tokio::test]
async fn test_each_export_is_recorded_with_its_format_filters_and_detail() {
    // GIVEN a journal that was never exported
    let dir = tempfile::tempdir().unwrap();
    let journal = journal(&dir).await;
    assert!(recorded_exports(&journal).await.is_empty());

    // WHEN it is exported as CSV with the detail and every filter, then as
    // JSON in summary with none
    let first = get(
        &dir,
        Some(journal.clone()),
        "/api/v1/audit/export?format=csv&detail=true&agents=quotes,invoices\
         &family=write_or_delete&nature=action&result=success\
         &since=2026-10-01&until=2026-10-02T00:00:00Z",
    )
    .await;
    let second = get(
        &dir,
        Some(journal.clone()),
        "/api/v1/audit/export?format=json",
    )
    .await;

    // THEN the journal holds one entry per export, in order
    assert_eq!(first.status, StatusCode::OK);
    assert_eq!(second.status, StatusCode::OK);
    let exports = recorded_exports(&journal).await;
    assert_eq!(exports.len(), 2);
    // AND the first names its format, its filters and the detail
    assert_eq!(
        exports[0],
        json!({
            "event": "journal_exported",
            "format": "csv",
            "entries": 1,
            "actor": "user",
            "filters": {
                "agents": ["quotes", "invoices"],
                "families": ["write_or_delete"],
                "natures": ["action"],
                "results": ["success"],
                "since": "2026-10-01T00:00:00Z",
                "until": "2026-10-02T00:00:00Z",
            },
            "detail": true,
        })
    );
    // AND the second says it was a summary narrowed to nothing, of the six
    // entries and of the first export, itself left out
    assert_eq!(exports[1]["format"], "json");
    assert_eq!(exports[1]["detail"], false);
    assert_eq!(exports[1]["entries"], 7);
    assert_eq!(
        exports[1]["filters"],
        json!({
            "agents": [], "families": [], "natures": [], "results": [],
            "since": null, "until": null,
        })
    );
    let document: Value = serde_json::from_str(&second.body).unwrap();
    assert_eq!(document["entries"][0]["kind"], "journal_exported");
    assert_eq!(document["entries"][0]["what"], "Journal exported");
}

#[tokio::test]
async fn test_a_refused_export_writes_no_entry() {
    // GIVEN a journal that was never exported
    let dir = tempfile::tempdir().unwrap();
    let journal = journal(&dir).await;

    // WHEN an export names a format the journal is not written in, or a
    // filter value that cannot be read, or no format at all
    let format = get(
        &dir,
        Some(journal.clone()),
        "/api/v1/audit/export?format=pdf",
    )
    .await;
    let filter = get(
        &dir,
        Some(journal.clone()),
        "/api/v1/audit/export?format=csv&nature=weather",
    )
    .await;
    let missing = get(&dir, Some(journal.clone()), "/api/v1/audit/export").await;

    // THEN each is refused, the first two by name
    assert_eq!(format.status, StatusCode::BAD_REQUEST);
    assert!(format.body.contains("unknown export format 'pdf'"));
    assert_eq!(filter.status, StatusCode::BAD_REQUEST);
    assert!(filter.body.contains("unknown nature 'weather'"));
    assert_eq!(missing.status, StatusCode::BAD_REQUEST);
    // AND the journal records no export
    assert!(recorded_exports(&journal).await.is_empty());
}

#[tokio::test]
async fn test_the_export_answers_503_without_a_journal() {
    // GIVEN a runtime with no journal
    let dir = tempfile::tempdir().unwrap();

    // WHEN the journal is exported
    let answer = get(&dir, None, "/api/v1/audit/export?format=csv").await;

    // THEN the route says the journal is not available
    assert_eq!(answer.status, StatusCode::SERVICE_UNAVAILABLE);
    assert!(answer.body.contains("audit journal not available"));
}

/// GETs `uri` on a router over `journal`, under a valid organisation policy
/// whose `[journal.export]` section sets `detail`.
async fn get_under_policy(
    dir: &tempfile::TempDir,
    journal: AuditJournalHandle,
    detail: &str,
    uri: &str,
) -> (StatusCode, Value) {
    let policy = apollia_permissions::parse_policy(&format!(
        "schema_version = 1\nsigned_at = 2026-10-02T09:30:00Z\n\
         [journal.export]\ndetail = \"{detail}\"\n"
    ))
    .expect("policy");
    let verdict =
        apollia_permissions::PolicyVerdict::Valid(Box::new(apollia_permissions::ValidPolicy {
            signed_at: policy.signed_at,
            policy,
            key_fingerprint: "ab".repeat(32),
            policy_digest: "00".into(),
            publisher: "it".into(),
        }));
    let mut state = app_state(
        Arc::new(StemLoader { refusal: None }),
        dir.path().to_path_buf(),
    );
    state.audit_journal = Some(journal);
    state.inference_governance = crate::governance::InferenceGovernance::new(Arc::new(verdict));
    crate::api::test_support::call(state, "GET", uri, None).await
}

#[tokio::test]
async fn test_a_policy_that_forbids_the_detail_refuses_it_and_leaves_the_summary() {
    // GIVEN a journal under a policy that forbids the detail in an export
    let dir = tempfile::tempdir().unwrap();
    let journal = journal(&dir).await;

    // WHEN the export is asked with the detail, then without
    let (with, refused) = get_under_policy(
        &dir,
        journal.clone(),
        "forbid",
        "/api/v1/audit/export?format=json&detail=true",
    )
    .await;
    let (without, document) = get_under_policy(
        &dir,
        journal.clone(),
        "forbid",
        "/api/v1/audit/export?format=json",
    )
    .await;

    // THEN the detail is refused with the reason, and no content left
    assert_eq!(with, StatusCode::FORBIDDEN, "{refused}");
    assert_eq!(refused["error"], super::DETAIL_FORBIDDEN_REASON);
    // AND the summary is served
    assert_eq!(without, StatusCode::OK);
    assert_eq!(document["detail"], false);
    // AND only the export that was written is recorded
    let exports = recorded_exports(&journal).await;
    assert_eq!(exports.len(), 1);
    assert_eq!(exports[0]["detail"], false);
    journal.shutdown().await;
}

#[tokio::test]
async fn test_a_policy_that_requires_the_detail_refuses_an_export_without_it() {
    // GIVEN a journal under a policy that requires the detail in an export
    let dir = tempfile::tempdir().unwrap();
    let journal = journal(&dir).await;

    // WHEN the export is asked without the detail, then with
    let (without, refused) = get_under_policy(
        &dir,
        journal.clone(),
        "require",
        "/api/v1/audit/export?format=json",
    )
    .await;
    let (with, document) = get_under_policy(
        &dir,
        journal.clone(),
        "require",
        "/api/v1/audit/export?format=json&detail=true",
    )
    .await;

    // THEN the summary alone is refused with the reason
    assert_eq!(without, StatusCode::FORBIDDEN, "{refused}");
    assert_eq!(refused["error"], super::DETAIL_REQUIRED_REASON);
    // AND the detail is served and recorded
    assert_eq!(with, StatusCode::OK);
    assert_eq!(document["detail"], true);
    let exports = recorded_exports(&journal).await;
    assert_eq!(exports.len(), 1);
    assert_eq!(exports[0]["detail"], true);
    journal.shutdown().await;
}
