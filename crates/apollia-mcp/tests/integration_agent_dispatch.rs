#![allow(clippy::unwrap_used, clippy::expect_used)]
//! End-to-end proof that an MCP tool actually executes through the agent
//! dispatch path, not merely resolves.
//!
//! This exercises the exact production assembly the runtime uses when wiring an
//! agent's tools: `build_agent_tool_executors` collects one executor per tool of
//! each connected MCP server, those executors are placed in a `ToolDispatcher`,
//! and a call addressed as `mcp:{server}/{tool}` is dispatched against a real
//! stdio MCP server. Both eager and deferred loading modes are covered.

use std::collections::HashMap;

use apollia_mcp::config::McpServerConfig;
use apollia_mcp::executor::build_agent_tool_executors;
use apollia_mcp::manager::McpClientManagerHandle;
use apollia_mcp::session::LoadingMode;
use apollia_tools::executor::ToolDispatcher;
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
        max_response_bytes: 8 * 1024 * 1024,
        max_tools: 256,
        tags: vec![],
    }
}

/// A dispatcher built from `build_agent_tool_executors` must actually run an
/// `mcp:` tool and return its output. Covers the eager loading mode.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn mcp_tool_executes_through_dispatcher_eager() {
    // GIVEN a manager with the mock server connected in eager mode
    let registry = ToolRegistryHandle::start();
    let manager = McpClientManagerHandle::start(
        vec![mock_server_config("calc")],
        &registry,
        None,
        None,
        LoadingMode::Eager,
    )
    .await
    .expect("manager start failed");

    // AND the agent-facing executors assembled exactly as the runtime does
    let execs = build_agent_tool_executors(&manager).await;
    let dispatcher = ToolDispatcher::new(execs);

    // WHEN the agent dispatches an mcp: tool by its qualified name
    let out = dispatcher
        .dispatch("mcp:calc/echo", serde_json::json!({"message": "hello mcp"}))
        .await
        .expect("mcp tool dispatch must succeed");

    // THEN the tool executed and its output came back (not just resolved)
    assert_eq!(out, serde_json::json!({"content": "hello mcp"}));

    // AND a second tool with real computation confirms it is truly executing
    let sum = dispatcher
        .dispatch("mcp:calc/add", serde_json::json!({"a": 2, "b": 3}))
        .await
        .expect("mcp add dispatch must succeed");
    assert_eq!(sum, serde_json::json!({"content": "5"}));

    manager.shutdown().await;
    registry.shutdown().await;
}

/// Same guarantee in deferred loading mode, where tool schemas are fetched
/// on demand rather than at boot.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn mcp_tool_executes_through_dispatcher_deferred() {
    // GIVEN a manager with the mock server connected in deferred mode
    let registry = ToolRegistryHandle::start();
    let manager = McpClientManagerHandle::start(
        vec![mock_server_config("calc")],
        &registry,
        None,
        None,
        LoadingMode::Deferred,
    )
    .await
    .expect("manager start failed");

    // AND the agent-facing executors assembled from the deferred index
    let execs = build_agent_tool_executors(&manager).await;
    let dispatcher = ToolDispatcher::new(execs);

    // WHEN the agent dispatches the deferred mcp: tool
    let out = dispatcher
        .dispatch(
            "mcp:calc/echo",
            serde_json::json!({"message": "deferred ok"}),
        )
        .await
        .expect("deferred mcp tool dispatch must succeed");

    // THEN the tool executed and returned its output
    assert_eq!(out, serde_json::json!({"content": "deferred ok"}));

    manager.shutdown().await;
    registry.shutdown().await;
}
