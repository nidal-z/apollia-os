//! MCP session: subprocess lifecycle, stdio pipes, JSON-RPC routing, and initialize handshake.
//!
//! Each [`McpSession`] owns one MCP server subprocess and two background Tokio tasks:
//! - a **stdin writer** that serialises outgoing JSON-RPC messages to the child's stdin,
//! - a **stdout reader** that parses incoming JSON-RPC responses and dispatches them to
//!   the caller waiting on the matching [`oneshot`] channel.
//!
//! Request/response correlation is handled via a shared `pending` map keyed by request ID.

use std::collections::HashMap;
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter};
use tokio::process::{Child, Command};
use tokio::sync::{mpsc, oneshot, Mutex};
use tracing::{debug, warn};

use crate::config::McpServerConfig;
use crate::jsonrpc::{JsonRpcNotification, JsonRpcRequest, JsonRpcResponse};
use crate::protocol::{
    ClientCapabilities, ClientInfo, InitializeParams, InitializeResult, McpToolDefinition,
    ServerCapabilities, ServerInfo, ToolCallParams, ToolCallResult, ToolsListResult,
};

// ─── errors ──────────────────────────────────────────────────────────────────

/// Errors that can arise during MCP session operations.
#[derive(Debug, thiserror::Error)]
pub enum McpSessionError {
    /// The server subprocess could not be spawned.
    #[error("failed to spawn server '{server}': {cause}")]
    SpawnFailed { server: String, cause: String },

    /// The `initialize` handshake completed but the response was malformed.
    #[error("server '{server}' initialize handshake failed: {cause}")]
    InitializeFailed { server: String, cause: String },

    /// The `initialize` handshake did not complete within the configured timeout.
    #[error("server '{server}' initialize timed out after {timeout_secs}s")]
    InitializeTimeout { server: String, timeout_secs: u64 },

    /// A `tools/call` request failed on the server side.
    #[error("server '{server}' tool call '{tool}' failed: {cause}")]
    ToolCallFailed {
        server: String,
        tool: String,
        cause: String,
    },

    /// A `tools/call` request did not complete within the configured timeout.
    #[error("server '{server}' tool call '{tool}' timed out after {timeout_secs}s")]
    ToolCallTimeout {
        server: String,
        tool: String,
        timeout_secs: u64,
    },

    /// The server process exited before the operation completed.
    #[error("server '{server}' process exited unexpectedly")]
    ServerExited { server: String },

    /// The server returned a JSON-RPC error object.
    #[error("JSON-RPC error from server '{server}': [{code}] {message}")]
    JsonRpcError {
        server: String,
        code: i64,
        message: String,
    },

    /// A JSON-RPC message could not be serialised or deserialised.
    #[error("failed to serialize/deserialize JSON-RPC message: {0}")]
    SerdeError(String),

    /// The server's stdin pipe was closed (writer task exited).
    #[error("server '{server}' stdin closed")]
    StdinClosed { server: String },
}

// ─── session ─────────────────────────────────────────────────────────────────

/// Active session with a single MCP server process.
///
/// Manages the stdio pipes, JSON-RPC message routing, and request/response
/// correlation. One session per server; owned by `McpClientManager`.
pub struct McpSession {
    /// Server configuration (name, timeouts, command, etc.).
    config: McpServerConfig,
    /// Child process handle. `kill_on_drop(true)` is set at spawn time.
    child: Child,
    /// Sender half of the channel consumed by the stdin writer task.
    stdin_tx: mpsc::Sender<String>,
    /// Pending in-flight requests: request ID → reply oneshot.
    pending: Arc<Mutex<HashMap<u64, oneshot::Sender<JsonRpcResponse>>>>,
    /// Monotonically increasing request ID counter.
    next_id: AtomicU64,
    /// Server capabilities received during the initialize handshake.
    capabilities: ServerCapabilities,
    /// Server identity received during the initialize handshake.
    server_info: ServerInfo,
    /// Tools discovered via `tools/list` (populated by `discover_tools` in the next phase).
    tools: Vec<McpToolDefinition>,
    /// Instant at which the session was successfully started.
    started_at: std::time::Instant,
    /// Background stdin writer task handle (kept alive for the session duration).
    _stdin_task: tokio::task::JoinHandle<()>,
    /// Background stdout reader task handle (kept alive for the session duration).
    _stdout_task: tokio::task::JoinHandle<()>,
}

impl McpSession {
    /// Spawn the server subprocess and perform the MCP `initialize` handshake.
    ///
    /// Resolves `${VAR}` placeholders in `config.env` before spawning.
    /// On success, returns a session that is ready to accept `tools/list` and
    /// `tools/call` requests.
    pub async fn start(config: McpServerConfig) -> Result<Self, McpSessionError> {
        let resolved_env = config
            .resolve_env()
            .map_err(|e| McpSessionError::SpawnFailed {
                server: config.name.clone(),
                cause: e.to_string(),
            })?;

        let mut child = Command::new(&config.command)
            .args(&config.args)
            .envs(resolved_env)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| McpSessionError::SpawnFailed {
                server: config.name.clone(),
                cause: e.to_string(),
            })?;

        // Both pipes are guaranteed by the Stdio::piped() builder above.
        let stdin = child.stdin.take().expect("stdin was piped");
        let stdout = child.stdout.take().expect("stdout was piped");

        let (stdin_tx, stdin_rx) = mpsc::channel::<String>(64);
        let pending: Arc<Mutex<HashMap<u64, oneshot::Sender<JsonRpcResponse>>>> =
            Arc::new(Mutex::new(HashMap::new()));

        let stdin_task = spawn_stdin_writer(stdin, stdin_rx);
        let stdout_task = spawn_stdout_reader(stdout, Arc::clone(&pending));

        let mut session = McpSession {
            config,
            child,
            stdin_tx,
            pending,
            next_id: AtomicU64::new(1),
            capabilities: ServerCapabilities {
                tools: None,
                resources: None,
                prompts: None,
            },
            server_info: ServerInfo {
                name: String::new(),
                version: None,
            },
            tools: Vec::new(),
            started_at: std::time::Instant::now(),
            _stdin_task: stdin_task,
            _stdout_task: stdout_task,
        };

        session.initialize().await?;
        session.discover_tools().await?;

        Ok(session)
    }

    /// Perform the MCP `initialize` handshake.
    ///
    /// Sends the `initialize` request with the client identity and capabilities,
    /// stores the server's capabilities and identity, then sends the
    /// `notifications/initialized` notification to complete the handshake.
    async fn initialize(&mut self) -> Result<(), McpSessionError> {
        let params = InitializeParams {
            protocol_version: "2024-11-05".to_string(),
            capabilities: ClientCapabilities {},
            client_info: ClientInfo {
                name: "apollia-runtime".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
            },
        };

        let params_value = serde_json::to_value(&params)
            .map_err(|e| McpSessionError::SerdeError(e.to_string()))?;

        let timeout_secs = self.config.init_timeout_secs;
        let result = self
            .send_request("initialize", Some(params_value), timeout_secs)
            .await?;

        let init_result: InitializeResult =
            serde_json::from_value(result).map_err(|e| McpSessionError::InitializeFailed {
                server: self.config.name.clone(),
                cause: e.to_string(),
            })?;

        debug!(
            server = %self.config.name,
            protocol_version = %init_result.protocol_version,
            "MCP initialize handshake completed"
        );

        self.capabilities = init_result.capabilities;
        self.server_info = init_result.server_info;

        self.send_notification("notifications/initialized", None)
            .await?;

        Ok(())
    }

    /// Discover tools available on this MCP server via `tools/list`.
    ///
    /// Called automatically at the end of `start()` after the `initialize` handshake.
    /// Populates the `tools` field; logs a warning when the server exposes no tools.
    async fn discover_tools(&mut self) -> Result<(), McpSessionError> {
        let timeout_secs = self.config.init_timeout_secs;
        let response = self.send_request("tools/list", None, timeout_secs).await?;

        let result: ToolsListResult =
            serde_json::from_value(response).map_err(|e| McpSessionError::InitializeFailed {
                server: self.config.name.clone(),
                cause: e.to_string(),
            })?;

        tracing::info!(
            server = %self.config.name,
            tools_count = result.tools.len(),
            "MCP tools discovered"
        );

        if result.tools.is_empty() {
            tracing::warn!(server = %self.config.name, "MCP server exposes no tools");
        }

        self.tools = result.tools;
        Ok(())
    }

    /// Send a JSON-RPC request and wait for the response, with a hard timeout.
    ///
    /// Inserts a [`oneshot::Sender`] into `pending`, writes the serialised request
    /// to the stdin writer channel, then awaits the response on the matching receiver.
    /// On timeout, the pending entry is removed to prevent map growth.
    async fn send_request(
        &self,
        method: &str,
        params: Option<serde_json::Value>,
        timeout_secs: u64,
    ) -> Result<serde_json::Value, McpSessionError> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel::<JsonRpcResponse>();

        self.pending.lock().await.insert(id, tx);

        let request = JsonRpcRequest::new(id, method, params);
        let json = serde_json::to_string(&request)
            .map_err(|e| McpSessionError::SerdeError(e.to_string()))?;

        self.stdin_tx
            .send(json)
            .await
            .map_err(|_| McpSessionError::StdinClosed {
                server: self.config.name.clone(),
            })?;

        let duration = std::time::Duration::from_secs(timeout_secs);

        match tokio::time::timeout(duration, rx).await {
            Ok(Ok(response)) => {
                if let Some(err) = response.error {
                    return Err(McpSessionError::JsonRpcError {
                        server: self.config.name.clone(),
                        code: err.code,
                        message: err.message,
                    });
                }
                response
                    .result
                    .ok_or_else(|| McpSessionError::InitializeFailed {
                        server: self.config.name.clone(),
                        cause: "server returned a response with neither result nor error"
                            .to_string(),
                    })
            }
            Ok(Err(_)) => Err(McpSessionError::ServerExited {
                server: self.config.name.clone(),
            }),
            Err(_) => {
                // Timed out — remove the stale pending entry to avoid map growth.
                self.pending.lock().await.remove(&id);
                Err(McpSessionError::InitializeTimeout {
                    server: self.config.name.clone(),
                    timeout_secs,
                })
            }
        }
    }

    /// Send a JSON-RPC notification (fire-and-forget; no response is expected).
    async fn send_notification(
        &self,
        method: &str,
        params: Option<serde_json::Value>,
    ) -> Result<(), McpSessionError> {
        let notification = JsonRpcNotification::new(method, params);
        let json = serde_json::to_string(&notification)
            .map_err(|e| McpSessionError::SerdeError(e.to_string()))?;
        self.stdin_tx
            .send(json)
            .await
            .map_err(|_| McpSessionError::StdinClosed {
                server: self.config.name.clone(),
            })
    }

    /// Returns the server name from the configuration.
    pub fn server_name(&self) -> &str {
        &self.config.name
    }

    /// Returns the server capabilities received during the initialize handshake.
    pub fn capabilities(&self) -> &ServerCapabilities {
        &self.capabilities
    }

    /// Returns the server identity received during the initialize handshake.
    pub fn server_info(&self) -> &ServerInfo {
        &self.server_info
    }

    /// Returns the tools discovered via `tools/list`, or an empty slice before discovery.
    pub fn tools(&self) -> &[McpToolDefinition] {
        &self.tools
    }

    /// Returns whether every tool call to this server requires HITL approval.
    pub fn requires_approval(&self) -> bool {
        self.config.requires_approval
    }

    /// Returns the OS process ID of the server subprocess, if still running.
    pub fn pid(&self) -> Option<u32> {
        self.child.id()
    }

    /// Returns the number of seconds elapsed since this session was started.
    pub fn uptime_secs(&self) -> u64 {
        self.started_at.elapsed().as_secs()
    }

    /// Returns the configuration used to start this session.
    pub fn config(&self) -> &McpServerConfig {
        &self.config
    }

    /// Execute a tool on this MCP server via `tools/call`.
    ///
    /// Serialises `tool_name` and `arguments` into a `tools/call` JSON-RPC request,
    /// sends it through the stdin writer, and waits for the response. The timeout
    /// applied is `call_timeout_secs` from the server configuration.
    ///
    /// Returns the raw [`ToolCallResult`] so the caller can inspect `is_error` and
    /// route content accordingly. Deserialisaton failures are surfaced as
    /// [`McpSessionError::ToolCallFailed`].
    pub async fn call_tool(
        &self,
        tool_name: &str,
        arguments: Option<serde_json::Value>,
    ) -> Result<ToolCallResult, McpSessionError> {
        let params = ToolCallParams {
            name: tool_name.to_string(),
            arguments,
        };

        let params_value = serde_json::to_value(&params)
            .map_err(|e| McpSessionError::SerdeError(e.to_string()))?;

        let response = self
            .send_request(
                "tools/call",
                Some(params_value),
                self.config.call_timeout_secs,
            )
            .await
            .map_err(|e| match e {
                McpSessionError::InitializeTimeout { .. } => McpSessionError::ToolCallTimeout {
                    server: self.config.name.clone(),
                    tool: tool_name.to_string(),
                    timeout_secs: self.config.call_timeout_secs,
                },
                other => other,
            })?;

        serde_json::from_value(response).map_err(|e| McpSessionError::ToolCallFailed {
            server: self.config.name.clone(),
            tool: tool_name.to_string(),
            cause: e.to_string(),
        })
    }

    /// Gracefully shut down the session.
    ///
    /// Closes the stdin channel so the writer task terminates and the server process
    /// receives EOF. Waits up to 5 seconds for the process to exit on its own before
    /// returning — `kill_on_drop` will terminate it when this value is dropped if it
    /// has not yet exited.
    pub async fn shutdown(mut self) {
        drop(self.stdin_tx);
        let _ = tokio::time::timeout(std::time::Duration::from_secs(5), self.child.wait()).await;
    }
}

// ─── background tasks ────────────────────────────────────────────────────────

/// Spawn the stdin writer task.
///
/// Reads JSON-encoded messages from `rx` and writes each as a newline-terminated
/// line to the child process's stdin. Exits when the channel sender is dropped.
fn spawn_stdin_writer(
    stdin: tokio::process::ChildStdin,
    mut rx: mpsc::Receiver<String>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut writer = BufWriter::new(stdin);
        while let Some(line) = rx.recv().await {
            let msg = format!("{line}\n");
            if writer.write_all(msg.as_bytes()).await.is_err() || writer.flush().await.is_err() {
                break;
            }
        }
    })
}

/// Spawn the stdout reader task.
///
/// Reads newline-terminated JSON lines from the child process's stdout,
/// deserialises them as [`JsonRpcResponse`] values, and dispatches each response
/// to the waiting caller via the `pending` map. Exits when the stdout pipe is closed.
fn spawn_stdout_reader(
    stdout: tokio::process::ChildStdout,
    pending: Arc<Mutex<HashMap<u64, oneshot::Sender<JsonRpcResponse>>>>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let reader = BufReader::new(stdout);
        let mut lines = reader.lines();
        while let Ok(Some(line)) = lines.next_line().await {
            match serde_json::from_str::<JsonRpcResponse>(&line) {
                Ok(response) => {
                    if let Some(id) = response.id {
                        let mut map = pending.lock().await;
                        if let Some(sender) = map.remove(&id) {
                            // The receiver may have been dropped on timeout — that is expected.
                            let _ = sender.send(response);
                        }
                    }
                    // Notifications (no id) are intentionally ignored in V1.
                }
                Err(e) => {
                    warn!(error = %e, "failed to parse JSON-RPC line from MCP server stdout");
                }
            }
        }
    })
}

// ─── tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn make_config(
        name: &str,
        command: &str,
        args: Vec<String>,
        init_timeout_secs: u64,
        call_timeout_secs: u64,
    ) -> McpServerConfig {
        McpServerConfig {
            name: name.to_string(),
            command: command.to_string(),
            args,
            env: HashMap::new(),
            transport: "stdio".to_string(),
            requires_approval: false,
            init_timeout_secs,
            call_timeout_secs,
            tags: vec![],
        }
    }

    #[test]
    fn test_tools_list_result_parsing() {
        // GIVEN a tools/list response with three tools
        let json = serde_json::json!({
            "tools": [
                {"name": "search", "description": "Search pages", "inputSchema": {"type": "object"}},
                {"name": "create", "inputSchema": {"type": "object"}},
                {"name": "delete", "description": "Delete a page", "inputSchema": {"type": "object"}}
            ]
        });
        // WHEN
        let result: ToolsListResult = serde_json::from_value(json).unwrap();
        // THEN
        assert_eq!(result.tools.len(), 3);
        assert_eq!(result.tools[0].name, "search");
        assert_eq!(result.tools[1].description, None);
    }

    #[test]
    fn test_empty_tools_list_is_valid() {
        // GIVEN a tools/list response with no tools
        let json = serde_json::json!({"tools": []});
        // WHEN
        let result: ToolsListResult = serde_json::from_value(json).unwrap();
        // THEN
        assert!(result.tools.is_empty());
    }

    #[test]
    fn test_session_error_display() {
        // GIVEN
        let error = McpSessionError::SpawnFailed {
            server: "notion".to_string(),
            cause: "command not found".to_string(),
        };
        // WHEN / THEN
        assert!(error.to_string().contains("notion"));
        assert!(error.to_string().contains("command not found"));
    }

    #[test]
    fn test_initialize_timeout_error_display() {
        // GIVEN
        let error = McpSessionError::InitializeTimeout {
            server: "notion".to_string(),
            timeout_secs: 30,
        };
        // WHEN / THEN
        assert!(error.to_string().contains("30s"));
    }

    #[tokio::test]
    async fn test_spawn_with_invalid_command_fails() {
        // GIVEN a command that does not exist on this system
        let config = make_config("test", "nonexistent-binary-12345", vec![], 5, 10);
        // WHEN
        let result = McpSession::start(config).await;
        // THEN
        assert!(matches!(result, Err(McpSessionError::SpawnFailed { .. })));
    }

    #[tokio::test]
    async fn test_handshake_timeout_when_server_never_responds() {
        // GIVEN `cat` spawns successfully but never writes a JSON-RPC response
        let config = make_config("timeout-test", "cat", vec![], 1, 10);
        // WHEN
        let result = McpSession::start(config).await;
        // THEN the timeout fires and an error is returned
        assert!(result.is_err());
    }

    #[test]
    fn test_ac1_tool_call_params_serialization() {
        use crate::protocol::ToolCallParams;
        // GIVEN
        let params = ToolCallParams {
            name: "search".to_string(),
            arguments: Some(serde_json::json!({"query": "test"})),
        };
        // WHEN
        let value = serde_json::to_value(&params).unwrap();
        // THEN
        assert_eq!(value["name"], "search");
        assert_eq!(value["arguments"]["query"], "test");
    }

    #[test]
    fn test_ac3_jsonrpc_error_display() {
        // GIVEN
        let error = McpSessionError::JsonRpcError {
            server: "notion".to_string(),
            code: -32600,
            message: "Invalid Request".to_string(),
        };
        // WHEN / THEN
        let display = error.to_string();
        assert!(display.contains("-32600"));
        assert!(display.contains("Invalid Request"));
    }

    #[test]
    fn test_ac4_tool_call_result_with_is_error() {
        use crate::protocol::ToolCallResult;
        // GIVEN
        let json = serde_json::json!({
            "content": [{"type": "text", "text": "tool not found"}],
            "isError": true
        });
        // WHEN
        let result: ToolCallResult = serde_json::from_value(json).unwrap();
        // THEN
        assert_eq!(result.is_error, Some(true));
        assert_eq!(result.content.len(), 1);
    }

    #[test]
    fn test_tool_call_timeout_error_display() {
        // GIVEN
        let error = McpSessionError::ToolCallTimeout {
            server: "notion".to_string(),
            tool: "search".to_string(),
            timeout_secs: 60,
        };
        // WHEN / THEN
        let display = error.to_string();
        assert!(display.contains("60s"));
        assert!(display.contains("search"));
    }
}
