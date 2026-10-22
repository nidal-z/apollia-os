#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Servers that start when a call needs them, and stop when no call does.
//!
//! A manager started with a tool store starts none of its servers: their
//! kept tools are served at once, the first call of a tool starts its server
//! and waits for its handshake, and a server idle for the idle delay stops,
//! its tools still served. A paused server never starts at a call.

use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

use apollia_mcp::config::McpServerConfig;
use apollia_mcp::manager::{McpClientManagerHandle, OnDemand};
use apollia_mcp::protocol::{ToolCallContent, ToolCallResult};
use apollia_mcp::session::LoadingMode;
use apollia_mcp::tool_store::ToolIndexStore;
use apollia_mcp::McpServerRepository;
use apollia_tools::ToolRegistryHandle;
use tempfile::TempDir;

fn calc_config() -> McpServerConfig {
    McpServerConfig {
        format_version: 1,
        name: "calc".to_string(),
        command: "python3".to_string(),
        args: vec![format!(
            "{}/tests/mock_mcp_server.py",
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

async fn start(
    configs: Vec<McpServerConfig>,
    db: &Path,
    registry: &ToolRegistryHandle,
    mode: LoadingMode,
    idle_stop: Option<Duration>,
) -> McpClientManagerHandle {
    McpClientManagerHandle::start_keeping_tools(
        configs,
        registry,
        None,
        mode,
        OnDemand {
            tool_store: ToolIndexStore::new(db),
            idle_stop,
        },
    )
    .await
    .unwrap()
}

/// Add the calc server once to a manager over `db`, so its tools are kept.
async fn keep_calc_tools(db: &Path, mode: LoadingMode) {
    let registry = ToolRegistryHandle::start();
    let manager = start(vec![], db, &registry, mode, None).await;
    manager.add_server(calc_config()).await.unwrap();
    manager.shutdown().await;
}

fn text_of(result: &ToolCallResult) -> String {
    match result.content.first() {
        Some(ToolCallContent::Text { text }) => text.clone(),
        _ => String::new(),
    }
}

fn indexed_tools(index: &[apollia_mcp::tool_search::ToolIndexSnapshot]) -> Vec<String> {
    let mut names: Vec<String> = index.iter().map(|e| e.tool_name.clone()).collect();
    names.sort();
    names
}

#[tokio::test]
async fn test_a_manager_with_a_store_starts_no_server_and_serves_the_kept_tools() {
    // GIVEN a server whose tools an earlier launch kept
    let dir = TempDir::new().unwrap();
    let db = dir.path().join("mcp.db");
    keep_calc_tools(&db, LoadingMode::Deferred).await;

    // WHEN a manager starts with it
    let registry = ToolRegistryHandle::start();
    let manager = start(
        vec![calc_config()],
        &db,
        &registry,
        LoadingMode::Deferred,
        None,
    )
    .await;

    // THEN no session runs, and the kept tools are indexed
    assert!(manager.status().await.is_empty());
    assert_eq!(
        indexed_tools(&manager.get_tool_index().await),
        vec!["add", "echo"]
    );
    // AND its detail answers from the configuration, not connected
    let detail = manager.server_detail("calc").await.expect("detail");
    assert!(!detail.status.connected);
    assert_eq!(detail.tools.len(), 2);
    manager.shutdown().await;
}

#[tokio::test]
async fn test_the_first_call_starts_its_server_and_the_calls_succeed() {
    // GIVEN a manager over a dormant server
    let dir = TempDir::new().unwrap();
    let db = dir.path().join("mcp.db");
    keep_calc_tools(&db, LoadingMode::Deferred).await;
    let registry = ToolRegistryHandle::start();
    let manager = start(
        vec![calc_config()],
        &db,
        &registry,
        LoadingMode::Deferred,
        None,
    )
    .await;
    assert!(manager.status().await.is_empty());

    // WHEN two calls of its tools arrive together
    let echo = serde_json::json!({"message": "on demand"});
    let add = serde_json::json!({"a": 2, "b": 3});
    let (first, second) = tokio::join!(
        manager.call_tool("calc", "echo", Some(echo)),
        manager.call_tool("calc", "add", Some(add)),
    );

    // THEN both waited for the one session they started, and succeeded
    assert_eq!(text_of(&first.expect("echo")), "on demand");
    assert_eq!(text_of(&second.expect("add")), "5");
    let statuses = manager.status().await;
    assert_eq!(statuses.len(), 1);
    assert_eq!(statuses[0].name, "calc");
    manager.shutdown().await;
}

#[tokio::test]
async fn test_a_server_idle_for_the_delay_stops_and_its_index_stays_readable() {
    // GIVEN a manager whose idle delay is spent as soon as a call ends
    let dir = TempDir::new().unwrap();
    let db = dir.path().join("mcp.db");
    keep_calc_tools(&db, LoadingMode::Deferred).await;
    let registry = ToolRegistryHandle::start();
    let manager = start(
        vec![calc_config()],
        &db,
        &registry,
        LoadingMode::Deferred,
        Some(Duration::ZERO),
    )
    .await;

    // WHEN a call starts the server and ends, and the actor is asked again
    let answer = manager
        .call_tool("calc", "echo", Some(serde_json::json!({"message": "once"})))
        .await
        .expect("call");
    let detail = manager.server_detail("calc").await.expect("detail");

    // THEN the call succeeded, the server stopped, and its tools are still
    //      indexed and kept
    assert_eq!(text_of(&answer), "once");
    assert!(!detail.status.connected);
    assert!(manager.status().await.is_empty());
    assert_eq!(
        indexed_tools(&manager.get_tool_index().await),
        vec!["add", "echo"]
    );
    let stored = McpServerRepository::open(&db)
        .unwrap()
        .stored_tools()
        .unwrap();
    assert_eq!(stored["calc"].len(), 2);
    // AND its next call starts it again
    let again = manager
        .call_tool(
            "calc",
            "echo",
            Some(serde_json::json!({"message": "twice"})),
        )
        .await
        .expect("second call");
    assert_eq!(text_of(&again), "twice");
    manager.shutdown().await;
}

#[tokio::test]
async fn test_a_server_within_its_idle_delay_keeps_running() {
    // GIVEN a manager whose idle delay is an hour
    let dir = TempDir::new().unwrap();
    let db = dir.path().join("mcp.db");
    keep_calc_tools(&db, LoadingMode::Deferred).await;
    let registry = ToolRegistryHandle::start();
    let manager = start(
        vec![calc_config()],
        &db,
        &registry,
        LoadingMode::Deferred,
        Some(Duration::from_secs(3600)),
    )
    .await;

    // WHEN a call starts the server and ends, and the actor is asked again
    manager
        .call_tool("calc", "echo", Some(serde_json::json!({"message": "x"})))
        .await
        .expect("call");
    let detail = manager.server_detail("calc").await.expect("detail");

    // THEN its session still runs
    assert!(detail.status.connected);
    assert_eq!(manager.status().await.len(), 1);
    manager.shutdown().await;
}

#[tokio::test]
async fn test_a_paused_dormant_server_never_starts_at_a_call() {
    // GIVEN a manager over a dormant server
    let dir = TempDir::new().unwrap();
    let db = dir.path().join("mcp.db");
    keep_calc_tools(&db, LoadingMode::Deferred).await;
    let registry = ToolRegistryHandle::start();
    let manager = start(
        vec![calc_config()],
        &db,
        &registry,
        LoadingMode::Deferred,
        None,
    )
    .await;

    // WHEN it is paused, then one of its tools is called
    manager.stop_server("calc").await.expect("paused");
    let call = manager.call_tool("calc", "echo", None).await;

    // THEN the call is refused, no session starts, its tools leave the
    //      index and stay kept in mcp.db
    assert!(call.is_err());
    assert!(manager.status().await.is_empty());
    assert!(manager.get_tool_index().await.is_empty());
    let stored = McpServerRepository::open(&db)
        .unwrap()
        .stored_tools()
        .unwrap();
    assert_eq!(stored["calc"].len(), 2);
    manager.shutdown().await;
}

#[tokio::test]
async fn test_in_eager_loading_a_dormant_server_s_tools_are_registered_and_callable() {
    // GIVEN tools kept by an eager launch
    let dir = TempDir::new().unwrap();
    let db = dir.path().join("mcp.db");
    keep_calc_tools(&db, LoadingMode::Eager).await;

    // WHEN an eager manager starts with the server
    let registry = ToolRegistryHandle::start();
    let manager = start(
        vec![calc_config()],
        &db,
        &registry,
        LoadingMode::Eager,
        None,
    )
    .await;

    // THEN its descriptors are registered before any session runs
    assert!(manager.status().await.is_empty());
    assert!(registry.get("mcp:calc/echo").await.unwrap().is_some());
    // AND a call starts it, its descriptors registered once
    let answer = manager
        .call_tool(
            "calc",
            "echo",
            Some(serde_json::json!({"message": "eager"})),
        )
        .await
        .expect("call");
    assert_eq!(text_of(&answer), "eager");
    assert!(registry.get("mcp:calc/add").await.unwrap().is_some());
    // AND removing it takes its descriptors out
    manager.remove_server("calc").await.expect("removed");
    assert!(registry.get("mcp:calc/echo").await.unwrap().is_none());
    manager.shutdown().await;
}

#[tokio::test]
async fn test_a_server_with_nothing_kept_starts_without_holding_the_manager() {
    // GIVEN no tool kept, and a server whose process does not answer
    let dir = TempDir::new().unwrap();
    let db = dir.path().join("mcp.db");
    let mut silent = calc_config();
    silent.name = "silent".to_string();
    silent.command = "sh".to_string();
    silent.args = vec!["-c".to_string(), "exec sleep 30".to_string()];

    // WHEN a manager starts with it and the reachable server
    let registry = ToolRegistryHandle::start();
    let manager = start(
        vec![silent, calc_config()],
        &db,
        &registry,
        LoadingMode::Deferred,
        None,
    )
    .await;

    // THEN the manager answers before the silent handshake ends, and a call
    //      of the reachable server succeeds meanwhile
    let answer = manager
        .call_tool(
            "calc",
            "echo",
            Some(serde_json::json!({"message": "meanwhile"})),
        )
        .await
        .expect("call");
    assert_eq!(text_of(&answer), "meanwhile");
    let names: Vec<String> = manager.status().await.into_iter().map(|s| s.name).collect();
    assert_eq!(names, vec!["calc".to_string()]);
    manager.shutdown().await;
}

#[tokio::test]
async fn test_a_live_test_of_a_dormant_server_starts_it() {
    // GIVEN a manager over a dormant server
    let dir = TempDir::new().unwrap();
    let db = dir.path().join("mcp.db");
    keep_calc_tools(&db, LoadingMode::Deferred).await;
    let registry = ToolRegistryHandle::start();
    let manager = start(
        vec![calc_config()],
        &db,
        &registry,
        LoadingMode::Deferred,
        None,
    )
    .await;

    // WHEN the operator tests it
    let tested = manager.test_live_server("calc", None).await;

    // THEN its session started for the test, which succeeded
    let tested = tested.expect("tested");
    assert_eq!(tested.tools.len(), 2);
    assert_eq!(manager.status().await.len(), 1);
    manager.shutdown().await;
}
