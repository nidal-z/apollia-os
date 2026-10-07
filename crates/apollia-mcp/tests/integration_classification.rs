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
