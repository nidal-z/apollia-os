//! Tauri IPC commands behind the journal screen.
//!
//! The screen reads the chained journal the way the command line does, through
//! two runtime routes, so it shows the same entries whether the runtime runs in
//! this process or as a daemon:
//!
//! - `list_journal_entries`: `GET /api/v1/audit/journal`, a page of entries,
//!   newest first, each with its summary. The filters travel in the query and
//!   are applied by the route, before paging.
//! - `get_journal_entry`: `GET /api/v1/audit/journal/{run_id}/entries/{seq}`,
//!   one entry with the arguments and the output of its tool call.
//! - `verify_journal`: `GET /api/v1/audit/verify`, the verification of the
//!   whole chain, as the route reports it.
//! - `export_journal`: `GET /api/v1/audit/export?format=csv`, then the file
//!   written where the user chose in the save dialog.
//! - `read_journal_export`: `GET /api/v1/audit/export?format=json`, the same
//!   export as a document, which the screen lays out as a page to print.
//! - `print_journal`: the print dialog of the system on this window, where
//!   that page is saved as a PDF. It reads nothing from the runtime.
//!
//! An entry the journal no longer holds answers an empty value rather than an
//! error: the screen says so in place of a detail.

use std::path::Path;

use serde::Deserialize;
use serde_json::{json, Value};
use tauri::State;

use crate::attached::{segment, RuntimeLink};
use crate::commands::agent_sheet::get_optional;

/// Entries a page holds when the screen names no size.
const DEFAULT_PAGE_SIZE: u32 = 50;

/// Route of the verification of the whole journal.
const VERIFY_ROUTE: &str = "/api/v1/audit/verify";

/// Route of the export of the journal.
const EXPORT_ROUTE: &str = "/api/v1/audit/export";

/// What the screen narrows a page of the journal to. Within one criterion the
/// values are alternatives; the criteria add up. An empty criterion keeps
/// everything.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct JournalQuery {
    /// Agent names.
    pub agents: Vec<String>,
    /// Action families, under their wire names.
    pub families: Vec<String>,
    /// Natures: `action`, `access_change`, `policy`, `system`.
    pub natures: Vec<String>,
    /// Results: `success`, `failure`, `refused`.
    pub results: Vec<String>,
    /// Start of the period, included: a day or an RFC 3339 instant.
    pub since: Option<String>,
    /// End of the period, excluded: a day or an RFC 3339 instant.
    pub until: Option<String>,
    /// Entries in the page.
    pub limit: Option<u32>,
    /// Matching entries to skip, newest first.
    pub offset: Option<u32>,
}

/// The filters of `query` as query parameters, each criterion in its own,
/// every one led by `&`. Empty when `query` narrows nothing.
fn filter_parameters(query: &JournalQuery) -> String {
    let mut parameters = String::new();
    let lists = [
        ("agents", &query.agents),
        ("family", &query.families),
        ("nature", &query.natures),
        ("result", &query.results),
    ];
    for (name, values) in lists {
        let values: Vec<&str> = values
            .iter()
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
            .collect();
        if !values.is_empty() {
            parameters.push_str(&format!("&{name}={}", segment(&values.join(","))));
        }
    }
    let bounds = [("since", &query.since), ("until", &query.until)];
    for (name, bound) in bounds {
        if let Some(bound) = bound.as_deref().map(str::trim).filter(|b| !b.is_empty()) {
            parameters.push_str(&format!("&{name}={}", segment(bound)));
        }
    }
    parameters
}

/// Route of the page `query` describes, each criterion in its query parameter.
fn page_path(query: &JournalQuery) -> String {
    format!(
        "/api/v1/audit/journal?limit={}&offset={}{}",
        query.limit.unwrap_or(DEFAULT_PAGE_SIZE),
        query.offset.unwrap_or(0),
        filter_parameters(query)
    )
}

/// Route of the export of the entries `query` narrows, as `format`, with or
/// without their detail. An export is never paged: the bounds of `query` are
/// left out.
fn export_path(query: &JournalQuery, format: &str, detail: bool) -> String {
    let detail = if detail { "&detail=true" } else { "" };
    format!(
        "{EXPORT_ROUTE}?format={format}{detail}{}",
        filter_parameters(query)
    )
}

/// Route of the detail of the entry `seq` of `run_id`.
fn entry_path(run_id: &str, seq: u64) -> String {
    format!("/api/v1/audit/journal/{}/entries/{seq}", segment(run_id))
}

pub(crate) async fn entries_on(
    link: &RuntimeLink,
    query: &JournalQuery,
) -> Result<Vec<Value>, String> {
    let page = link.get_json(&page_path(query)).await?;
    Ok(page
        .get("entries")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default())
}

pub(crate) async fn entry_on(
    link: &RuntimeLink,
    run_id: &str,
    seq: u64,
) -> Result<Option<Value>, String> {
    let body = get_optional(link, &entry_path(run_id, seq)).await?;
    Ok(body.and_then(|body| body.get("entry").cloned()))
}

pub(crate) async fn verify_on(link: &RuntimeLink) -> Result<Value, String> {
    link.get_json(VERIFY_ROUTE).await
}

pub(crate) async fn export_on(
    link: &RuntimeLink,
    query: &JournalQuery,
    detail: bool,
    target: &Path,
) -> Result<Value, String> {
    let bytes = link
        .get_for_file(&export_path(query, "csv", detail))
        .await?;
    std::fs::write(target, &bytes).map_err(|e| format!("{}: {e}", target.display()))?;
    tracing::event!(
        tracing::Level::INFO,
        detail = detail,
        bytes = bytes.len(),
        "journal.export.written"
    );
    Ok(json!({ "path": target.display().to_string(), "bytes": bytes.len() }))
}

pub(crate) async fn export_document_on(
    link: &RuntimeLink,
    query: &JournalQuery,
    detail: bool,
) -> Result<Value, String> {
    link.get_json(&export_path(query, "json", detail)).await
}

/// A page of the journal narrowed by `query`, newest first. Each entry is the
/// chained entry with its `summary`: agent, nature, family, what happened,
/// tool, target, outcome and date, without content.
#[tauri::command]
pub async fn list_journal_entries(
    link: State<'_, RuntimeLink>,
    query: JournalQuery,
) -> Result<Vec<Value>, String> {
    entries_on(link.inner(), &query).await
}

/// The entry `seq` of `run_id` with everything it records: its summary and,
/// when it is about a tool call, the full arguments and the full output of
/// that call. `null` when the journal holds no such entry.
#[tauri::command]
pub async fn get_journal_entry(
    link: State<'_, RuntimeLink>,
    run_id: String,
    seq: u64,
) -> Result<Option<Value>, String> {
    entry_on(link.inner(), &run_id, seq).await
}

/// The verification of the whole journal: whether the chain holds, how many
/// entries were read, the first broken link when there is one, and where the
/// verification started when a purge removed the beginning.
#[tauri::command]
pub async fn verify_journal(link: State<'_, RuntimeLink>) -> Result<Value, String> {
    verify_on(link.inner()).await
}

/// Writes the entries `query` narrows to `path` as a CSV file, with their
/// detail when `detail` is set, and answers the file written and its size.
/// A refusal of the runtime, such as a detail the organisation policy
/// forbids, comes back as its message and nothing is written.
#[tauri::command]
pub async fn export_journal(
    link: State<'_, RuntimeLink>,
    query: JournalQuery,
    detail: bool,
    path: String,
) -> Result<Value, String> {
    export_on(link.inner(), &query, detail, Path::new(&path)).await
}

/// The export of the entries `query` narrows as a document: `detail`, `count`
/// and `entries`. The screen lays it out as the page the system prints.
#[tauri::command]
pub async fn read_journal_export(
    link: State<'_, RuntimeLink>,
    query: JournalQuery,
    detail: bool,
) -> Result<Value, String> {
    export_document_on(link.inner(), &query, detail).await
}

/// Opens the print dialog of the system on this window. The page it prints
/// is the one the screen laid out; saving it as a PDF is a choice of that
/// dialog.
#[tauri::command]
pub fn print_journal(window: tauri::WebviewWindow) -> Result<(), String> {
    window.print().map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use wiremock::matchers::{header, method, path, query_param, query_param_is_missing};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::attached::{gate, AttachedClient, LinkMode};

    const TOKEN: &str = "token-of-this-installation";

    fn attached(server: &MockServer) -> RuntimeLink {
        RuntimeLink::Attached(AttachedClient::new(server.uri(), TOKEN).unwrap())
    }

    #[test]
    fn test_every_filter_travels_in_the_query_of_the_route() {
        // GIVEN a query naming two agents, a family, two natures, a result and a period
        let query = JournalQuery {
            agents: vec!["mail sorter".to_owned(), "watch".to_owned()],
            families: vec!["send_external".to_owned()],
            natures: vec!["action".to_owned(), "policy".to_owned()],
            results: vec!["refused".to_owned()],
            since: Some("2026-10-01".to_owned()),
            until: Some("2026-10-03T00:00:00+02:00".to_owned()),
            limit: Some(20),
            offset: Some(40),
        };

        // WHEN the route of its page is written
        let route = page_path(&query);

        // THEN each criterion is in its parameter, encoded, values joined by a comma
        assert_eq!(
            route,
            "/api/v1/audit/journal?limit=20&offset=40\
             &agents=mail%20sorter%2Cwatch&family=send_external\
             &nature=action%2Cpolicy&result=refused\
             &since=2026-10-01&until=2026-10-03T00%3A00%3A00%2B02%3A00"
        );
    }

    #[test]
    fn test_an_empty_criterion_leaves_its_parameter_out() {
        // GIVEN a query with no criterion, and one whose values are blank
        let none = JournalQuery::default();
        let blank = JournalQuery {
            agents: vec!["  ".to_owned()],
            since: Some(String::new()),
            ..JournalQuery::default()
        };

        // WHEN their routes are written
        // THEN only the page bounds remain
        assert_eq!(page_path(&none), "/api/v1/audit/journal?limit=50&offset=0");
        assert_eq!(page_path(&blank), "/api/v1/audit/journal?limit=50&offset=0");
    }

    #[tokio::test]
    async fn test_the_screen_reads_a_filtered_page_from_a_daemon() {
        // GIVEN a daemon whose journal answers one refused call for the agent `watch`
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/audit/journal"))
            .and(query_param("agents", "watch"))
            .and(query_param("result", "refused"))
            .and(query_param("limit", "50"))
            .and(query_param_is_missing("family"))
            .and(header("authorization", format!("Bearer {TOKEN}").as_str()))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "entries": [{
                    "seq": 7, "run_id": "run-1", "ts": "2026-10-02T12:00:00Z",
                    "kind": "rule_refused", "payload": {},
                    "summary": {
                        "agent": "watch", "nature": "action", "family": "send_external",
                        "what": "Call refused by a rule", "tool": "http_fetch",
                        "target": null, "outcome": "refused", "ts": "2026-10-02T12:00:00Z",
                    },
                }],
                "count": 1,
            })))
            .expect(1)
            .mount(&server)
            .await;
        let query = JournalQuery {
            agents: vec!["watch".to_owned()],
            results: vec!["refused".to_owned()],
            ..JournalQuery::default()
        };

        // WHEN the screen lists the page
        let entries = entries_on(&attached(&server), &query).await.unwrap();

        // THEN the entry comes back with its summary, as the route wrote it
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0]["summary"]["outcome"], "refused");
        assert_eq!(entries[0]["summary"]["agent"], "watch");
    }

    #[tokio::test]
    async fn test_the_detail_of_an_entry_carries_the_arguments_of_its_call() {
        // GIVEN a daemon holding the entry 3 of `run 1`, a tool call with its arguments
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/audit/journal/run%201/entries/3"))
            .and(header("authorization", format!("Bearer {TOKEN}").as_str()))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "entry": {
                    "seq": 3, "run_id": "run 1", "kind": "tool_call_completed",
                    "summary": { "tool": "file_read", "target": "notes.md" },
                    "tool_call": {
                        "tool_call_id": "call-1",
                        "arguments": { "path": "notes.md" },
                        "output": { "content": "hello" },
                        "status": "success",
                    },
                },
            })))
            .expect(1)
            .mount(&server)
            .await;

        // WHEN the screen asks for the detail of that entry
        let entry = entry_on(&attached(&server), "run 1", 3).await.unwrap();

        // THEN the arguments and the output of the call come with it
        let entry = entry.expect("the entry is held");
        assert_eq!(entry["tool_call"]["arguments"]["path"], "notes.md");
        assert_eq!(entry["tool_call"]["output"]["content"], "hello");
    }

    #[tokio::test]
    async fn test_an_entry_the_journal_does_not_hold_reads_as_absent() {
        // GIVEN a daemon whose journal has no entry 9 in `run-1`
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/audit/journal/run-1/entries/9"))
            .respond_with(
                ResponseTemplate::new(404)
                    .set_body_json(serde_json::json!({ "error": "not_found" })),
            )
            .expect(1)
            .mount(&server)
            .await;

        // WHEN the screen asks for its detail
        let entry = entry_on(&attached(&server), "run-1", 9).await.unwrap();

        // THEN it is absent rather than an error
        assert!(entry.is_none());
    }

    #[test]
    fn test_an_export_carries_the_filters_and_never_the_page_bounds() {
        // GIVEN a query naming an agent, a result and a period, on its third page
        let query = JournalQuery {
            agents: vec!["mail sorter".to_owned()],
            results: vec!["refused".to_owned()],
            since: Some("2026-10-01".to_owned()),
            limit: Some(50),
            offset: Some(100),
            ..JournalQuery::default()
        };

        // WHEN the routes of its export are written, with and without the detail
        let summary = export_path(&query, "csv", false);
        let detailed = export_path(&query, "json", true);

        // THEN the filters travel, the detail only when asked, and no page bound
        assert_eq!(
            summary,
            "/api/v1/audit/export?format=csv&agents=mail%20sorter&result=refused&since=2026-10-01"
        );
        assert_eq!(
            detailed,
            "/api/v1/audit/export?format=json&detail=true\
             &agents=mail%20sorter&result=refused&since=2026-10-01"
        );
    }

    #[tokio::test]
    async fn test_the_screen_reads_the_verification_of_the_whole_journal() {
        // GIVEN a daemon whose journal breaks at the entry 217, after a purge
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/audit/verify"))
            .and(header("authorization", format!("Bearer {TOKEN}").as_str()))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "ok": false, "entries_checked": 216, "runs_checked": 4,
                "first_break": {
                    "global_seq": 217, "run_id": "run-9", "reason": "global_hash_mismatch",
                },
                "head_matches_state": true,
                "start": {
                    "global_seq": 12, "ts": "2026-09-12T08:00:00Z",
                    "purged_at": "2026-10-01T00:00:00Z",
                },
            })))
            .expect(1)
            .mount(&server)
            .await;

        // WHEN the screen asks for the verification
        let report = verify_on(&attached(&server)).await.unwrap();

        // THEN the report comes back as the route wrote it, break and start included
        assert_eq!(report["ok"], false);
        assert_eq!(report["first_break"]["global_seq"], 217);
        assert_eq!(report["start"]["global_seq"], 12);
    }

    #[tokio::test]
    async fn test_the_export_is_written_where_the_user_chose() {
        // GIVEN a daemon answering the export of the refused calls, detail included
        let server = MockServer::start().await;
        let body = "ts,agent,nature\n2026-10-02T12:00:00Z,watch,action\n";
        Mock::given(method("GET"))
            .and(path("/api/v1/audit/export"))
            .and(query_param("format", "csv"))
            .and(query_param("detail", "true"))
            .and(query_param("result", "refused"))
            .and(query_param_is_missing("limit"))
            .respond_with(ResponseTemplate::new(200).set_body_string(body))
            .expect(1)
            .mount(&server)
            .await;
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("journal.csv");
        let query = JournalQuery {
            results: vec!["refused".to_owned()],
            limit: Some(50),
            ..JournalQuery::default()
        };

        // WHEN the screen exports to that file
        let written = export_on(&attached(&server), &query, true, &target)
            .await
            .unwrap();

        // THEN the file holds what the route answered, and its size is reported
        assert_eq!(std::fs::read_to_string(&target).unwrap(), body);
        assert_eq!(written["bytes"], body.len());
    }

    #[tokio::test]
    async fn test_a_detail_the_policy_forbids_writes_no_file() {
        // GIVEN a daemon whose policy forbids the detail in an export
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/audit/export"))
            .respond_with(ResponseTemplate::new(403).set_body_json(
                serde_json::json!({ "error": "the organisation policy forbids the detail" }),
            ))
            .mount(&server)
            .await;
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("journal.csv");

        // WHEN the screen asks for an export with the detail
        let refused = export_on(&attached(&server), &JournalQuery::default(), true, &target).await;

        // THEN the refusal comes back with its reason and nothing is written
        assert!(refused.unwrap_err().contains("forbids the detail"));
        assert!(!target.exists());
    }

    #[tokio::test]
    async fn test_the_page_to_print_is_read_as_the_export_document() {
        // GIVEN a daemon answering the export as a document of one entry
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/audit/export"))
            .and(query_param("format", "json"))
            .and(query_param_is_missing("detail"))
            .and(query_param("agents", "watch"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "detail": false, "count": 1,
                "entries": [{ "run_id": "run-1", "seq": 7, "what": "Call refused by a rule" }],
            })))
            .expect(1)
            .mount(&server)
            .await;
        let query = JournalQuery {
            agents: vec!["watch".to_owned()],
            ..JournalQuery::default()
        };

        // WHEN the screen reads the export to lay it out
        let document = export_document_on(&attached(&server), &query, false)
            .await
            .unwrap();

        // THEN the document carries its entries
        assert_eq!(document["count"], 1);
        assert_eq!(document["entries"][0]["seq"], 7);
    }

    #[test]
    fn test_the_journal_commands_are_routed_to_a_daemon() {
        // GIVEN the commands of the journal screen
        let commands = [
            "list_journal_entries",
            "get_journal_entry",
            "verify_journal",
            "export_journal",
            "read_journal_export",
            "print_journal",
        ];

        // WHEN each passes the gate of an attached window
        // THEN none is refused
        for command in commands {
            assert!(gate(LinkMode::Attached, command).is_ok(), "{command}");
        }
    }
}
