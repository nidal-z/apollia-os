//! A server a request brings: connected over a transport the caller built,
//! its tools unclassified whatever it declares, gone from the registry when
//! it is removed.

use std::time::Duration;

use apollia_tools::{ActionFamily, ToolRegistryHandle};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Router;
use serde_json::{json, Value};

use crate::config::McpServerConfig;
use crate::manager::McpClientManagerHandle;
use crate::session::{LoadingMode, McpSessionError};
use crate::transport::http::tests::start_server;
use crate::transport::StreamableHttpTransport;

const NAME: &str = "request-mcp:127.0.0.1";

/// A streamable-HTTP MCP server with one tool it declares read-only.
async fn answer(body: String) -> axum::response::Response {
    let message: Value = serde_json::from_str(&body).unwrap_or(Value::Null);
    let result = match message["method"].as_str().unwrap_or_default() {
        "initialize" => json!({
            "protocolVersion": "2025-03-26",
            "capabilities": {"tools": {}},
            "serverInfo": {"name": "fake", "version": "1"}
        }),
        "tools/list" => json!({"tools": [{
            "name": "echo",
            "inputSchema": {"type": "object"},
            "annotations": {"readOnlyHint": true}
        }]}),
        _ => return StatusCode::ACCEPTED.into_response(),
    };
    axum::Json(json!({"jsonrpc": "2.0", "id": message["id"], "result": result})).into_response()
}

fn config(url: &str) -> McpServerConfig {
    McpServerConfig {
        format_version: 1,
        name: NAME.to_owned(),
        command: String::new(),
        args: vec![],
        env: std::collections::HashMap::new(),
        transport: "streamable-http".to_owned(),
        url: Some(url.to_owned()),
        init_timeout_secs: 5,
        call_timeout_secs: 5,
        max_response_bytes: 8 * 1024 * 1024,
        max_tools: 256,
        tags: vec![],
        tool_families: Default::default(),
        origin: None,
    }
}

fn transport(url: &str) -> Box<StreamableHttpTransport> {
    let client = apollia_core::net::loopback_client_builder()
        .build()
        .expect("client");
    Box::new(StreamableHttpTransport::with_client(
        client,
        url,
        vec![],
        Duration::from_secs(5),
        8 * 1024 * 1024,
    ))
}

#[tokio::test]
async fn test_a_request_server_is_unclassified_and_leaves_the_registry_with_its_session() {
    // GIVEN a manager with no server, and an MCP server whose one tool
    // declares itself read-only
    let addr = start_server(Router::new().fallback(answer)).await;
    let url = format!("http://{addr}/mcp");
    let registry = ToolRegistryHandle::start();
    let manager = McpClientManagerHandle::start(vec![], &registry, None, LoadingMode::Eager)
        .await
        .expect("manager");

    // WHEN it is connected as a server a request brought
    let status = manager
        .add_request_server(config(&url), transport(&url))
        .await
        .expect("connected");

    // THEN its tool is registered unclassified, the hint set aside
    assert_eq!(status.tools_count, 1);
    let tool = registry
        .get("mcp:request-mcp:127.0.0.1/echo")
        .await
        .expect("registry")
        .expect("registered");
    assert_eq!(tool.family, ActionFamily::Unclassified);
    assert!(!tool.is_read_only);

    // AND it can be neither restarted nor reloaded, nor connected twice
    assert!(manager.restart_server(NAME).await.is_err());
    assert!(manager.reload_server(NAME).await.is_err());
    let twice = manager
        .add_request_server(config(&url), transport(&url))
        .await;
    assert!(matches!(
        twice,
        Err(McpSessionError::InitializeFailed { .. })
    ));
    assert_eq!(manager.status().await.len(), 1);

    // WHEN it is removed
    manager.remove_server(NAME).await.expect("removed");

    // THEN neither the session nor the tool is left
    assert!(manager.status().await.is_empty());
    assert!(registry
        .get("mcp:request-mcp:127.0.0.1/echo")
        .await
        .expect("registry")
        .is_none());
}
