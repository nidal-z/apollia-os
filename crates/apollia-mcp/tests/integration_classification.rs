#![allow(clippy::unwrap_used, clippy::expect_used)]
//! The action family an MCP tool is published with, end to end: a real
//! subprocess declares its hints in `tools/list`, the manager registers the
//! tools, and the registry holds the family each one was given.

use std::collections::HashMap;

use apollia_mcp::config::McpServerConfig;
use apollia_mcp::manager::McpClientManagerHandle;
use apollia_mcp::session::LoadingMode;
use apollia_tools::{ActionFamily, FamilySource, ToolDescriptor, ToolRegistryHandle};

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
        tags: vec![],
        tool_families: Default::default(),
        operator_families: Default::default(),
        origin: None,
    }
}

async fn descriptor(registry: &ToolRegistryHandle, name: &str) -> ToolDescriptor {
    registry
        .get(name)
        .await
        .unwrap()
        .unwrap_or_else(|| panic!("{name} is not registered"))
}

#[tokio::test]
async fn test_each_tool_is_published_with_the_family_its_hints_give() {
    // GIVEN a server announcing a read-only tool, a destructive tool and a
    //       tool with no hint, on a connection without an account
    let registry = ToolRegistryHandle::start();
    let manager = McpClientManagerHandle::start(vec![], &registry, None, LoadingMode::Eager)
        .await
        .unwrap();

    // WHEN the server is added and its tools registered
    manager
        .add_server(annotated_server_config("annotated"))
        .await
        .unwrap();

    // THEN the read-only tool reads, on the server's word, without the
    //      read-only fast path a server hint is not trusted to unlock
    let lookup = descriptor(&registry, "mcp:annotated/lookup").await;
    assert_eq!(lookup.family, ActionFamily::Read);
    assert_eq!(lookup.family_source, FamilySource::Server);
    assert!(!lookup.is_read_only);
    assert_eq!(lookup.risk_score, 1);
    // AND the destructive tool writes or deletes
    let purge = descriptor(&registry, "mcp:annotated/purge").await;
    assert_eq!(purge.family, ActionFamily::WriteOrDelete);
    assert_eq!(purge.family_source, FamilySource::Server);
    assert!(!purge.is_read_only);
    // AND the tool without a hint is unclassified, with no source
    let ping = descriptor(&registry, "mcp:annotated/ping").await;
    assert_eq!(ping.family, ActionFamily::Unclassified);
    assert_eq!(ping.family_source, FamilySource::Unsourced);
    assert!(!ping.is_read_only);
    // AND no MCP tool designates a target argument
    for tool in [&lookup, &purge, &ping] {
        assert_eq!(tool.target_arg, None, "{}", tool.name);
    }

    manager.shutdown().await;
}

#[tokio::test]
async fn test_the_selection_classification_wins_over_the_hints() {
    // GIVEN the same server, whose selection entry classifies two of its
    //       three tools
    let registry = ToolRegistryHandle::start();
    let manager = McpClientManagerHandle::start(vec![], &registry, None, LoadingMode::Eager)
        .await
        .unwrap();
    let mut config = annotated_server_config("curated");
    config.tool_families = [
        ("lookup".to_string(), ActionFamily::ReadPersonal),
        ("ping".to_string(), ActionFamily::SendExternal),
    ]
    .into();

    // WHEN the server is added
    manager.add_server(config).await.unwrap();

    // THEN the classified tools take the selection's family, and a read from
    //      the selection is read-only
    let lookup = descriptor(&registry, "mcp:curated/lookup").await;
    assert_eq!(lookup.family, ActionFamily::ReadPersonal);
    assert_eq!(lookup.family_source, FamilySource::Selection);
    assert!(lookup.is_read_only);
    let ping = descriptor(&registry, "mcp:curated/ping").await;
    assert_eq!(ping.family, ActionFamily::SendExternal);
    assert_eq!(ping.family_source, FamilySource::Selection);
    // AND the tool the selection leaves out falls back to its hints
    let purge = descriptor(&registry, "mcp:curated/purge").await;
    assert_eq!(purge.family, ActionFamily::WriteOrDelete);
    assert_eq!(purge.family_source, FamilySource::Server);

    manager.shutdown().await;
}

#[tokio::test]
async fn test_the_operator_family_wins_over_the_hints_in_both_loadings() {
    // GIVEN the server in eager loading, and the same server in deferred
    //       loading, each started with an operator family for `lookup`,
    //       which the server declares read-only
    let mut config = annotated_server_config("classified");
    config.operator_families = [("lookup".to_string(), ActionFamily::SendExternal)].into();
    let registry = ToolRegistryHandle::start();
    let eager = McpClientManagerHandle::start(vec![], &registry, None, LoadingMode::Eager)
        .await
        .unwrap();
    let deferred_registry = ToolRegistryHandle::start();
    let deferred =
        McpClientManagerHandle::start(vec![], &deferred_registry, None, LoadingMode::Deferred)
            .await
            .unwrap();

    // WHEN each is added
    eager.add_server(config.clone()).await.unwrap();
    deferred.add_server(config).await.unwrap();

    // THEN the registry holds the operator's family, from the operator
    let lookup = descriptor(&registry, "mcp:classified/lookup").await;
    assert_eq!(lookup.family, ActionFamily::SendExternal);
    assert_eq!(lookup.family_source, FamilySource::Operator);
    // AND so does the deferred index the calls are decided on
    let index = deferred.get_tool_index().await;
    let entry = index.iter().find(|e| e.tool_name == "lookup").unwrap();
    assert_eq!(entry.family, ActionFamily::SendExternal);
    assert_eq!(entry.family_source, FamilySource::Operator);

    eager.shutdown().await;
    deferred.shutdown().await;
}

#[tokio::test]
async fn test_a_live_session_takes_in_the_operator_family_without_a_restart() {
    // GIVEN a running server in each loading, `purge` declared destructive
    let registry = ToolRegistryHandle::start();
    let eager = McpClientManagerHandle::start(vec![], &registry, None, LoadingMode::Eager)
        .await
        .unwrap();
    let deferred_registry = ToolRegistryHandle::start();
    let deferred =
        McpClientManagerHandle::start(vec![], &deferred_registry, None, LoadingMode::Deferred)
            .await
            .unwrap();
    eager
        .add_server(annotated_server_config("live"))
        .await
        .unwrap();
    deferred
        .add_server(annotated_server_config("live"))
        .await
        .unwrap();

    // WHEN the operator classifies `purge` as a read
    eager
        .set_operator_family("live", "purge", ActionFamily::Read)
        .await
        .unwrap();
    deferred
        .set_operator_family("live", "purge", ActionFamily::Read)
        .await
        .unwrap();

    // THEN the registry descriptor is replaced, still not on the read-only
    //      fast path, and the deferred index reads it too
    let purge = descriptor(&registry, "mcp:live/purge").await;
    assert_eq!(purge.family, ActionFamily::Read);
    assert_eq!(purge.family_source, FamilySource::Operator);
    assert!(!purge.is_read_only);
    let index = deferred.get_tool_index().await;
    let entry = index.iter().find(|e| e.tool_name == "purge").unwrap();
    assert_eq!(entry.family, ActionFamily::Read);
    assert_eq!(entry.family_source, FamilySource::Operator);
    // AND the other tools keep their hints
    let lookup = descriptor(&registry, "mcp:live/lookup").await;
    assert_eq!(lookup.family_source, FamilySource::Server);

    eager.shutdown().await;
    deferred.shutdown().await;
}
