#![allow(clippy::unwrap_used, clippy::expect_used)]
use std::collections::HashMap;

use apollia_mcp::config::McpServerConfig;
use apollia_mcp::manager::{McpClientManagerHandle, ProbeSpec};
use apollia_mcp::session::{LoadingMode, McpSessionError};
use apollia_tools::ToolRegistryHandle;

fn mock_server_config(name: &str) -> McpServerConfig {
    McpServerConfig {
        format_version: 1,
        name: name.to_string(),
        command: "python3".to_string(),
        args: vec![format!(
            "{}/tests/mock_mcp_server.py",
            env!("CARGO_MANIFEST_DIR")
        )],
        env: HashMap::new(),
        transport: "stdio".to_string(),
        url: None,
        requires_approval: false,
        init_timeout_secs: 10,
        call_timeout_secs: 10,
        max_response_bytes: 8 * 1024 * 1024,
        max_tools: 256,
        tags: vec![],
    }
}

/// A server added at runtime must appear in status and have its tools registered.
#[tokio::test]
async fn test_add_server_hot() {
    // GIVEN a manager started with no initial servers
    let registry = ToolRegistryHandle::start();
    let manager = McpClientManagerHandle::start(vec![], &registry, None, None, LoadingMode::Eager)
        .await
        .unwrap();
    assert_eq!(manager.status().await.len(), 0);

    // WHEN a new server is added at runtime
    let status = manager
        .add_server(mock_server_config("hot-add"))
        .await
        .unwrap();

    // THEN the returned status reflects the connected server and its tools
    assert_eq!(status.name, "hot-add");
    assert_eq!(status.tools_count, 2);
    assert!(status.connected);

    // AND the server now appears in the global status list
    let all = manager.status().await;
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].name, "hot-add");

    manager.shutdown().await;
}

/// A removed server must disappear from status and tool calls to it must fail.
#[tokio::test]
async fn test_remove_server() {
    // GIVEN a manager with one connected server
    let registry = ToolRegistryHandle::start();
    let manager = McpClientManagerHandle::start(
        vec![mock_server_config("to-remove")],
        &registry,
        None,
        None,
        LoadingMode::Eager,
    )
    .await
    .unwrap();
    assert_eq!(manager.status().await.len(), 1);

    // WHEN the server is removed
    manager.remove_server("to-remove").await.unwrap();

    // THEN the server no longer appears in status
    assert_eq!(manager.status().await.len(), 0);

    // AND a tool call addressed to that server returns ServerExited
    let result = manager
        .call_tool(
            "to-remove",
            "echo",
            Some(serde_json::json!({"message": "gone"})),
        )
        .await;
    assert!(matches!(result, Err(McpSessionError::ServerExited { .. })));

    manager.shutdown().await;
}

/// test_connection must return server info and tools without registering any session.
#[tokio::test]
async fn test_connection_no_side_effect() {
    // GIVEN a manager started with no initial servers
    let registry = ToolRegistryHandle::start();
    let manager = McpClientManagerHandle::start(vec![], &registry, None, None, LoadingMode::Eager)
        .await
        .unwrap();

    // WHEN test_connection is called with a valid mock config
    let result = manager
        .test_connection(mock_server_config("conn-test"))
        .await
        .unwrap();

    // THEN the test result contains the expected server identity and tools
    assert_eq!(result.server_info, "mock-mcp-server");
    assert_eq!(result.tools.len(), 2);
    let tool_names: Vec<&str> = result.tools.iter().map(|t| t.local_name.as_str()).collect();
    assert!(tool_names.contains(&"echo"));
    assert!(tool_names.contains(&"add"));

    // AND no session was persisted (status remains empty)
    assert_eq!(manager.status().await.len(), 0);

    manager.shutdown().await;
}

/// Collect the `mcp:` descriptors currently held by the registry.
async fn registered_mcp_tools(registry: &ToolRegistryHandle) -> Vec<String> {
    registry
        .list()
        .await
        .unwrap()
        .into_iter()
        .map(|d| d.name)
        .filter(|n| n.starts_with("mcp:"))
        .collect()
}

/// In deferred mode the manager keeps only the lightweight index: no `mcp:` tool
/// is registered, and `get_tool_index` aggregates every tool with its server tags.
#[tokio::test]
async fn test_deferred_manager_indexes_without_registering() {
    // GIVEN a manager started in deferred mode with one tagged mock server
    let registry = ToolRegistryHandle::start();
    let mut config = mock_server_config("notion");
    config.tags = vec!["productivity".to_string()];
    let manager =
        McpClientManagerHandle::start(vec![config], &registry, None, None, LoadingMode::Deferred)
            .await
            .unwrap();

    // WHEN the tool registry and the tool index are read
    // THEN the registry holds no mcp descriptor (schemas were never loaded)
    assert!(
        registered_mcp_tools(&registry).await.is_empty(),
        "deferred mode must not register mcp tools"
    );

    // AND get_tool_index returns both tools, each carrying the server tag
    let mut index = manager.get_tool_index().await;
    index.sort_by(|a, b| a.tool_name.cmp(&b.tool_name));
    assert_eq!(index.len(), 2);
    assert_eq!(index[0].tool_name, "add");
    assert_eq!(index[1].tool_name, "echo");
    assert!(index.iter().all(|e| e.server_name == "notion"));
    assert!(index
        .iter()
        .all(|e| e.tags.contains(&"productivity".to_string())));

    manager.shutdown().await;
}

/// In eager mode the manager registers every tool and the deferred index is empty.
#[tokio::test]
async fn test_eager_manager_registers_and_index_empty() {
    // GIVEN a manager started in eager mode with one mock server
    let registry = ToolRegistryHandle::start();
    let manager = McpClientManagerHandle::start(
        vec![mock_server_config("notion")],
        &registry,
        None,
        None,
        LoadingMode::Eager,
    )
    .await
    .unwrap();

    // WHEN the tool registry and the tool index are read
    // THEN the registry holds the two mcp descriptors (schemas loaded at boot)
    assert_eq!(registered_mcp_tools(&registry).await.len(), 2);

    // AND get_tool_index is empty: eager sessions keep no lightweight index
    assert!(manager.get_tool_index().await.is_empty());

    manager.shutdown().await;
}

/// A tool call in deferred mode resolves normally, exercising the on-demand
/// schema fetch performed before the call.
#[tokio::test]
async fn test_deferred_manager_tool_call_succeeds() {
    // GIVEN a deferred-mode manager with the mock server
    let registry = ToolRegistryHandle::start();
    let manager = McpClientManagerHandle::start(
        vec![mock_server_config("calc")],
        &registry,
        None,
        None,
        LoadingMode::Deferred,
    )
    .await
    .unwrap();

    // WHEN a tool is invoked by its local name, with no schema loaded at boot
    let result = manager
        .call_tool("calc", "add", Some(serde_json::json!({"a": 2, "b": 3})))
        .await;

    // THEN the call resolves normally
    assert!(result.is_ok(), "deferred tool call failed: {result:?}");

    manager.shutdown().await;
}

// ─── negotiated protocol version ───────────────────────────────────────────

/// Config for the mock that answers a protocol version of its own.
fn version_server_config(name: &str) -> McpServerConfig {
    McpServerConfig {
        command: "python3".to_string(),
        args: vec![format!(
            "{}/tests/mock_mcp_server_version.py",
            env!("CARGO_MANIFEST_DIR")
        )],
        ..mock_server_config(name)
    }
}

/// A connection test must report the version the server answered.
#[tokio::test]
async fn test_connection_reports_the_server_protocol_version() {
    // GIVEN a manager and a server whose initialize answers "2025-06-18",
    // which is neither the version Apollia sends nor the one the other mocks
    // answer
    let registry = ToolRegistryHandle::start();
    let manager = McpClientManagerHandle::start(vec![], &registry, None, None, LoadingMode::Eager)
        .await
        .unwrap();

    // WHEN the configuration is tested without being persisted
    let result = manager
        .test_connection(version_server_config("version-probe"))
        .await
        .unwrap();

    // THEN the reported version is the one that came back from the server,
    // not a constant compiled into the client
    assert_eq!(result.protocol_version, "2025-06-18");
    assert_eq!(result.server_info, "version-mcp-server");

    manager.shutdown().await;
}

/// The negative control: a server on another revision is reported as such.
#[tokio::test]
async fn test_connection_version_follows_the_server_not_a_constant() {
    // GIVEN a manager and the default mock, which answers "2024-11-05"
    let registry = ToolRegistryHandle::start();
    let manager = McpClientManagerHandle::start(vec![], &registry, None, None, LoadingMode::Eager)
        .await
        .unwrap();

    // WHEN two servers on different revisions are tested through the same path
    let old = manager
        .test_connection(mock_server_config("version-old"))
        .await
        .unwrap();
    let new = manager
        .test_connection(version_server_config("version-new"))
        .await
        .unwrap();

    // THEN the two verdicts differ: reporting one constant for every server
    // would be the same defect as reporting the other
    assert_eq!(old.protocol_version, "2024-11-05");
    assert_ne!(old.protocol_version, new.protocol_version);

    manager.shutdown().await;
}

// ─── the health probe in deferred mode ─────────────────────────────────────

/// A live-server test must run its probe whatever the loading mode.
#[tokio::test]
async fn test_live_server_probe_runs_in_deferred_mode() {
    // GIVEN a manager holding the mock server in deferred mode, where the
    // session carries a tool index rather than loaded schemas
    let registry = ToolRegistryHandle::start();
    let manager = McpClientManagerHandle::start(
        vec![mock_server_config("probe-deferred")],
        &registry,
        None,
        None,
        LoadingMode::Deferred,
    )
    .await
    .unwrap();

    // WHEN the server is tested with a read-only probe on a tool it exposes
    let result = manager
        .test_live_server(
            "probe-deferred",
            Some(ProbeSpec {
                tool: "echo".to_string(),
                args: Some(serde_json::json!({"message": "probe"})),
            }),
        )
        .await
        .unwrap();

    // THEN the probe ran and its success is what the health reports: a
    // verdict of `verified: false` means the probe was skipped, and a probe
    // silently skipped is a health check that never happened
    assert_eq!(
        result.live_health,
        Some(apollia_core::McpHealth::Healthy { verified: true }),
        "the probe must run in deferred mode, not be skipped as an unknown tool"
    );

    manager.shutdown().await;
}

/// The negative control: a probe on a tool the server does not expose is skipped.
#[tokio::test]
async fn test_live_server_probe_skips_an_unknown_tool() {
    // GIVEN the same deferred manager
    let registry = ToolRegistryHandle::start();
    let manager = McpClientManagerHandle::start(
        vec![mock_server_config("probe-unknown")],
        &registry,
        None,
        None,
        LoadingMode::Deferred,
    )
    .await
    .unwrap();

    // WHEN the probe names a tool the server never published
    let result = manager
        .test_live_server(
            "probe-unknown",
            Some(ProbeSpec {
                tool: "not-a-tool".to_string(),
                args: None,
            }),
        )
        .await
        .unwrap();

    // THEN it is skipped rather than counted as a failure: running every
    // probe would be the same defect as running none
    assert_eq!(
        result.live_health,
        Some(apollia_core::McpHealth::Healthy { verified: false }),
        "an absent probe tool must leave the verdict at reachability only"
    );

    manager.shutdown().await;
}

// ─── what a reload reports ─────────────────────────────────────────────────

/// A reload event must name the tools it moved, whatever the loading mode.
#[tokio::test]
async fn test_reload_event_names_tools_in_deferred_mode() {
    // GIVEN a manager in deferred mode with an event bus subscribed
    let (bus, mut rx) = tokio::sync::broadcast::channel(16);
    let registry = ToolRegistryHandle::start();
    let manager = McpClientManagerHandle::start(
        vec![mock_server_config("reload-deferred")],
        &registry,
        Some(bus),
        None,
        LoadingMode::Deferred,
    )
    .await
    .unwrap();

    // WHEN the server is hot-reloaded
    manager.reload_server("reload-deferred").await.unwrap();

    // THEN the published event names the two tools on both sides: an empty
    // pair would report that a server exposing two tools moved nothing
    let event = rx.try_recv().unwrap();
    match event {
        apollia_core::RuntimeEvent::McpServerReloaded {
            name,
            old_tools,
            new_tools,
        } => {
            assert_eq!(name, "reload-deferred");
            assert_eq!(old_tools, vec!["echo".to_string(), "add".to_string()]);
            assert_eq!(new_tools, old_tools);
        }
        other => panic!("expected McpServerReloaded, got {other:?}"),
    }

    manager.shutdown().await;
}
