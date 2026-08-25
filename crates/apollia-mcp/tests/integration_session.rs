#![allow(clippy::unwrap_used, clippy::expect_used)]
use std::collections::HashMap;

use apollia_mcp::config::McpServerConfig;
use apollia_mcp::protocol::ToolCallContent;
use apollia_mcp::session::{LoadingMode, McpSession, McpSessionError};

fn mock_server_config() -> McpServerConfig {
    McpServerConfig {
        format_version: 1,
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
        max_response_bytes: 8 * 1024 * 1024,
        max_tools: 256,
        tags: vec!["test".to_string()],
    }
}

fn crash_server_config() -> McpServerConfig {
    McpServerConfig {
        format_version: 1,
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
        max_response_bytes: 8 * 1024 * 1024,
        max_tools: 256,
        tags: vec![],
    }
}

/// Config for the mock server that exits after its second `tools/list`,
/// used to prove the deferred-mode schema cache.
fn deferred_server_config() -> McpServerConfig {
    McpServerConfig {
        format_version: 1,
        name: "deferred".to_string(),
        command: "python3".to_string(),
        args: vec![format!(
            "{}/tests/mock_mcp_server_deferred.py",
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

/// The session must surface the server-level `instructions` from `initialize`.
#[tokio::test]
async fn test_mock_server_exposes_instructions() {
    // GIVEN the mock MCP server, whose initialize response carries instructions
    let config = mock_server_config();

    // WHEN a session is started
    let session = McpSession::start(config, None).await.unwrap();

    // THEN the instructions are surfaced verbatim by the accessor
    assert_eq!(
        session.instructions(),
        Some("Use this mock server for echo and add tools.")
    );

    session.shutdown().await;
}

/// A server that omits `instructions` must surface `None` without failing.
#[tokio::test]
async fn test_server_without_instructions_returns_none() {
    // GIVEN the crash mock server, whose initialize response omits instructions
    let config = crash_server_config();

    // WHEN a session is started (handshake + tools/list succeed before the crash)
    let session = McpSession::start(config, None).await.unwrap();

    // THEN the absent field surfaces as None and the handshake still succeeded
    assert!(session.instructions().is_none());

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
        format_version: 1,
        name: "slow".to_string(),
        command: "sleep".to_string(),
        args: vec!["100".to_string()],
        env: HashMap::new(),
        transport: "stdio".to_string(),
        url: None,
        requires_approval: false,
        init_timeout_secs: 1,
        call_timeout_secs: 1,
        max_response_bytes: 8 * 1024 * 1024,
        max_tools: 256,
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

// ─── deferred loading ──────────────────────────────────────────────────────

/// Deferred start must load only the lightweight index, not the schemas.
#[tokio::test]
async fn test_deferred_start_loads_index_only() {
    // GIVEN the mock server exposing two tools
    let config = mock_server_config();

    // WHEN a session is started in deferred mode
    let session = McpSession::start_with_mode(config, None, LoadingMode::Deferred)
        .await
        .unwrap();

    // THEN the index holds both tools while the schema slice stays empty
    assert_eq!(session.tool_index().len(), 2);
    assert!(session.tools().is_empty());
    // AND the schemas that came back with that same tools/list are kept in the
    // cache rather than dropped: they cost nothing in the prompt and they are
    // what makes an indexed tool callable.
    let cached = session
        .cached_tool_schema("echo")
        .expect("boot discovery seeds the schema cache");
    assert!(cached["properties"]["message"].is_object());
    let names: Vec<&str> = session
        .tool_index()
        .iter()
        .map(|entry| entry.name.as_str())
        .collect();
    assert!(names.contains(&"echo"));
    assert!(names.contains(&"add"));

    session.shutdown().await;
}

/// fetch_tool_schema must serve every indexed tool without a round-trip.
#[tokio::test]
async fn test_deferred_fetch_schema_caches() {
    // GIVEN a deferred session against a server that dies after its 2nd tools/list
    let config = deferred_server_config();
    let mut session = McpSession::start_with_mode(config, None, LoadingMode::Deferred)
        .await
        .unwrap();

    // WHEN a schema is fetched: the boot tools/list already cached it, so no
    // second request is sent and the mock never reaches its exit condition
    let schema = session.fetch_tool_schema("echo").await.unwrap();
    // THEN it carries the expected shape
    assert_eq!(schema["type"], "object");
    assert!(schema["properties"]["message"].is_object());

    // Allow the server process to fully exit so any new request would fail
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // WHEN the same and a sibling tool are fetched again
    let echo_again = session.fetch_tool_schema("echo").await.unwrap();
    let add_schema = session.fetch_tool_schema("add").await.unwrap();
    // THEN both are served from the cache (no third tools/list to the dead server)
    assert_eq!(echo_again, schema);
    assert!(add_schema["properties"]["a"].is_object());

    session.shutdown().await;
}

/// fetch_tool_schema on an unknown tool must produce a typed error.
#[tokio::test]
async fn test_deferred_fetch_unknown_tool_errors() {
    // GIVEN a deferred session against the live mock server
    let config = mock_server_config();
    let mut session = McpSession::start_with_mode(config, None, LoadingMode::Deferred)
        .await
        .unwrap();

    // WHEN a schema is fetched for a tool absent from tools/list
    let result = session.fetch_tool_schema("does_not_exist").await;

    // THEN a SchemaFetchFailed error is returned (no panic)
    assert!(matches!(
        result,
        Err(McpSessionError::SchemaFetchFailed { .. })
    ));

    session.shutdown().await;
}

/// A transport cut after boot must surface as a typed schema-fetch error.
#[tokio::test]
async fn test_deferred_fetch_network_error_is_typed() {
    // GIVEN a deferred session against the crash server, which exits right after
    // the boot tools/list that builds the index
    let config = crash_server_config();
    let mut session = McpSession::start_with_mode(config, None, LoadingMode::Deferred)
        .await
        .unwrap();
    assert_eq!(session.tool_index().len(), 1);

    // Allow the server process to fully exit before the on-demand fetch
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // WHEN a schema is fetched for a tool the boot index never saw, forcing the
    // on-demand tools/list onto the dead process
    let result = session.fetch_tool_schema("appeared_after_boot").await;

    // THEN the network failure surfaces as a typed SchemaFetchFailed, not a panic
    assert!(matches!(
        result,
        Err(McpSessionError::SchemaFetchFailed { .. })
    ));

    // AND an indexed tool is still served, because its schema was cached at
    // boot: a server that dies does not make what it already published
    // unreachable
    let indexed = session.fetch_tool_schema("echo").await.unwrap();
    assert_eq!(indexed["type"], "object");

    session.shutdown().await;
}

/// Eager mode is unchanged: schemas load at boot and fetch needs no round-trip.
#[tokio::test]
async fn test_eager_fetch_schema_uses_loaded_tools() {
    // GIVEN an eager session (default) against the live mock server
    let mut session = McpSession::start(mock_server_config(), None).await.unwrap();
    assert_eq!(session.tools().len(), 2);
    assert!(session.tool_index().is_empty());

    // WHEN a known schema is fetched
    let schema = session.fetch_tool_schema("echo").await.unwrap();
    // THEN it is served from the already-loaded tools
    assert_eq!(schema["type"], "object");

    // WHEN an unknown schema is fetched
    let missing = session.fetch_tool_schema("nope").await;
    // THEN it is a typed SchemaFetchFailed
    assert!(matches!(
        missing,
        Err(McpSessionError::SchemaFetchFailed { .. })
    ));

    session.shutdown().await;
}
