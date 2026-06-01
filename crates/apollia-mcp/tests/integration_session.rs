use std::collections::HashMap;

use apollia_mcp::config::McpServerConfig;
use apollia_mcp::protocol::ToolCallContent;
use apollia_mcp::session::{McpSession, McpSessionError};

fn mock_server_config() -> McpServerConfig {
    McpServerConfig {
        name: "mock".to_string(),
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
        tags: vec!["test".to_string()],
    }
}

fn crash_server_config() -> McpServerConfig {
    McpServerConfig {
        name: "crash".to_string(),
        command: "python3".to_string(),
        args: vec![format!(
            "{}/tests/mock_mcp_server_crash.py",
            env!("CARGO_MANIFEST_DIR")
        )],
        env: HashMap::new(),
        transport: "stdio".to_string(),
        url: None,
        requires_approval: false,
        init_timeout_secs: 10,
        call_timeout_secs: 1,
        tags: vec![],
    }
}

/// The mock MCP server must complete the initialize handshake and expose its tools.
#[tokio::test]
async fn test_mock_server_handshake() {
    // GIVEN the mock MCP server
    let config = mock_server_config();

    // WHEN a session is started
    let session = McpSession::start(config, None).await.unwrap();

    // THEN the handshake succeeded and both tools were discovered
    assert_eq!(session.tools().len(), 2);
    assert_eq!(session.server_info().name, "mock-mcp-server");
    assert!(session.pid().is_some());

    session.shutdown().await;
}

/// The echo tool must return the exact string passed in the message argument.
#[tokio::test]
async fn test_full_flow_echo_tool_call() {
    // GIVEN a running session with the mock server
    let session = McpSession::start(mock_server_config(), None).await.unwrap();

    // WHEN the echo tool is called with a message
    let result = session
        .call_tool("echo", Some(serde_json::json!({"message": "hello"})))
        .await
        .unwrap();

    // THEN the response content matches the sent message
    assert_eq!(result.content.len(), 1);
    assert!(matches!(
        &result.content[0],
        ToolCallContent::Text { text } if text == "hello"
    ));
    assert_eq!(result.is_error, Some(false));

    session.shutdown().await;
}

/// The add tool must return the correct numeric sum of its arguments.
#[tokio::test]
async fn test_full_flow_add_tool_call() {
    // GIVEN a running session with the mock server
    let session = McpSession::start(mock_server_config(), None).await.unwrap();

    // WHEN the add tool is called with two numbers
    let result = session
        .call_tool("add", Some(serde_json::json!({"a": 2, "b": 3})))
        .await
        .unwrap();

    // THEN the response contains the correct sum as a text string
    assert_eq!(result.content.len(), 1);
    assert!(matches!(
        &result.content[0],
        ToolCallContent::Text { text } if text == "5"
    ));

    session.shutdown().await;
}

/// A server that never writes to stdout must cause an initialize timeout.
#[tokio::test]
async fn test_initialize_timeout_on_slow_server() {
    // GIVEN a process that idles without producing any stdout output (sleep)
    let config = McpServerConfig {
        name: "slow".to_string(),
        command: "sleep".to_string(),
        args: vec!["100".to_string()],
        env: HashMap::new(),
        transport: "stdio".to_string(),
        url: None,
        requires_approval: false,
        init_timeout_secs: 1,
        call_timeout_secs: 1,
        tags: vec![],
    };

    // WHEN a session start is attempted
    let result = McpSession::start(config, None).await;

    // THEN the initialize handshake times out
    assert!(matches!(
        result,
        Err(McpSessionError::InitializeTimeout { .. })
    ));
}

/// A tool call on a server that has exited must return an error.
#[tokio::test]
async fn test_server_crash_returns_error_on_tool_call() {
    // GIVEN a server that exits immediately after responding to tools/list
    let config = crash_server_config();

    // WHEN the session is started - succeeds because the crash happens after tools/list
    let session = McpSession::start(config, None).await.unwrap();
    assert_eq!(session.tools().len(), 1);

    // Allow the server process to fully exit before calling a tool
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // WHEN a tool call is attempted on the dead server
    let result = session
        .call_tool("echo", Some(serde_json::json!({"message": "crash?"})))
        .await;

    // THEN an error is returned (the server has exited or the call timed out)
    assert!(result.is_err());

    session.shutdown().await;
}
