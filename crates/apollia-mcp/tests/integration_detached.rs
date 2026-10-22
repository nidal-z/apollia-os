//! The manager answers while a tool call is in progress.
//!
//! Tool calls run in tasks of their own; the statuses and the tool index are
//! read from the state the actor publishes, so a slow server delays neither.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use std::collections::HashMap;
use std::time::{Duration, Instant};

use apollia_core::{McpHealth, RuntimeEvent};
use apollia_mcp::config::McpServerConfig;
use apollia_mcp::manager::McpClientManagerHandle;
use apollia_mcp::session::LoadingMode;
use apollia_tools::ToolRegistryHandle;
use futures::FutureExt;

fn slow_server_config(name: &str) -> McpServerConfig {
    McpServerConfig {
        format_version: 1,
        name: name.to_string(),
        command: "python3".to_string(),
        args: vec![format!(
            "{}/tests/mock_mcp_server_slow.py",
            env!("CARGO_MANIFEST_DIR")
        )],
        env: HashMap::new(),
        transport: "stdio".to_string(),
        url: None,
        init_timeout_secs: 10,
        call_timeout_secs: 20,
        max_response_bytes: 8 * 1024 * 1024,
        max_tools: 256,
        tags: vec![],
        tool_families: Default::default(),
        operator_families: Default::default(),
        origin: None,
    }
}

/// The time `fut` takes to complete.
async fn timed<F: std::future::Future>(fut: F) -> (F::Output, Duration) {
    let start = Instant::now();
    let out = fut.await;
    (out, start.elapsed())
}

#[tokio::test]
async fn test_status_and_tool_index_answer_during_a_five_second_call() {
    // GIVEN a manager with one deferred server whose tool takes five seconds
    let registry = ToolRegistryHandle::start();
    let manager = McpClientManagerHandle::start(
        vec![slow_server_config("slow")],
        &registry,
        None,
        LoadingMode::Deferred,
    )
    .await
    .unwrap();
    let calling = manager.clone();
    let call = tokio::spawn(async move {
        calling
            .call_tool("slow", "wait", Some(serde_json::json!({"seconds": 5})))
            .await
    });
    tokio::time::sleep(Duration::from_millis(300)).await;

    // WHEN the statuses and the tool index are read while the call runs
    let (statuses, status_took) = timed(manager.status()).await;
    let (index, index_took) = timed(manager.get_tool_index()).await;

    // THEN both answer in under 100 ms, with the server and its tool
    assert!(
        status_took < Duration::from_millis(100),
        "status took {status_took:?}"
    );
    assert!(
        index_took < Duration::from_millis(100),
        "tool index took {index_took:?}"
    );
    assert_eq!(statuses.len(), 1);
    assert_eq!(statuses[0].name, "slow");
    assert_eq!(index.len(), 1);
    assert_eq!(index[0].tool_name, "wait");
    // AND the call itself still completes
    assert!(!call.await.unwrap().unwrap().is_error.unwrap_or(false));

    manager.shutdown().await;
}

#[tokio::test]
async fn test_a_call_outcome_is_published_before_the_answer() {
    // GIVEN a manager with one eager server that no call has verified yet
    let registry = ToolRegistryHandle::start();
    let manager = McpClientManagerHandle::start(
        vec![slow_server_config("published")],
        &registry,
        None,
        LoadingMode::Eager,
    )
    .await
    .unwrap();
    let before = manager
        .status()
        .now_or_never()
        .expect("the status read waited on the actor");
    assert_eq!(before[0].health, McpHealth::Healthy { verified: false });
    assert!(before[0].last_call_at.is_none());

    // WHEN a call succeeds
    manager
        .call_tool("published", "wait", Some(serde_json::json!({"seconds": 0})))
        .await
        .unwrap();

    // THEN the state read right after the answer carries its outcome, and
    // the read completes at its first poll: it is the published state, not
    // a question to the actor, which would answer on another task
    let after = manager
        .status()
        .now_or_never()
        .expect("the status read waited on the actor");
    assert_eq!(after[0].health, McpHealth::Healthy { verified: true });
    assert!(after[0].last_call_at.is_some());

    // AND once the actor stops, the handle reads no server
    let reader = manager.clone();
    manager.shutdown().await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(reader.status().await.is_empty());
}

#[tokio::test]
async fn test_two_concurrent_calls_record_their_outcomes_in_the_order_they_end() {
    // GIVEN a manager on an event bus, with one server
    let (bus, mut events) = tokio::sync::broadcast::channel(64);
    let registry = ToolRegistryHandle::start();
    let manager = McpClientManagerHandle::start(
        vec![slow_server_config("ordered")],
        &registry,
        Some(bus),
        LoadingMode::Eager,
    )
    .await
    .unwrap();
    // The start announced its health; only the calls' outcomes are counted.
    assert!(matches!(
        events.try_recv(),
        Ok(RuntimeEvent::McpServerHealthChanged {
            health: McpHealth::Healthy { verified: false },
            ..
        })
    ));

    // WHEN a slow call that succeeds starts, then a fast call that fails
    let slow_handle = manager.clone();
    let slow = tokio::spawn(async move {
        let out = slow_handle
            .call_tool("ordered", "wait", Some(serde_json::json!({"seconds": 1.5})))
            .await;
        (out, Instant::now())
    });
    tokio::time::sleep(Duration::from_millis(100)).await;
    let fast = manager
        .call_tool(
            "ordered",
            "wait",
            Some(serde_json::json!({"seconds": 0.1, "fail": true})),
        )
        .await;
    let fast_ended = Instant::now();
    let degraded_meanwhile = manager.status().await[0].health.clone();
    let (slow, slow_ended) = slow.await.unwrap();

    // THEN the fast call answered while the slow one was still running
    assert!(fast.unwrap().is_error.unwrap_or(false));
    assert!(!slow.unwrap().is_error.unwrap_or(false));
    assert!(fast_ended < slow_ended);
    assert!(
        matches!(degraded_meanwhile, McpHealth::Degraded { .. }),
        "got {degraded_meanwhile:?}"
    );
    // AND the health moved once per outcome, in the order the calls ended
    let mut changes = Vec::new();
    while let Ok(event) = events.try_recv() {
        if let RuntimeEvent::McpServerHealthChanged { name, health } = event {
            assert_eq!(name, "ordered");
            changes.push(health);
        }
    }
    assert_eq!(changes.len(), 2, "health changes: {changes:?}");
    assert!(matches!(changes[0], McpHealth::Degraded { .. }));
    assert_eq!(changes[1], McpHealth::Healthy { verified: true });
    assert_eq!(
        manager.status().await[0].health,
        McpHealth::Healthy { verified: true }
    );

    manager.shutdown().await;
}

#[tokio::test]
async fn test_a_connection_test_does_not_hold_the_status() {
    // GIVEN a manager with one server, and a connection test of a server
    // that never answers its handshake within the read below
    let registry = ToolRegistryHandle::start();
    let manager = McpClientManagerHandle::start(
        vec![slow_server_config("listed")],
        &registry,
        None,
        LoadingMode::Eager,
    )
    .await
    .unwrap();
    let mut silent = slow_server_config("silent");
    silent.command = "sh".to_string();
    silent.args = vec!["-c".to_string(), "sleep 3".to_string()];
    silent.init_timeout_secs = 2;
    let testing = manager.clone();
    let test = tokio::spawn(async move { testing.test_connection(silent).await });
    tokio::time::sleep(Duration::from_millis(200)).await;

    // WHEN the server list is read while the test waits for its handshake
    let (statuses, took) = timed(manager.server_detail("listed")).await;

    // THEN the actor answers at once, and the test still fails on its own
    assert!(took < Duration::from_millis(100), "detail took {took:?}");
    assert_eq!(statuses.unwrap().status.name, "listed");
    assert!(test.await.unwrap().is_err());

    manager.shutdown().await;
}
