use super::*;

use apollia_mcp::{ConnectionOrigin, Provenance};
use apollia_tools::ActionFamily;

use crate::governance::{ConnectionAdmission, PolicyReadError};

/// A policy that refuses every remote connection.
struct RefuseRemote;

impl ConnectionAdmission for RefuseRemote {
    fn read_decision(
        &self,
        request: &AdmissionRequest,
    ) -> Result<AdmissionDecision, PolicyReadError> {
        Ok(if request.remote {
            AdmissionDecision::Refused {
                reason: "remote connections are not allowed".into(),
            }
        } else {
            AdmissionDecision::Admitted
        })
    }
}

fn entry(id: &str, label: &str, description: &str, remote: bool) -> SelectionEntry {
    let (transport, url) = if remote {
        ("streamable-http", Some("https://mcp.example.com/mcp"))
    } else {
        ("stdio", None)
    };
    SelectionEntry {
        id: id.into(),
        label: label.into(),
        description: description.into(),
        keywords: vec!["productivity".into()],
        admission: AdmissionRequest::for_selection_entry(
            id,
            transport,
            url,
            None,
            [ActionFamily::Read],
        ),
    }
}

fn selection(governance: ConnectionGovernance) -> ConnectionSelection {
    ConnectionSelection::new(
        vec![
            entry(
                "@notionhq/notion-mcp-server",
                "Notion",
                "Read and write Notion pages and databases.",
                true,
            ),
            entry(
                "@acme/calendar-mcp",
                "Acme Calendar",
                "List and create calendar events.",
                false,
            ),
        ],
        governance,
    )
}

fn server_from(name: &str, entry: &str) -> McpServerConfig {
    let mut config: McpServerConfig = serde_json::from_value(serde_json::json!({
        "name": name,
        "transport": "streamable-http",
        "url": "https://mcp.example.com/mcp",
    }))
    .expect("config");
    config.origin = Some(ConnectionOrigin {
        provenance: Provenance::Selection,
        official: true,
        entry: Some(entry.into()),
    });
    config
}

// An entry no connection was made from reads as not connected, named by its
// entry.
#[test]
fn test_an_entry_not_connected_is_answered_with_its_entry() {
    // GIVEN the selection, no server configured and nothing granted
    let selection = selection(ConnectionGovernance::default());

    // WHEN Apollia searches for a calendar
    let found = lookup(&selection, "calendar events", 5, &[], &[]);

    // THEN the calendar entry is the one found, not connected, with its entry
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].entry, "@acme/calendar-mcp");
    assert_eq!(found[0].status, EntryStatus::NotConnected);
    assert_eq!(found[0].summary, "not connected: @acme/calendar-mcp");
}

// A connection made from the entry and granted to no tool reads as connected
// but not granted; once a tool of it is granted, as granted.
#[test]
fn test_a_connected_entry_says_whether_it_is_granted() {
    // GIVEN a Notion server made from its entry
    let selection = selection(ConnectionGovernance::default());
    let connected = vec![server_from("notion", "@notionhq/notion-mcp-server")];

    // WHEN an agent granted no Notion tool searches, then one granted a tool
    let not_granted = lookup(&selection, "notion", 5, &connected, &["file_read".into()]);
    let granted = lookup(
        &selection,
        "notion",
        5,
        &connected,
        &["mcp:notion/notion-search".into()],
    );

    // THEN the first reads as connected but not granted, the second as granted,
    // both naming the entry and the connection
    assert_eq!(not_granted[0].entry, "@notionhq/notion-mcp-server");
    assert_eq!(
        not_granted[0].status,
        EntryStatus::ConnectedNotGranted {
            connection: "mcp:notion".into()
        }
    );
    assert_eq!(
        not_granted[0].summary,
        "connected but not granted to you: mcp:notion"
    );
    assert_eq!(
        granted[0].status,
        EntryStatus::Granted {
            connection: "mcp:notion".into()
        }
    );
}

// An entry the organisation policy would refuse to add is never offered.
#[test]
fn test_an_entry_the_policy_forbids_is_not_offered() {
    // GIVEN a policy refusing remote connections, Notion being remote
    let selection = selection(ConnectionGovernance::new(Arc::new(RefuseRemote), None));

    // WHEN Apollia searches for Notion, then for what both entries hold
    let notion = lookup(&selection, "notion", 5, &[], &[]);
    let both = lookup(&selection, "productivity", 5, &[], &[]);

    // THEN Notion is never answered, the local calendar still is
    assert!(notion.is_empty(), "{notion:?}");
    assert_eq!(both.len(), 1, "{both:?}");
    assert_eq!(both[0].entry, "@acme/calendar-mcp");
}

// The result the model reads carries the entry the add flow opens.
#[tokio::test]
async fn test_the_tool_result_carries_the_entry_and_its_status() {
    // GIVEN the executor over the selection, on a data directory with no
    // `mcp.db`
    let data_dir = tempfile::tempdir().expect("tempdir");
    let mcp_db = data_dir.path().join("mcp.db");
    let executor = ConnectionLookupExecutor::new(
        Arc::new(selection(ConnectionGovernance::default())),
        mcp_db.clone(),
        Vec::new(),
    );

    // WHEN the model calls it
    let out = executor
        .execute(serde_json::json!({ "query": "notion" }))
        .await
        .expect("lookup");

    // THEN the match names its entry and reads as not connected, and the
    // search created no file
    assert_eq!(out["total"], 1);
    assert_eq!(out["matches"][0]["entry"], "@notionhq/notion-mcp-server");
    assert_eq!(out["matches"][0]["status"], "not_connected");
    assert_eq!(
        out["matches"][0]["summary"],
        "not connected: @notionhq/notion-mcp-server"
    );
    assert!(!mcp_db.exists());
}

// The servers configured are read from `mcp.db` at each call.
#[tokio::test]
async fn test_the_tool_reads_the_servers_configured_on_this_machine() {
    // GIVEN a Notion server made from its entry, saved in `mcp.db`
    let data_dir = tempfile::tempdir().expect("tempdir");
    let mcp_db = data_dir.path().join("mcp.db");
    McpServerRepository::open(&mcp_db)
        .expect("open")
        .save(&server_from("notion", "@notionhq/notion-mcp-server"))
        .expect("save");
    let executor = ConnectionLookupExecutor::new(
        Arc::new(selection(ConnectionGovernance::default())),
        mcp_db,
        vec!["file_read".into()],
    );

    // WHEN the model calls it
    let out = executor
        .execute(serde_json::json!({ "query": "notion pages", "limit": 3 }))
        .await
        .expect("lookup");

    // THEN Notion reads as connected but not granted, with its connection
    assert_eq!(out["matches"][0]["status"], "connected_not_granted");
    assert_eq!(out["matches"][0]["connection"], "mcp:notion");
}

// A query with no word to search, or a limit out of bounds, is refused.
#[tokio::test]
async fn test_an_empty_query_or_a_bad_limit_is_refused() {
    // GIVEN the executor
    let executor = ConnectionLookupExecutor::new(
        Arc::new(selection(ConnectionGovernance::default())),
        PathBuf::from("/nonexistent/mcp.db"),
        Vec::new(),
    );

    // WHEN it is called with no query, then with a limit of zero
    let empty = executor.execute(serde_json::json!({ "query": " " })).await;
    let zero = executor
        .execute(serde_json::json!({ "query": "notion", "limit": 0 }))
        .await;

    // THEN both are refused as invalid input
    assert!(matches!(
        empty,
        Err(ToolExecutionError::InvalidInput { .. })
    ));
    assert!(matches!(zero, Err(ToolExecutionError::InvalidInput { .. })));
}

// The selection is searched by Apollia alone.
#[test]
fn test_only_apollia_is_offered_the_selection() {
    // GIVEN a selection handed over
    let handed = Some(Arc::new(selection(ConnectionGovernance::default())));

    // WHEN an exchange of Apollia, then of an installed agent, takes it
    let apollia = offered_to(handed.clone(), crate::chat::APOLLIA_CHAT_AGENT_ID);
    let other = offered_to(handed, "ledger-clerk");

    // THEN Apollia has it, the other agent does not
    assert!(apollia.is_some());
    assert!(other.is_none());
}
