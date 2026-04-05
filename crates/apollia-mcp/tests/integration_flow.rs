use std::collections::HashMap;

use apollia_mcp::config::McpServerConfig;
use apollia_mcp::manager::McpClientManagerHandle;
use apollia_mcp::session::McpSessionError;
use apollia_tools::ToolRegistryHandle;

fn mock_server_config(name: &str) -> McpServerConfig {
    McpServerConfig {
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
        tags: vec![],
    }
}

/// A server added at runtime must appear in status and have its tools registered.
#[tokio::test]
async fn test_add_server_hot() {
    // GIVEN a manager started with no initial servers
    let registry = ToolRegistryHandle::start();
    let manager = McpClientManagerHandle::start(vec![], &registry, None, None)
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
    let manager =
        McpClientManagerHandle::start(vec![mock_server_config("to-remove")], &registry, None, None)
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
    let manager = McpClientManagerHandle::start(vec![], &registry, None, None)
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
