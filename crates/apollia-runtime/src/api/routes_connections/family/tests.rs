//! Tests of the reclassification route: what is refused, stored, taken in
//! by the live manager, kept across a new addition, and journaled.

use std::sync::{Arc, Mutex};

use apollia_mcp::config::McpServerConfig;
use apollia_mcp::manager::McpClientManagerHandle;
use apollia_mcp::session::LoadingMode;
use apollia_mcp::{ConnectionOrigin, McpServerRepository};
use apollia_tools::ToolRegistryHandle;
use serde_json::json;

use super::*;
use crate::api::routes_llm::tests::{test_app_state_no_llm, MockBackend};
use crate::audit_journal::{AuditJournalHandle, JournalEntryKind};
use crate::governance::{connection_run_id, ConnectionGovernance};

/// A server the annotated mock MCP script answers for: `lookup` declared
/// read-only, `purge` destructive, `ping` without a hint.
fn annotated(name: &str) -> McpServerConfig {
    serde_json::from_value(json!({
        "name": name,
        "command": "python3",
        "args": [concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../apollia-mcp/tests/mock_mcp_server_annotated.py"
        )],
        "init_timeout_secs": 10,
        "call_timeout_secs": 10,
    }))
    .unwrap()
}

struct Harness {
    _dir: tempfile::TempDir,
    state: AppState<MockBackend>,
    registry: ToolRegistryHandle,
    manager: McpClientManagerHandle,
    journal: AuditJournalHandle,
}

/// A runtime whose live manager runs `servers` in eager loading, each saved
/// in `mcp.db` with the tools it published, with connection governance over
/// a journal.
async fn harness(servers: Vec<McpServerConfig>) -> Harness {
    harness_in(servers, LoadingMode::Eager).await
}

/// [`harness`], in the loading `mode`.
async fn harness_in(servers: Vec<McpServerConfig>, mode: LoadingMode) -> Harness {
    let dir = tempfile::TempDir::new().unwrap();
    let repo = McpServerRepository::open(&dir.path().join("mcp.db")).unwrap();
    let registry = ToolRegistryHandle::start();
    let manager = McpClientManagerHandle::start_keeping_tools(
        vec![],
        &registry,
        None,
        mode,
        apollia_mcp::tool_store::ToolIndexStore::new(dir.path().join("mcp.db")),
    )
    .await
    .unwrap();
    for server in servers {
        repo.save(&server).unwrap();
        manager.add_server(server).await.unwrap();
    }
    let journal = AuditJournalHandle::open(&dir.path().join("journal.db"))
        .await
        .unwrap();
    let mut state = test_app_state_no_llm();
    state.data_dir = dir.path().to_path_buf();
    state.tool_registry_handle = Some(registry.clone());
    state.mcp_handle = Some(manager.clone());
    state.mcp_server_repo = Some(Arc::new(Mutex::new(repo)));
    state.connection_governance = ConnectionGovernance::without_policy(Some(journal.clone()));
    state.governance = Some(crate::governance::GovernanceHandle::spawn(
        crate::governance::GovernanceSources {
            governance_db: Some(apollia_core::paths::DataFile::Governance.path(dir.path())),
            config_path: None,
            policy: Arc::new(apollia_permissions::PolicyVerdict::Absent),
            journal: None,
            events: None,
            forbidden_connections: Default::default(),
        },
    ));
    Harness {
        _dir: dir,
        state,
        registry,
        manager,
        journal,
    }
}

fn tool<'a>(connection: &'a Connection, id: &str) -> &'a crate::connections::ConnectionTool {
    connection.tools.iter().find(|t| t.id == id).unwrap()
}

fn stored(h: &Harness, server: &str) -> std::collections::BTreeMap<String, ActionFamily> {
    let repo = h.state.mcp_server_repo.as_ref().unwrap().lock().unwrap();
    repo.operator_families(server).unwrap()
}

// A tool the server declares read-only is classified by the operator: the
// family is stored, wins over the hint at once in the registry and in the
// list, and the change is journaled with both families.
#[tokio::test]
async fn test_the_operator_family_wins_over_the_hints_and_is_journaled() {
    // GIVEN a custom server whose `lookup` its hints classify as reading
    let h = harness(vec![annotated("files")]).await;
    let before = h.registry.get("mcp:files/lookup").await.unwrap().unwrap();
    assert_eq!(before.family, ActionFamily::Read);
    assert_eq!(before.family_source, FamilySource::Server);

    // WHEN the operator gives it the family of a write
    let connection = reclassify(
        &h.state,
        Vec::new(),
        "mcp:files",
        "lookup",
        ActionFamily::WriteOrDelete,
    )
    .await
    .unwrap();

    // THEN the list shows the operator's family, from the operator
    let listed = tool(&connection, "mcp:files/lookup");
    assert_eq!(listed.family, ActionFamily::WriteOrDelete);
    assert_eq!(listed.family_source, FamilySource::Operator);
    // AND the registry the calls are decided on holds it, without a restart
    let after = h.registry.get("mcp:files/lookup").await.unwrap().unwrap();
    assert_eq!(after.family, ActionFamily::WriteOrDelete);
    assert_eq!(after.family_source, FamilySource::Operator);
    // AND the other tools keep the family their hints give
    let purge = h.registry.get("mcp:files/purge").await.unwrap().unwrap();
    assert_eq!(purge.family_source, FamilySource::Server);
    // AND the family is stored apart from the server row
    assert_eq!(
        stored(&h, "files"),
        [("lookup".to_string(), ActionFamily::WriteOrDelete)].into()
    );
    // AND the connection's run records the change, from reading to writing
    let entries = h.journal.query_run(&connection_run_id("mcp:files")).await;
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].kind, JournalEntryKind::ToolReclassified);
    assert_eq!(entries[0].payload["connection_id"], "mcp:files");
    assert_eq!(entries[0].payload["tool"], "lookup");
    assert_eq!(entries[0].payload["from"], "read");
    assert_eq!(entries[0].payload["to"], "write_or_delete");

    h.manager.shutdown().await;
    h.journal.shutdown().await;
}

// The operator's family survives the connection added again, whose
// selection classification is rewritten at each addition. In deferred
// loading, the index of the new session is what the calls are decided on.
#[tokio::test]
async fn test_the_operator_family_survives_the_connection_added_again() {
    // GIVEN a custom server whose `ping` the operator classified
    let h = harness_in(vec![annotated("files")], LoadingMode::Deferred).await;
    reclassify(
        &h.state,
        Vec::new(),
        "mcp:files",
        "ping",
        ActionFamily::SendExternal,
    )
    .await
    .unwrap();

    // WHEN the server is removed from the manager and added again from a
    //      configuration that carries no operator family
    h.manager.remove_server("files").await.unwrap();
    assert!(h.manager.get_tool_index().await.is_empty());
    crate::api::routes_mcp::add_server_to(&h.state, annotated("files"))
        .await
        .unwrap();

    // THEN the new session classifies the tool in the operator's family
    let index = h.manager.get_tool_index().await;
    let ping = index.iter().find(|e| e.tool_name == "ping").unwrap();
    assert_eq!(ping.family, ActionFamily::SendExternal);
    assert_eq!(ping.family_source, FamilySource::Operator);
    assert_eq!(
        stored(&h, "files"),
        [("ping".to_string(), ActionFamily::SendExternal)].into()
    );

    h.manager.shutdown().await;
    h.journal.shutdown().await;
}

// A connection of the selection is not reclassified: refused, nothing stored,
// nothing journaled.
#[tokio::test]
async fn test_a_connection_of_the_selection_is_refused() {
    // GIVEN a server added from the selection
    let mut curated = annotated("curated");
    curated.origin = Some(ConnectionOrigin {
        provenance: Provenance::Selection,
        official: false,
        entry: None,
    });
    let h = harness(vec![curated]).await;

    // WHEN the operator tries to reclassify one of its tools
    let (status, _) = reclassify(
        &h.state,
        Vec::new(),
        "mcp:curated",
        "lookup",
        ActionFamily::WriteOrDelete,
    )
    .await
    .unwrap_err();

    // THEN it is refused, the registry keeps its family, and nothing is
    //      stored or journaled
    assert_eq!(status, StatusCode::CONFLICT);
    let lookup = h.registry.get("mcp:curated/lookup").await.unwrap().unwrap();
    assert_eq!(lookup.family_source, FamilySource::Server);
    assert!(stored(&h, "curated").is_empty());
    assert!(h
        .journal
        .query_run(&connection_run_id("mcp:curated"))
        .await
        .is_empty());

    h.manager.shutdown().await;
    h.journal.shutdown().await;
}

// A family nobody can give, a tool the server does not publish, and a
// connection nobody added are refused before anything is written.
#[tokio::test]
async fn test_an_exempt_family_an_unknown_tool_and_an_unknown_connection_are_refused() {
    // GIVEN a custom server
    let h = harness(vec![annotated("files")]).await;

    // WHEN each is attempted
    let exempt = reclassify(
        &h.state,
        Vec::new(),
        "mcp:files",
        "lookup",
        ActionFamily::Exempt,
    )
    .await
    .unwrap_err();
    let unknown_tool = reclassify(
        &h.state,
        Vec::new(),
        "mcp:files",
        "nope",
        ActionFamily::Read,
    )
    .await
    .unwrap_err();
    let unknown_connection = reclassify(
        &h.state,
        Vec::new(),
        "mcp:ghost",
        "lookup",
        ActionFamily::Read,
    )
    .await
    .unwrap_err();

    // THEN each answers its own status, and nothing is stored
    assert_eq!(exempt.0, StatusCode::BAD_REQUEST);
    assert_eq!(unknown_tool.0, StatusCode::NOT_FOUND);
    assert_eq!(unknown_connection.0, StatusCode::NOT_FOUND);
    assert!(stored(&h, "files").is_empty());

    h.manager.shutdown().await;
    h.journal.shutdown().await;
}

// In eager loading, a connection in pause has no live session: the operator's
// family given then is the one the list answers, the registry holds no stale
// family from the hints, and the resumed session registers the operator's.
#[tokio::test]
async fn test_a_tool_reclassified_in_pause_takes_the_operator_family_at_once_and_on_resume() {
    // GIVEN a custom server whose `lookup` its hints classify as reading,
    //       put in pause
    let h = harness(vec![annotated("files")]).await;
    crate::api::routes_connections::pause::set_paused(&h.state, Vec::new(), "mcp:files", true)
        .await
        .unwrap();

    // WHEN the operator gives `lookup` the family of a write
    let connection = reclassify(
        &h.state,
        Vec::new(),
        "mcp:files",
        "lookup",
        ActionFamily::WriteOrDelete,
    )
    .await
    .unwrap();

    // THEN the route answers the operator's family, from the operator
    let listed = tool(&connection, "mcp:files/lookup");
    assert_eq!(listed.family, ActionFamily::WriteOrDelete);
    assert_eq!(listed.family_source, FamilySource::Operator);
    // AND the registry holds no descriptor still classified by the hints
    if let Some(stale) = h.registry.get("mcp:files/lookup").await.unwrap() {
        assert_eq!(stale.family, ActionFamily::WriteOrDelete);
        assert_eq!(stale.family_source, FamilySource::Operator);
    }

    // WHEN the connection is resumed
    let resumed =
        crate::api::routes_connections::pause::set_paused(&h.state, Vec::new(), "mcp:files", false)
            .await
            .unwrap();

    // THEN the calls are decided on the operator's family, without a restart
    let after = h.registry.get("mcp:files/lookup").await.unwrap().unwrap();
    assert_eq!(after.family, ActionFamily::WriteOrDelete);
    assert_eq!(after.family_source, FamilySource::Operator);
    // AND the list after the resume shows it too
    let listed = tool(&resumed, "mcp:files/lookup");
    assert_eq!(listed.family, ActionFamily::WriteOrDelete);
    assert_eq!(listed.family_source, FamilySource::Operator);

    h.manager.shutdown().await;
    h.journal.shutdown().await;
}

// A tool the selection classifies keeps its family, even on a connection the
// operator added: refused, nothing stored, nothing journaled.
#[tokio::test]
async fn test_a_tool_the_selection_classifies_is_refused() {
    // GIVEN a custom server whose `lookup` the selection classifies
    let mut files = annotated("files");
    files
        .tool_families
        .insert("lookup".to_owned(), ActionFamily::Read);
    let h = harness(vec![files]).await;

    // WHEN the operator tries to reclassify it
    let (status, _) = reclassify(
        &h.state,
        Vec::new(),
        "mcp:files",
        "lookup",
        ActionFamily::WriteOrDelete,
    )
    .await
    .unwrap_err();

    // THEN it is refused, the registry keeps the selection's family, and
    //      nothing is stored or journaled
    assert_eq!(status, StatusCode::CONFLICT);
    let lookup = h.registry.get("mcp:files/lookup").await.unwrap().unwrap();
    assert_eq!(lookup.family_source, FamilySource::Selection);
    assert!(stored(&h, "files").is_empty());
    assert!(h
        .journal
        .query_run(&connection_run_id("mcp:files"))
        .await
        .is_empty());

    h.manager.shutdown().await;
    h.journal.shutdown().await;
}

// A connection that is not an MCP server keeps the family Apollia's code
// gives its tools: refused before anything is written.
#[tokio::test]
async fn test_a_connection_that_is_not_mcp_is_refused() {
    // GIVEN a runtime with one mailbox connected over IMAP
    let h = harness(Vec::new()).await;
    std::fs::write(
        h.state
            .data_dir
            .join(apollia_connectors::imap::IMAP_INDEX_FILE),
        json!({"accounts": [
            {"email": "ada@example.org", "host": "imap.example.org", "port": 993, "username": "ada@example.org"},
        ]})
        .to_string(),
    )
    .unwrap();

    // WHEN the operator tries to reclassify one of its tools
    let (status, Json(body)) = reclassify(
        &h.state,
        Vec::new(),
        "imap:ada@example.org",
        "imap_search",
        ActionFamily::WriteOrDelete,
    )
    .await
    .unwrap_err();

    // THEN it is refused for its kind, before its provenance is read (every
    //      connection that is not MCP also counts as the selection), and
    //      nothing is journaled
    assert_eq!(status, StatusCode::CONFLICT);
    assert!(
        body.error.contains("keep the family Apollia gives them"),
        "{}",
        body.error
    );
    assert!(h
        .journal
        .query_run(&connection_run_id("imap:ada@example.org"))
        .await
        .is_empty());

    h.manager.shutdown().await;
    h.journal.shutdown().await;
}
