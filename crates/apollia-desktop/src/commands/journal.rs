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
//!
//! An entry the journal no longer holds answers an empty value rather than an
//! error: the screen says so in place of a detail.

use serde::Deserialize;
use serde_json::Value;
use tauri::State;

use crate::attached::{segment, RuntimeLink};
use crate::commands::agent_sheet::get_optional;

/// Entries a page holds when the screen names no size.
const DEFAULT_PAGE_SIZE: u32 = 50;

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

/// Route of the page `query` describes, each criterion in its query parameter.
fn page_path(query: &JournalQuery) -> String {
    let mut path = format!(
        "/api/v1/audit/journal?limit={}&offset={}",
        query.limit.unwrap_or(DEFAULT_PAGE_SIZE),
        query.offset.unwrap_or(0)
    );
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
            path.push_str(&format!("&{name}={}", segment(&values.join(","))));
        }
    }
    let bounds = [("since", &query.since), ("until", &query.until)];
    for (name, bound) in bounds {
        if let Some(bound) = bound.as_deref().map(str::trim).filter(|b| !b.is_empty()) {
            path.push_str(&format!("&{name}={}", segment(bound)));
        }
    }
    path
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
    fn test_the_journal_commands_are_routed_to_a_daemon() {
        // GIVEN the two commands of the journal screen
        let commands = ["list_journal_entries", "get_journal_entry"];

        // WHEN each passes the gate of an attached window
        // THEN none is refused
        for command in commands {
            assert!(gate(LinkMode::Attached, command).is_ok(), "{command}");
        }
    }
}
