//! MCP stdio server: a bridge from an external MCP client to the daemon.
//!
//! [`McpStdioServer`] reads newline-delimited JSON-RPC 2.0 messages from stdin,
//! answers them, and writes the responses to stdout. It holds no tool of its
//! own: what it lists and what it calls is asked of the daemon through an
//! [`McpBridge`], which serves the agents and workflows the user exposed, and
//! nothing else. The daemon decides; this loop relays.
//!
//! Supported methods:
//! - `initialize`: handshake, returns server capabilities
//! - `tools/list`: the exposed items, as the daemon lists them
//! - `tools/call`: runs one exposed item through the daemon and returns its
//!   final result
//!
//! The server handles malformed JSON gracefully: a parse error produces a
//! JSON-RPC error response with code `-32700` instead of crashing.
//!
//! The bridge is a trait so that this crate stays independent of the HTTP
//! client of the CLI and of `apollia-runtime` (which depends on this crate).

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use crate::server_types::{parse_request, CallToolParams, McpRequest};

/// A boxed future, the return of the object-safe [`McpBridge`] methods.
pub type BridgeFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Why the daemon did not list or run what the client asked.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum BridgeError {
    /// The daemon refuses the bridge: the user has not turned it on.
    #[error("{0}")]
    Refused(String),
    /// No exposed item carries this tool name.
    #[error("{0}")]
    UnknownTool(String),
    /// The arguments do not fit the tool.
    #[error("{0}")]
    InvalidArguments(String),
    /// The daemon could not be reached, or failed.
    #[error("{0}")]
    Unavailable(String),
}

impl BridgeError {
    /// The JSON-RPC error code the client receives.
    fn code(&self) -> i64 {
        match self {
            BridgeError::UnknownTool(_) | BridgeError::InvalidArguments(_) => -32602,
            BridgeError::Refused(_) | BridgeError::Unavailable(_) => -32000,
        }
    }
}

/// The end of a call: the final text of the run, and whether the run failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BridgeCallOutcome {
    /// The answer of the agent, or what the workflow returned; the failure
    /// when the run did not complete.
    pub text: String,
    /// `true` when the run failed or was stopped. The client receives it as
    /// a tool result with `isError`, as MCP asks for an execution failure.
    pub is_error: bool,
}

/// What the stdio loop asks the daemon.
///
/// The `apollia-cli` crate implements it over the runtime HTTP API.
pub trait McpBridge: Send + Sync {
    /// The exposed items, each an MCP tool descriptor
    /// (`{name, description, inputSchema}`).
    fn list_tools(&self) -> BridgeFuture<'_, Result<Vec<serde_json::Value>, BridgeError>>;

    /// Runs the exposed item behind the tool `name` on `arguments`, to its
    /// end.
    fn call_tool(
        &self,
        name: String,
        arguments: serde_json::Value,
    ) -> BridgeFuture<'_, Result<BridgeCallOutcome, BridgeError>>;
}

/// Errors that can terminate the MCP stdio server loop.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum McpServerError {
    /// I/O failure on stdin or stdout.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

/// MCP stdio server that relays an external client to the daemon.
///
/// External MCP clients (editors, other assistants) connect by spawning this
/// process and communicating over stdin/stdout using JSON-RPC 2.0.
pub struct McpStdioServer {
    bridge: Arc<dyn McpBridge>,
}

impl McpStdioServer {
    /// Create a server that asks `bridge` for every list and call.
    pub fn new(bridge: Arc<dyn McpBridge>) -> Self {
        Self { bridge }
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
            let Some(response) = self.handle_line(&line).await else {
                continue;
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

    /// The response to one line read from the client, `None` for a blank
    /// line or a notification.
    async fn handle_line(&self, line: &str) -> Option<serde_json::Value> {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return None;
        }
        match serde_json::from_str::<serde_json::Value>(trimmed) {
            Err(_) => Some(jsonrpc_error(
                serde_json::Value::Null,
                -32700,
                "Parse error",
            )),
            Ok(raw) => match parse_request(raw) {
                Err(id) => Some(jsonrpc_error(id, -32601, "Method not found")),
                Ok(McpRequest::Notification { .. }) => None,
                Ok(req) => Some(self.handle_request(req).await),
            },
        }
    }

    /// Dispatch a parsed request to the correct handler.
    async fn handle_request(&self, request: McpRequest) -> serde_json::Value {
        match request {
            McpRequest::Initialize { id, .. } => handle_initialize(id),
            McpRequest::ListTools { id } => self.handle_list_tools(id).await,
            McpRequest::CallTool { id, params } => self.handle_call_tool(id, params).await,
            McpRequest::Notification { .. } => serde_json::Value::Null,
        }
    }

    /// Respond to `tools/list` with the exposed items the daemon serves.
    async fn handle_list_tools(&self, id: serde_json::Value) -> serde_json::Value {
        match self.bridge.list_tools().await {
            Ok(tools) => jsonrpc_ok(id, serde_json::json!({ "tools": tools })),
            Err(e) => jsonrpc_error(id, e.code(), &e.to_string()),
        }
    }

    /// Respond to `tools/call` with the final result of the run.
    async fn handle_call_tool(
        &self,
        id: serde_json::Value,
        params: CallToolParams,
    ) -> serde_json::Value {
        match self.bridge.call_tool(params.name, params.arguments).await {
            Ok(outcome) => jsonrpc_ok(
                id,
                serde_json::json!({
                    "content": [{ "type": "text", "text": outcome.text }],
                    "isError": outcome.is_error,
                }),
            ),
            Err(e) => jsonrpc_error(id, e.code(), &e.to_string()),
        }
    }
}

/// Respond to the `initialize` handshake.
fn handle_initialize(id: serde_json::Value) -> serde_json::Value {
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

#[cfg(test)]
mod tests;
