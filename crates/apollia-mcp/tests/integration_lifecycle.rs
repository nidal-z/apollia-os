//! What the manager announces when a session starts, restarts, or ends on
//! its own.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use std::collections::HashMap;
use std::time::Duration;

use apollia_core::{McpHealth, RuntimeEvent};
use apollia_tools::ToolRegistryHandle;

use apollia_mcp::config::McpServerConfig;
use apollia_mcp::manager::McpClientManagerHandle;
use apollia_mcp::session::LoadingMode;

fn mock(name: &str, script: &str) -> McpServerConfig {
    McpServerConfig {
        format_version: 1,
        name: name.to_string(),
        command: "python3".to_string(),
        args: vec![format!("{}/tests/{script}", env!("CARGO_MANIFEST_DIR"))],
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

/// The next health change the bus carries for `server`, within ten seconds.
async fn next_health(
    events: &mut tokio::sync::broadcast::Receiver<RuntimeEvent>,
    server: &str,
) -> Option<McpHealth> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        let event = tokio::time::timeout_at(deadline, events.recv())
            .await
            .ok()?
            .ok()?;
        if let RuntimeEvent::McpServerHealthChanged { name, health } = event {
            if name == server {
                return Some(health);
            }
        }
    }
}

#[tokio::test]
async fn test_a_server_that_exits_on_its_own_is_marked_unavailable_and_announced() {
    // GIVEN a manager on a bus, with a server that exits right after its
    // handshake
    let (bus, mut events) = tokio::sync::broadcast::channel(64);
    let registry = ToolRegistryHandle::start();
    let manager = McpClientManagerHandle::start(
        vec![mock("crash", "mock_mcp_server_crash.py")],
        &registry,
        Some(bus),
        LoadingMode::Eager,
    )
    .await
    .expect("manager");

    // WHEN its transport closes: the first announcement is its start, the
    // next one is what the end of its dispatch task says
    let started = next_health(&mut events, "crash").await;
    let ended = next_health(&mut events, "crash").await;

    // THEN it is announced unavailable, and the status says the same
    assert_eq!(started, Some(McpHealth::Healthy { verified: false }));
    assert_eq!(
        ended,
        Some(McpHealth::Unavailable {
            reason: "process_exited".to_string()
        })
    );
    let status = manager.status().await;
    assert!(matches!(status[0].health, McpHealth::Unavailable { .. }));
    manager.shutdown().await;
}

#[tokio::test]
async fn test_a_restart_announces_the_health_of_the_new_session() {
    // GIVEN a manager on a bus, with one server whose start was announced
    let (bus, mut events) = tokio::sync::broadcast::channel(64);
    let registry = ToolRegistryHandle::start();
    let manager = McpClientManagerHandle::start(
        vec![mock("plain", "mock_mcp_server.py")],
        &registry,
        Some(bus),
        LoadingMode::Eager,
    )
    .await
    .expect("manager");
    assert!(next_health(&mut events, "plain").await.is_some());

    // WHEN it is restarted
    manager.restart_server("plain").await.expect("restarted");

    // THEN the restart announces the health of the new session
    assert_eq!(
        next_health(&mut events, "plain").await,
        Some(McpHealth::Healthy { verified: false })
    );
    manager.shutdown().await;
}
