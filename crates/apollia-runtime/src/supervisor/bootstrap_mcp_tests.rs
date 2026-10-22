//! The MCP manager the boot starts launches no server: the API opens before
//! any handshake, whatever the servers answer.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use apollia_mcp::config::McpServerConfig;
use apollia_mcp::server_repository::StoredTool;
use apollia_mcp::session::LoadingMode;
use apollia_mcp::McpServerRepository;
use apollia_tools::registry::ToolRegistryHandle;

use super::start_mcp_manager;

fn server(name: &str, command: &str, args: Vec<String>) -> McpServerConfig {
    McpServerConfig {
        format_version: 1,
        name: name.to_string(),
        command: command.to_string(),
        args,
        env: HashMap::new(),
        transport: "stdio".to_string(),
        url: None,
        init_timeout_secs: 5,
        call_timeout_secs: 5,
        max_response_bytes: 8 * 1024 * 1024,
        max_tools: 256,
        tags: vec![],
        tool_families: Default::default(),
        operator_families: Default::default(),
        origin: None,
    }
}

fn reachable(name: &str) -> McpServerConfig {
    server(
        name,
        "python3",
        vec![concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../apollia-mcp/tests/mock_mcp_server.py"
        )
        .to_string()],
    )
}

/// A process that starts and never answers its handshake.
fn silent(name: &str) -> McpServerConfig {
    server(
        name,
        "sh",
        vec!["-c".to_string(), "exec sleep 30".to_string()],
    )
}

/// A remote server whose address takes no connection.
fn unreachable_remote(name: &str) -> McpServerConfig {
    let mut config = server(name, "", vec![]);
    config.transport = "streamable-http".to_string();
    config.url = Some("http://10.255.255.1:9/mcp".to_string());
    config
}

fn echo_tool() -> StoredTool {
    StoredTool {
        name: "echo".to_string(),
        title: None,
        description: Some("Echo a message".to_string()),
        input_schema: serde_json::json!({"type": "object"}),
        annotations: None,
        fetched_at: "2026-10-22T00:00:00Z".to_string(),
    }
}

#[tokio::test]
async fn the_mcp_manager_of_the_boot_answers_before_any_handshake() {
    // GIVEN ten servers saved in mcp.db with the tools they last published,
    //       three of which do not answer
    let dir = tempfile::tempdir().expect("dir");
    let repo = McpServerRepository::open(&dir.path().join("mcp.db")).expect("repo");
    let mut servers: Vec<McpServerConfig> = (0..7)
        .map(|i| reachable(&format!("reachable-{i}")))
        .collect();
    servers.push(silent("silent-0"));
    servers.push(silent("silent-1"));
    servers.push(unreachable_remote("remote-0"));
    for config in &servers {
        repo.save(config).expect("save");
        repo.replace_tools(&config.name, &[echo_tool()])
            .expect("keep");
    }
    let registry = ToolRegistryHandle::start();
    let (bus, _) = tokio::sync::broadcast::channel(64);

    // WHEN the boot starts the MCP manager, and the API reads it
    let begun = Instant::now();
    let handle = start_mcp_manager(
        servers,
        &registry,
        &bus,
        dir.path(),
        (LoadingMode::Deferred, Duration::from_secs(900)),
    )
    .await
    .expect("manager");
    let statuses = handle.status().await;
    let index = handle.get_tool_index().await;
    let elapsed = begun.elapsed();

    // THEN it answered in under two seconds, no server was started, and the
    //      ten kept tools are listed
    assert!(elapsed < Duration::from_secs(2), "took {elapsed:?}");
    assert!(statuses.is_empty(), "{statuses:?}");
    assert_eq!(index.len(), 10);
    handle.shutdown().await;
}
