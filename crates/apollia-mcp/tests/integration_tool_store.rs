#![allow(clippy::unwrap_used, clippy::expect_used)]
//! The tools a server publishes survive a restart: a real subprocess answers
//! `tools/list` with hints, the manager keeps the answer in `mcp.db`, and a
//! manager started later, with no live session of that server, still serves
//! the tools, their descriptions and their family in its tool index.

use std::collections::HashMap;
use std::path::Path;

use apollia_mcp::config::McpServerConfig;
use apollia_mcp::manager::McpClientManagerHandle;
use apollia_mcp::session::LoadingMode;
use apollia_mcp::tool_search::ToolIndexSnapshot;
use apollia_mcp::tool_store::ToolIndexStore;
use apollia_mcp::McpServerRepository;
use apollia_tools::{ActionFamily, FamilySource, ToolRegistryHandle};
use tempfile::TempDir;

fn annotated_server_config(name: &str) -> McpServerConfig {
    McpServerConfig {
        format_version: 1,
        name: name.to_string(),
        command: "python3".to_string(),
        args: vec![format!(
            "{}/tests/mock_mcp_server_annotated.py",
            env!("CARGO_MANIFEST_DIR")
        )],
        env: HashMap::new(),
        transport: "stdio".to_string(),
        url: None,
        init_timeout_secs: 10,
        call_timeout_secs: 10,
        max_response_bytes: 8 * 1024 * 1024,
        max_tools: 256,
        tags: vec!["records".to_string()],
        tool_families: Default::default(),
        origin: None,
    }
}

/// The same server, as configured, whose process cannot start.
fn unstartable(mut config: McpServerConfig) -> McpServerConfig {
    config.command = "/nonexistent/apollia-mcp-server".to_string();
    config.args = vec![];
    config
}

async fn start(
    configs: Vec<McpServerConfig>,
    db: &Path,
    registry: &ToolRegistryHandle,
) -> McpClientManagerHandle {
    McpClientManagerHandle::start_keeping_tools(
        configs,
        registry,
        None,
        LoadingMode::Deferred,
        ToolIndexStore::new(db),
    )
    .await
    .unwrap()
}

fn by_name(index: &[ToolIndexSnapshot]) -> HashMap<String, ToolIndexSnapshot> {
    index
        .iter()
        .map(|e| (e.tool_name.clone(), e.clone()))
        .collect()
}

/// Run the annotated server once under `config`, so its tools are kept in `db`.
async fn run_once(config: McpServerConfig, db: &Path) {
    let registry = ToolRegistryHandle::start();
    let manager = start(vec![config], db, &registry).await;
    manager.shutdown().await;
}

#[tokio::test]
async fn test_a_deferred_index_classifies_each_tool_by_its_hints() {
    // GIVEN a deferred manager over a server declaring a read-only tool, a
    //       destructive tool and a tool with no hint
    let dir = TempDir::new().unwrap();
    let registry = ToolRegistryHandle::start();
    let db = dir.path().join("mcp.db");

    // WHEN its tool index is read
    let manager = start(vec![annotated_server_config("annotated")], &db, &registry).await;
    let index = by_name(&manager.get_tool_index().await);

    // THEN each entry carries the hints and the family they give
    assert_eq!(index["lookup"].family, ActionFamily::Read);
    assert_eq!(index["lookup"].family_source, FamilySource::Server);
    assert_eq!(
        index["lookup"].annotations.and_then(|a| a.read_only_hint),
        Some(true)
    );
    assert_eq!(index["purge"].family, ActionFamily::WriteOrDelete);
    assert_eq!(index["ping"].family, ActionFamily::Unclassified);
    assert_eq!(index["ping"].family_source, FamilySource::Unsourced);

    manager.shutdown().await;
}

#[tokio::test]
async fn test_the_tools_list_is_kept_in_mcp_db() {
    // GIVEN a server started once by a manager that keeps tools
    let dir = TempDir::new().unwrap();
    let db = dir.path().join("mcp.db");

    // WHEN the manager has started it
    run_once(annotated_server_config("annotated"), &db).await;

    // THEN mcp.db holds the three tools, title, schema and hints included
    let stored = McpServerRepository::open(&db)
        .unwrap()
        .stored_tools()
        .unwrap();
    let tools = &stored["annotated"];
    let names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(names, vec!["lookup", "ping", "purge"]);
    let lookup = &tools[0];
    assert_eq!(lookup.title.as_deref(), Some("Look up a record"));
    assert_eq!(lookup.input_schema, serde_json::json!({"type": "object"}));
    assert_eq!(
        lookup.annotations.and_then(|a| a.read_only_hint),
        Some(true)
    );
    assert!(!lookup.fetched_at.is_empty());
}

#[tokio::test]
async fn test_after_a_restart_without_a_live_session_the_kept_tools_are_indexed() {
    // GIVEN a server whose tools were kept by an earlier launch
    let dir = TempDir::new().unwrap();
    let db = dir.path().join("mcp.db");
    run_once(annotated_server_config("annotated"), &db).await;

    // WHEN a new manager starts with the same server, whose process does not
    //      start this time
    let registry = ToolRegistryHandle::start();
    let manager = start(
        vec![unstartable(annotated_server_config("annotated"))],
        &db,
        &registry,
    )
    .await;

    // THEN no session is live, and the tool index still serves the three
    //      tools with their descriptions, tags and families
    assert!(manager.status().await.is_empty());
    let index = by_name(&manager.get_tool_index().await);
    assert_eq!(index.len(), 3);
    assert_eq!(
        index["lookup"].description.as_deref(),
        Some("Read a record")
    );
    assert_eq!(index["lookup"].tags, vec!["records".to_string()]);
    assert_eq!(index["lookup"].family, ActionFamily::Read);
    assert_eq!(index["purge"].family, ActionFamily::WriteOrDelete);
    assert_eq!(
        index["lookup"].input_schema,
        Some(serde_json::json!({"type": "object"}))
    );
    assert!(index.values().all(|e| e.server_name == "annotated"));

    manager.shutdown().await;
}

#[tokio::test]
async fn test_a_kept_server_of_the_selection_keeps_its_compiled_classification() {
    // GIVEN kept tools of a server, now configured from a selection entry
    //       that classifies one tool against its hint
    let dir = TempDir::new().unwrap();
    let db = dir.path().join("mcp.db");
    run_once(annotated_server_config("annotated"), &db).await;
    let mut config = unstartable(annotated_server_config("annotated"));
    config.tool_families = [("lookup".to_string(), ActionFamily::SendExternal)].into();

    // WHEN a manager starts without a live session of it
    let registry = ToolRegistryHandle::start();
    let manager = start(vec![config], &db, &registry).await;
    let index = by_name(&manager.get_tool_index().await);

    // THEN the selection decides first, then the hints, then nothing
    assert_eq!(
        (index["lookup"].family, index["lookup"].family_source),
        (ActionFamily::SendExternal, FamilySource::Selection)
    );
    assert_eq!(
        (index["purge"].family, index["purge"].family_source),
        (ActionFamily::WriteOrDelete, FamilySource::Server)
    );
    assert_eq!(
        (index["ping"].family, index["ping"].family_source),
        (ActionFamily::Unclassified, FamilySource::Unsourced)
    );

    manager.shutdown().await;
}

#[tokio::test]
async fn test_a_server_no_longer_configured_is_not_indexed() {
    // GIVEN kept tools of a server that is no longer configured
    let dir = TempDir::new().unwrap();
    let db = dir.path().join("mcp.db");
    run_once(annotated_server_config("annotated"), &db).await;

    // WHEN a manager starts without it
    let registry = ToolRegistryHandle::start();
    let manager = start(vec![], &db, &registry).await;

    // THEN its tools are not served
    assert!(manager.get_tool_index().await.is_empty());
    manager.shutdown().await;
}

#[tokio::test]
async fn test_removing_a_kept_server_forgets_its_tools() {
    // GIVEN a manager serving the kept tools of a server with no session
    let dir = TempDir::new().unwrap();
    let db = dir.path().join("mcp.db");
    run_once(annotated_server_config("annotated"), &db).await;
    let registry = ToolRegistryHandle::start();
    let manager = start(
        vec![unstartable(annotated_server_config("annotated"))],
        &db,
        &registry,
    )
    .await;
    assert_eq!(manager.get_tool_index().await.len(), 3);

    // WHEN the server is removed
    let _ = manager.remove_server("annotated").await;

    // THEN the index no longer serves it, and mcp.db no longer keeps it
    assert!(manager.get_tool_index().await.is_empty());
    let stored = McpServerRepository::open(&db)
        .unwrap()
        .stored_tools()
        .unwrap();
    assert!(!stored.contains_key("annotated"));
    manager.shutdown().await;
}

#[tokio::test]
async fn test_a_server_that_starts_again_replaces_its_kept_tools() {
    // GIVEN a manager serving the kept tools of a server with no session
    let dir = TempDir::new().unwrap();
    let db = dir.path().join("mcp.db");
    run_once(annotated_server_config("annotated"), &db).await;
    let registry = ToolRegistryHandle::start();
    let manager = start(
        vec![unstartable(annotated_server_config("annotated"))],
        &db,
        &registry,
    )
    .await;

    // WHEN the server is added again and its session starts
    manager
        .add_server(annotated_server_config("annotated"))
        .await
        .unwrap();

    // THEN the index serves the live session's tools, once each
    let index = manager.get_tool_index().await;
    assert_eq!(index.len(), 3);
    assert_eq!(manager.status().await.len(), 1);
    manager.shutdown().await;
}

#[tokio::test]
async fn test_a_tools_list_re_sent_at_a_call_is_kept() {
    // GIVEN a deferred manager over a server that published one tool at its
    //       start and publishes a second one since
    let dir = TempDir::new().unwrap();
    let db = dir.path().join("mcp.db");
    let mut config = annotated_server_config("growing");
    config.args = vec![format!(
        "{}/tests/mock_mcp_server_growing.py",
        env!("CARGO_MANIFEST_DIR")
    )];
    let registry = ToolRegistryHandle::start();
    let manager = start(vec![config], &db, &registry).await;
    let kept = |db: &Path| {
        McpServerRepository::open(db)
            .unwrap()
            .stored_tools()
            .unwrap()
            .remove("growing")
            .unwrap_or_default()
    };
    assert_eq!(kept(&db).len(), 1);

    // WHEN the new tool, absent from the index, is called, which lists the
    //      server's tools again
    manager.call_tool("growing", "archive", None).await.unwrap();

    // THEN that list is kept in mcp.db, its hints included, and indexed
    let tools = kept(&db);
    let names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(names, vec!["archive", "lookup"]);
    assert_eq!(
        tools[0].annotations.and_then(|a| a.destructive_hint),
        Some(true)
    );
    let index = by_name(&manager.get_tool_index().await);
    assert_eq!(index["archive"].family, ActionFamily::WriteOrDelete);

    manager.shutdown().await;
}
