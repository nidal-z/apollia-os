//! MCP stdio server: exposes native Apollia tools to external MCP clients.
//!
//! [`McpStdioServer`] reads newline-delimited JSON-RPC 2.0 messages from stdin,
//! dispatches them to the appropriate handler, and writes responses to stdout.
//!
//! Supported methods:
//! - `initialize`: handshake, returns server capabilities
//! - `tools/list`: enumerate the 9 native tools (+ `submit_task` when runtime present)
//! - `tools/call`: invoke a tool via [`ToolDispatcher`]
//!
//! The server handles malformed JSON gracefully: a parse error produces a
//! JSON-RPC error response with code `-32700` instead of crashing.
//!
//! # Runtime integration
//!
//! To expose `submit_task`, callers inject a [`SubmitTaskHandler`] implementation.
//! This keeps `apollia-mcp` independent of `apollia-runtime` (which already
//! depends on this crate).

use std::sync::Arc;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use apollia_tools::executor::{ToolDispatcher, ToolExecutionError};

use crate::server_tools::{native_tool_definitions, submit_task_definition};
use crate::server_types::{parse_request, CallToolParams, McpRequest};

// ─── SubmitTaskHandler trait ─────────────────────────────────────────────────

/// Abstraction over the runtime task-submission path.
///
/// Implement this trait to inject `submit_task` capability into [`McpStdioServer`]
/// without creating a dependency on `apollia-runtime`.
///
/// The `apollia-cli` crate provides a concrete implementation backed by
/// `RuntimeHandle::router_handle`.
#[async_trait::async_trait]
pub trait SubmitTaskHandler: Send + Sync {
    /// Submit a task to the named agent and return the assigned task ID.
    ///
    /// # Errors
    ///
    /// Returns a human-readable error string when the task cannot be submitted.
    async fn submit(&self, task: String, agent_id: String) -> Result<String, String>;
}

// ─── Error type ──────────────────────────────────────────────────────────────

/// Errors that can terminate the MCP stdio server loop.
#[derive(Debug, thiserror::Error)]
pub enum McpServerError {
    /// I/O failure on stdin or stdout.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

// ─── Server ──────────────────────────────────────────────────────────────────

/// MCP stdio server that exposes native Apollia tools to external clients.
///
/// External MCP clients (Claude Desktop, VS Code Copilot Chat, Cursor) connect
/// by spawning this process and communicating over stdin/stdout using JSON-RPC 2.0.
///
/// Construct via [`McpStdioServer::new`]. Inject a [`SubmitTaskHandler`] via
/// [`with_submit_task`](Self::with_submit_task) to expose the optional
/// `submit_task` tool. Call [`run`](Self::run) to enter the read-dispatch-write
/// loop.
pub struct McpStdioServer {
    dispatcher: ToolDispatcher,
    /// When `Some`, `submit_task` is advertised in `tools/list` and routable.
    submit_task: Option<Arc<dyn SubmitTaskHandler>>,
}

impl McpStdioServer {
    /// Create a new server with the given tool dispatcher.
    ///
    /// Without a `SubmitTaskHandler`, `submit_task` is not advertised.
    pub fn new(dispatcher: ToolDispatcher) -> Self {
        Self {
            dispatcher,
            submit_task: None,
        }
    }

    /// Attach a `submit_task` handler, enabling the 10th MCP tool.
    ///
    /// Call this when starting the server with `--with-runtime`.
    pub fn with_submit_task(mut self, handler: Arc<dyn SubmitTaskHandler>) -> Self {
        self.submit_task = Some(handler);
        self
    }

    /// Enter the stdin → dispatch → stdout loop.
    ///
    /// Reads lines from stdin. Each line is parsed as a JSON-RPC 2.0 message.
    /// Malformed JSON produces a `-32700` error response. Unknown methods produce
    /// a `-32601` response. The loop exits cleanly when stdin reaches EOF.
    ///
    /// # Errors
    ///
    /// Returns [`McpServerError::Io`] only on unrecoverable I/O errors on stdin
    /// or stdout. JSON parsing and method routing errors are returned to the
    /// client as JSON-RPC error responses; they never terminate the loop.
    pub async fn run(self) -> Result<(), McpServerError> {
        let stdin = tokio::io::stdin();
        let mut lines = BufReader::new(stdin).lines();
        let mut stdout = tokio::io::stdout();

        while let Some(line) = lines.next_line().await? {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            let response = match serde_json::from_str::<serde_json::Value>(trimmed) {
                Err(_) => jsonrpc_error(serde_json::Value::Null, -32700, "Parse error"),
                Ok(raw) => match parse_request(raw) {
                    Err(id) => jsonrpc_error(id, -32601, "Method not found"),
                    Ok(McpRequest::Notification { .. }) => {
                        // Notifications require no response.
                        continue;
                    }
                    Ok(req) => self.handle_request(req).await,
                },
            };

            let mut payload = serde_json::to_string(&response).unwrap_or_else(|_| {
                r#"{"jsonrpc":"2.0","id":null,"error":{"code":-32603,"message":"Internal error"}}"#
                    .to_string()
            });
            payload.push('\n');
            stdout.write_all(payload.as_bytes()).await?;
            stdout.flush().await?;
        }

        Ok(())
    }

    /// Dispatch a parsed request to the correct handler.
    async fn handle_request(&self, request: McpRequest) -> serde_json::Value {
        match request {
            McpRequest::Initialize { id, .. } => self.handle_initialize(id),
            McpRequest::ListTools { id } => self.handle_list_tools(id).await,
            McpRequest::CallTool { id, params } => self.handle_call_tool(id, params).await,
            McpRequest::Notification { .. } => serde_json::Value::Null,
        }
    }

    /// Respond to the `initialize` handshake.
    fn handle_initialize(&self, id: serde_json::Value) -> serde_json::Value {
        jsonrpc_ok(
            id,
            serde_json::json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {
                    "tools": {}
                },
                "serverInfo": {
                    "name": "apollia-os",
                    "version": env!("CARGO_PKG_VERSION")
                }
            }),
        )
    }

    /// Respond to `tools/list` with all available tool schemas.
    ///
    /// The base list contains the 9 native tools. When a [`SubmitTaskHandler`]
    /// is configured, `submit_task` is appended as the 10th tool.
    async fn handle_list_tools(&self, id: serde_json::Value) -> serde_json::Value {
        let mut tools = native_tool_definitions();

        if self.submit_task.is_some() {
            tools.push(submit_task_definition());
        }

        jsonrpc_ok(id, serde_json::json!({ "tools": tools }))
    }

    /// Respond to `tools/call` by routing to the appropriate handler.
    async fn handle_call_tool(
        &self,
        id: serde_json::Value,
        params: CallToolParams,
    ) -> serde_json::Value {
        match params.name.as_str() {
            "submit_task" => self.handle_submit_task(id, params.arguments).await,
            "mcp_client" => jsonrpc_error(
                id,
                -32000,
                "mcp_client requires a running Apollia runtime with mcp.toml configuration",
            ),
            "agent_install" => jsonrpc_error(
                id,
                -32000,
                "agent_install requires a running Apollia runtime",
            ),
            tool_name => self.dispatch_to_tool(id, tool_name, params.arguments).await,
        }
    }

    /// Route a call to the native tool dispatcher.
    async fn dispatch_to_tool(
        &self,
        id: serde_json::Value,
        tool_name: &str,
        arguments: serde_json::Value,
    ) -> serde_json::Value {
        let arguments = enrich_arguments(tool_name, arguments);
        match self.dispatcher.dispatch(tool_name, arguments).await {
            Ok(output) => {
                let text = value_to_text(&output);
                jsonrpc_ok(
                    id,
                    serde_json::json!({
                        "content": [{ "type": "text", "text": text }]
                    }),
                )
            }
            Err(ToolExecutionError::UnknownTool { name }) => {
                jsonrpc_error(id, -32000, &format!("unknown tool: '{}'", name))
            }
            Err(ToolExecutionError::InvalidInput { message }) => {
                jsonrpc_error(id, -32602, &format!("invalid input: {}", message))
            }
            Err(ToolExecutionError::PermissionDenied { reason }) => {
                jsonrpc_error(id, -32000, &format!("permission denied: {}", reason))
            }
            Err(e) => jsonrpc_error(id, -32000, &format!("execution error: {}", e)),
        }
    }

    /// Handle a `submit_task` call by delegating to the injected handler.
    async fn handle_submit_task(
        &self,
        id: serde_json::Value,
        args: serde_json::Value,
    ) -> serde_json::Value {
        let Some(handler) = &self.submit_task else {
            return jsonrpc_error(
                id,
                -32000,
                "submit_task requires a runtime — start with --with-runtime",
            );
        };

        let task = match args.get("task").and_then(|v| v.as_str()) {
            Some(t) => t.to_string(),
            None => return jsonrpc_error(id, -32602, "missing required parameter: task"),
        };
        let agent_id = args
            .get("agent_id")
            .and_then(|v| v.as_str())
            .unwrap_or("default")
            .to_string();

        match handler.submit(task, agent_id).await {
            Ok(task_id) => jsonrpc_ok(
                id,
                serde_json::json!({
                    "content": [{
                        "type": "text",
                        "text": format!("Task submitted. task_id={}", task_id)
                    }]
                }),
            ),
            Err(e) => jsonrpc_error(id, -32000, &format!("submit_task failed: {}", e)),
        }
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Build a successful JSON-RPC 2.0 response.
fn jsonrpc_ok(id: serde_json::Value, result: serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": result
    })
}

/// Build a JSON-RPC 2.0 error response.
fn jsonrpc_error(id: serde_json::Value, code: i64, message: &str) -> serde_json::Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {
            "code": code,
            "message": message
        }
    })
}

/// Inject MCP-layer defaults into tool arguments before dispatching.
///
/// `bash_executor` requires `timeout_secs`; the MCP schema marks it as optional
/// with a default of 30. This function fills in the field when absent so MCP
/// clients that follow the schema do not receive a spurious `InvalidInput` error.
fn enrich_arguments(tool_name: &str, mut args: serde_json::Value) -> serde_json::Value {
    if tool_name == "bash_executor" {
        if let Some(obj) = args.as_object_mut() {
            obj.entry("timeout_secs").or_insert(serde_json::json!(30));
        }
    }
    args
}

/// Convert a tool output value to a compact text representation.
///
/// When the value is already a string, it is returned verbatim.
/// Otherwise it is serialized to compact JSON.
fn value_to_text(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use apollia_tools::executor::ToolDispatcher;
    use apollia_tools::tools::bash_executor::BashExecutor;

    fn make_dispatcher() -> ToolDispatcher {
        ToolDispatcher::new(vec![Box::new(BashExecutor::new())])
    }

    fn make_server() -> McpStdioServer {
        McpStdioServer::new(make_dispatcher())
    }

    // initialize

    #[tokio::test]
    async fn mcp_server_handles_initialize() {
        // GIVEN
        let server = make_server();
        let req = McpRequest::Initialize {
            id: serde_json::json!(1),
            params: crate::server_types::InitializeParams {
                protocol_version: "2024-11-05".to_string(),
                client_info: serde_json::json!({ "name": "test" }),
            },
        };
        // WHEN
        let response = server.handle_request(req).await;
        // THEN
        assert_eq!(response["result"]["protocolVersion"], "2024-11-05");
        assert_eq!(response["result"]["serverInfo"]["name"], "apollia-os");
    }

    // 9 tools without runtime

    #[tokio::test]
    async fn mcp_server_lists_9_native_tools() {
        // GIVEN
        let server = make_server();
        let req = McpRequest::ListTools {
            id: serde_json::json!(2),
        };
        // WHEN
        let response = server.handle_request(req).await;
        // THEN
        let tools = response["result"]["tools"]
            .as_array()
            .expect("tools must be an array");
        assert_eq!(tools.len(), 9, "expected 9 tools without runtime");
        let has_submit = tools.iter().any(|t| t["name"] == "submit_task");
        assert!(!has_submit, "submit_task must not appear without runtime");
    }

    // 10 tools with runtime

    #[tokio::test]
    async fn mcp_server_lists_submit_task_with_runtime() {
        // GIVEN: inject a stub SubmitTaskHandler to simulate the runtime path
        struct StubSubmit;

        #[async_trait::async_trait]
        impl SubmitTaskHandler for StubSubmit {
            async fn submit(&self, _task: String, _agent_id: String) -> Result<String, String> {
                Ok("stub-task-id".to_string())
            }
        }

        let server = McpStdioServer::new(make_dispatcher()).with_submit_task(Arc::new(StubSubmit));

        let req = McpRequest::ListTools {
            id: serde_json::json!(3),
        };
        // WHEN
        let response = server.handle_request(req).await;
        // THEN
        let tools = response["result"]["tools"]
            .as_array()
            .expect("tools must be an array");
        assert_eq!(tools.len(), 10, "expected 10 tools with runtime");
        let has_submit = tools.iter().any(|t| t["name"] == "submit_task");
        assert!(has_submit, "submit_task must appear with runtime");
    }

    // bash_executor call

    #[tokio::test]
    async fn mcp_server_calls_bash_executor() {
        // GIVEN
        let server = make_server();
        let req = McpRequest::CallTool {
            id: serde_json::json!(4),
            params: crate::server_types::CallToolParams {
                name: "bash_executor".to_string(),
                arguments: serde_json::json!({ "command": "echo hello" }),
            },
        };
        // WHEN
        let response = server.handle_request(req).await;
        // THEN: result contains content[0].text with "hello"
        let text = response["result"]["content"][0]["text"]
            .as_str()
            .expect("content[0].text must be a string");
        assert!(
            text.contains("hello"),
            "expected 'hello' in output, got: {text}"
        );
    }

    // unknown tool yields -32000

    #[tokio::test]
    async fn mcp_server_returns_error_on_unknown_tool() {
        // GIVEN
        let server = make_server();
        let req = McpRequest::CallTool {
            id: serde_json::json!(5),
            params: crate::server_types::CallToolParams {
                name: "inexistant_tool".to_string(),
                arguments: serde_json::json!({}),
            },
        };
        // WHEN
        let response = server.handle_request(req).await;
        // THEN
        assert!(response.get("error").is_some(), "expected error field");
        assert_eq!(response["error"]["code"], -32000);
    }

    // parse error yields -32700

    #[test]
    fn mcp_server_handles_parse_error() {
        // GIVEN the parse error response as produced by the run() loop
        let response = jsonrpc_error(serde_json::Value::Null, -32700, "Parse error");
        // WHEN / THEN
        assert_eq!(response["error"]["code"], -32700);
        assert_eq!(response["error"]["message"], "Parse error");
    }
}
