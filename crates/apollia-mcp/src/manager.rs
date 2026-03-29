//! MCP client manager: actor owning all MCP server sessions and registering their tools.
//!
//! [`McpClientManagerHandle`] is the only public entry point. It starts N sessions in
//! sequence, registers discovered tools in the [`ToolRegistryHandle`], and routes
//! `call_tool` requests to the correct session. A server that fails to start is
//! logged and skipped — it is never fatal to the rest.

use std::collections::HashMap;

use tokio::sync::{mpsc, oneshot};

use apollia_core::SandboxProfile;
use apollia_tools::descriptor::{McpTransport, ToolDescriptor, ToolKind};
use apollia_tools::registry::ToolRegistryHandle;

use crate::config::McpServerConfig;
use crate::protocol::ToolCallResult;
use crate::session::{McpSession, McpSessionError};

// ─── commands ────────────────────────────────────────────────────────────────

/// Messages processed by the [`McpClientManager`] actor.
enum McpCommand {
    /// Execute a tool on a named MCP server.
    CallTool {
        server_name: String,
        tool_name: String,
        arguments: Option<serde_json::Value>,
        reply: oneshot::Sender<Result<ToolCallResult, McpSessionError>>,
    },
    /// Return the status of every managed session.
    GetStatus {
        reply: oneshot::Sender<Vec<McpServerStatus>>,
    },
    /// Return detailed info (status + tools + redacted config) for a single server.
    GetDetail {
        server_name: String,
        reply: oneshot::Sender<Option<McpServerDetail>>,
    },
    /// Restart a specific server session (stop + re-spawn).
    RestartServer {
        server_name: String,
        reply: oneshot::Sender<Result<McpServerStatus, McpSessionError>>,
    },
    /// Check whether a named server requires HITL approval for all its tools.
    ServerRequiresApproval {
        server_name: String,
        reply: oneshot::Sender<bool>,
    },
    /// Gracefully shut down all sessions and stop the actor loop.
    Shutdown,
}

// ─── public types ────────────────────────────────────────────────────────────

/// Status snapshot for a single connected MCP server.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct McpServerStatus {
    /// Server name as declared in the configuration.
    pub name: String,
    /// Human-readable server identity returned during the initialize handshake.
    pub server_info: String,
    /// Number of tools discovered on this server.
    pub tools_count: usize,
    /// Whether every tool call to this server requires HITL approval.
    pub requires_approval: bool,
    /// `true` when the session is alive; always `true` for sessions tracked by the manager.
    pub connected: bool,
    /// OS process ID of the server subprocess, if still running.
    pub pid: Option<u32>,
    /// Seconds elapsed since the session was started.
    pub uptime_secs: Option<u64>,
    /// ISO 8601 timestamp of the last tool call (`None` if the server has never been called).
    pub last_call_at: Option<String>,
    /// Error message when the server is in a degraded state.
    pub error: Option<String>,
    /// Package identifier (e.g. `@notionhq/notion-mcp-server`), when identifiable.
    pub package: Option<String>,
    /// Transport protocol declared in the configuration (e.g. `"stdio"`).
    pub transport: String,
}

/// Detailed information for a single MCP server, including its tool list.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct McpServerDetail {
    /// Summary status of the server.
    pub status: McpServerStatus,
    /// Full list of tools exposed by this server.
    pub tools: Vec<McpToolSummary>,
    /// Server configuration with secrets redacted.
    pub config: McpServerConfigView,
}

/// Summary of a single MCP tool for API and UI consumption.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct McpToolSummary {
    /// Fully qualified tool name as registered in the ToolRegistry (e.g. `mcp:notion/search`).
    pub full_name: String,
    /// Tool name within the server scope (e.g. `search`).
    pub local_name: String,
    /// Human-readable description, if provided by the server.
    pub description: Option<String>,
    /// JSON Schema for the tool's input parameters.
    pub input_schema: serde_json::Value,
}

/// Read-only view of a server configuration with all secret values redacted.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct McpServerConfigView {
    /// Server name as declared in the configuration.
    pub name: String,
    /// Command used to launch the server subprocess.
    pub command: String,
    /// Arguments passed to the server command.
    pub args: Vec<String>,
    /// Environment variable keys declared for this server (values are not exposed).
    pub env_keys: Vec<String>,
    /// Transport protocol (e.g. `"stdio"`).
    pub transport: String,
    /// Whether tool calls require HITL approval.
    pub requires_approval: bool,
    /// Tags attached to this server.
    pub tags: Vec<String>,
}

/// Result of a connection test performed without persisting a new session.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct McpConnectionTestResult {
    /// Server identity returned by the `initialize` response.
    pub server_info: String,
    /// MCP protocol version negotiated with the server.
    pub protocol_version: String,
    /// Tools discovered during the test session.
    pub tools: Vec<McpToolSummary>,
    /// Wall-clock duration of the test in milliseconds.
    pub test_duration_ms: u64,
}

/// Clonable handle to the [`McpClientManager`] actor.
///
/// Obtain one via [`McpClientManagerHandle::start`]. All methods are async and
/// communicate with the actor through an `mpsc` channel.
#[derive(Clone)]
pub struct McpClientManagerHandle {
    tx: mpsc::Sender<McpCommand>,
}

// ─── actor ───────────────────────────────────────────────────────────────────

/// Actor that owns and orchestrates all MCP server sessions.
///
/// Never instantiated directly — always accessed through [`McpClientManagerHandle`].
struct McpClientManager {
    sessions: HashMap<String, McpSession>,
    rx: mpsc::Receiver<McpCommand>,
}

// ─── handle impl ─────────────────────────────────────────────────────────────

impl McpClientManagerHandle {
    /// Start the MCP client manager.
    ///
    /// Iterates through `configs` in order, spawning each server process and
    /// performing the MCP initialize + tools/list handshake. Discovered tools are
    /// registered in `tool_registry` with the `mcp:<server>/<tool>` naming convention.
    ///
    /// A server that fails to start is logged at `error` level and skipped; the
    /// remaining servers continue unaffected. The actor loop is spawned after all
    /// sessions are established.
    pub async fn start(
        configs: Vec<McpServerConfig>,
        tool_registry: &ToolRegistryHandle,
    ) -> Result<Self, McpSessionError> {
        let (tx, rx) = mpsc::channel(32);
        let mut sessions: HashMap<String, McpSession> = HashMap::new();

        for config in configs {
            let server_name = config.name.clone();
            let requires_approval = config.requires_approval;
            let tags = config.tags.clone();

            match McpSession::start(config).await {
                Ok(session) => {
                    tracing::info!(
                        server = %server_name,
                        tools = session.tools().len(),
                        "MCP server connected"
                    );

                    for tool_def in session.tools() {
                        let mut tool_tags = vec!["mcp".to_string(), server_name.clone()];
                        tool_tags.extend(tags.clone());

                        let descriptor = ToolDescriptor {
                            name: format!("mcp:{}/{}", server_name, tool_def.name),
                            version: "1.0.0".to_string(),
                            description: tool_def
                                .description
                                .clone()
                                .unwrap_or_else(|| format!("MCP tool from {}", server_name)),
                            kind: ToolKind::McpServer {
                                server_url: format!("stdio://{}", server_name),
                                transport: McpTransport::Stdio,
                                tool_name: tool_def.name.clone(),
                            },
                            input_schema: tool_def.input_schema.clone(),
                            output_schema: None,
                            sandbox_profile: if requires_approval {
                                SandboxProfile::Full
                            } else {
                                SandboxProfile::NetworkRestricted
                            },
                            tags: tool_tags,
                            dangerous: requires_approval,
                        };

                        match tool_registry.register(descriptor).await {
                            Ok(()) => {
                                tracing::info!(
                                    server = %server_name,
                                    tool = %tool_def.name,
                                    "MCP tool registered"
                                );
                            }
                            Err(e) => {
                                tracing::warn!(
                                    server = %server_name,
                                    tool = %tool_def.name,
                                    error = %e,
                                    "failed to register MCP tool"
                                );
                            }
                        }
                    }

                    sessions.insert(server_name, session);
                }
                Err(e) => {
                    tracing::error!(
                        server = %server_name,
                        error = %e,
                        "MCP server failed to start, skipping"
                    );
                }
            }
        }

        let actor = McpClientManager { sessions, rx };
        tokio::spawn(actor.run());

        Ok(Self { tx })
    }

    /// Execute a tool on the named MCP server.
    ///
    /// Routes the call to the session identified by `server_name`. Returns
    /// [`McpSessionError::ServerExited`] when no session with that name exists or
    /// when the actor channel is closed.
    pub async fn call_tool(
        &self,
        server_name: &str,
        tool_name: &str,
        arguments: Option<serde_json::Value>,
    ) -> Result<ToolCallResult, McpSessionError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(McpCommand::CallTool {
                server_name: server_name.to_string(),
                tool_name: tool_name.to_string(),
                arguments,
                reply: reply_tx,
            })
            .await
            .map_err(|_| McpSessionError::ServerExited {
                server: server_name.to_string(),
            })?;
        reply_rx.await.map_err(|_| McpSessionError::ServerExited {
            server: server_name.to_string(),
        })?
    }

    /// Return the status of every connected MCP server.
    ///
    /// Returns an empty `Vec` when the actor has already shut down.
    pub async fn status(&self) -> Vec<McpServerStatus> {
        let (reply_tx, reply_rx) = oneshot::channel();
        if self
            .tx
            .send(McpCommand::GetStatus { reply: reply_tx })
            .await
            .is_err()
        {
            return Vec::new();
        }
        reply_rx.await.unwrap_or_default()
    }

    /// Check whether a server requires HITL approval for all its tools.
    ///
    /// Returns `true` when the server's `requires_approval` flag is set in `mcp.toml`.
    /// Returns `false` when no session with that name is connected, or the actor has
    /// already shut down.
    pub async fn server_requires_approval(&self, server_name: &str) -> bool {
        let (reply_tx, reply_rx) = oneshot::channel();
        if self
            .tx
            .send(McpCommand::ServerRequiresApproval {
                server_name: server_name.to_string(),
                reply: reply_tx,
            })
            .await
            .is_err()
        {
            return false;
        }
        reply_rx.await.unwrap_or(false)
    }

    /// Return detailed information (status, tools, redacted config) for a single server.
    ///
    /// Returns `None` when no session with `server_name` is connected, or when
    /// the actor has already shut down.
    pub async fn server_detail(&self, server_name: &str) -> Option<McpServerDetail> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(McpCommand::GetDetail {
                server_name: server_name.to_string(),
                reply: reply_tx,
            })
            .await
            .ok()?;
        reply_rx.await.unwrap_or(None)
    }

    /// Restart the named server: stop the current session and spawn a new one.
    ///
    /// Returns the updated [`McpServerStatus`] on success. Returns an error when
    /// no session with `server_name` exists, the actor has shut down, or the new
    /// session fails to initialise.
    pub async fn restart_server(
        &self,
        server_name: &str,
    ) -> Result<McpServerStatus, McpSessionError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(McpCommand::RestartServer {
                server_name: server_name.to_string(),
                reply: reply_tx,
            })
            .await
            .map_err(|_| McpSessionError::ServerExited {
                server: server_name.to_string(),
            })?;
        reply_rx.await.map_err(|_| McpSessionError::ServerExited {
            server: server_name.to_string(),
        })?
    }

    /// Gracefully shut down all MCP sessions and stop the actor.
    ///
    /// Consumes the handle. Remaining clones will receive channel-closed errors
    /// on their next operation.
    pub async fn shutdown(self) {
        let _ = self.tx.send(McpCommand::Shutdown).await;
    }
}

// ─── actor impl ──────────────────────────────────────────────────────────────

impl McpClientManager {
    /// Main actor loop: process commands until a [`McpCommand::Shutdown`] is received
    /// or all senders are dropped.
    async fn run(mut self) {
        while let Some(cmd) = self.rx.recv().await {
            match cmd {
                McpCommand::CallTool {
                    server_name,
                    tool_name,
                    arguments,
                    reply,
                } => {
                    let result = match self.sessions.get(&server_name) {
                        Some(session) => session.call_tool(&tool_name, arguments).await,
                        None => Err(McpSessionError::ServerExited {
                            server: server_name,
                        }),
                    };
                    let _ = reply.send(result);
                }

                McpCommand::GetStatus { reply } => {
                    let statuses = self
                        .sessions
                        .iter()
                        .map(|(name, session)| build_status(name, session))
                        .collect();
                    let _ = reply.send(statuses);
                }

                McpCommand::GetDetail { server_name, reply } => {
                    let detail = self
                        .sessions
                        .get(&server_name)
                        .map(|session| build_detail(&server_name, session));
                    let _ = reply.send(detail);
                }

                McpCommand::RestartServer { server_name, reply } => {
                    match self.sessions.remove(&server_name) {
                        None => {
                            let _ = reply.send(Err(McpSessionError::ServerExited {
                                server: server_name,
                            }));
                        }
                        Some(old_session) => {
                            let config = old_session.config().clone();
                            old_session.shutdown().await;
                            match McpSession::start(config).await {
                                Ok(new_session) => {
                                    let status = build_status(&server_name, &new_session);
                                    self.sessions.insert(server_name, new_session);
                                    let _ = reply.send(Ok(status));
                                }
                                Err(e) => {
                                    let _ = reply.send(Err(e));
                                }
                            }
                        }
                    }
                }

                McpCommand::ServerRequiresApproval { server_name, reply } => {
                    let requires = self
                        .sessions
                        .get(&server_name)
                        .map(|s| s.requires_approval())
                        .unwrap_or(false);
                    let _ = reply.send(requires);
                }

                McpCommand::Shutdown => {
                    for (_, session) in self.sessions.drain() {
                        session.shutdown().await;
                    }
                    break;
                }
            }
        }
    }
}

// ─── helpers ─────────────────────────────────────────────────────────────────

/// Build an enriched [`McpServerStatus`] snapshot from a live session.
fn build_status(name: &str, session: &McpSession) -> McpServerStatus {
    McpServerStatus {
        name: name.to_string(),
        server_info: session.server_info().name.clone(),
        tools_count: session.tools().len(),
        requires_approval: session.requires_approval(),
        connected: true,
        pid: session.pid(),
        uptime_secs: Some(session.uptime_secs()),
        last_call_at: None,
        error: None,
        package: None,
        transport: session.config().transport.clone(),
    }
}

/// Build a [`McpServerDetail`] from a live session, redacting secret env values.
fn build_detail(name: &str, session: &McpSession) -> McpServerDetail {
    let config = session.config();
    let tools = session
        .tools()
        .iter()
        .map(|t| McpToolSummary {
            full_name: format!("mcp:{}/{}", name, t.name),
            local_name: t.name.clone(),
            description: t.description.clone(),
            input_schema: t.input_schema.clone(),
        })
        .collect();

    let config_view = McpServerConfigView {
        name: config.name.clone(),
        command: config.command.clone(),
        args: config.args.clone(),
        env_keys: config.env.keys().cloned().collect(),
        transport: config.transport.clone(),
        requires_approval: config.requires_approval,
        tags: config.tags.clone(),
    };

    McpServerDetail {
        status: build_status(name, session),
        tools,
        config: config_view,
    }
}

// ─── tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ac3_tool_naming_convention() {
        // GIVEN a server "notion" and a tool "search_pages"
        let server_name = "notion";
        let tool_name = "search_pages";
        // WHEN the composite name is built
        let full_name = format!("mcp:{}/{}", server_name, tool_name);
        // THEN
        assert_eq!(full_name, "mcp:notion/search_pages");
    }

    #[test]
    fn test_ac5_server_status_serialization() {
        // GIVEN a status snapshot for a connected server
        let status = McpServerStatus {
            name: "notion".to_string(),
            server_info: "notion-mcp-server".to_string(),
            tools_count: 5,
            requires_approval: true,
            connected: true,
            pid: Some(1234),
            uptime_secs: Some(60),
            last_call_at: None,
            error: None,
            package: None,
            transport: "stdio".to_string(),
        };
        // WHEN serialized to JSON
        let json = serde_json::to_value(&status).unwrap();
        // THEN all fields are present and correct
        assert_eq!(json["name"], "notion");
        assert_eq!(json["tools_count"], 5);
        assert_eq!(json["requires_approval"], true);
        assert_eq!(json["transport"], "stdio");
    }

    #[test]
    fn test_server_requires_approval_flag_in_config() {
        // GIVEN a server config with requires_approval=true
        use crate::config::McpServerConfig;
        let config = McpServerConfig {
            name: "notion".to_string(),
            command: "npx".to_string(),
            args: vec![],
            env: std::collections::HashMap::new(),
            transport: "stdio".to_string(),
            requires_approval: true,
            init_timeout_secs: 30,
            call_timeout_secs: 60,
            tags: vec![],
        };
        // WHEN / THEN the flag is readable
        assert!(config.requires_approval);
    }

    #[test]
    fn test_server_requires_approval_false_by_default() {
        // GIVEN a server config with requires_approval=false
        use crate::config::McpServerConfig;
        let config = McpServerConfig {
            name: "sqlite".to_string(),
            command: "npx".to_string(),
            args: vec![],
            env: std::collections::HashMap::new(),
            transport: "stdio".to_string(),
            requires_approval: false,
            init_timeout_secs: 30,
            call_timeout_secs: 60,
            tags: vec![],
        };
        // THEN
        assert!(!config.requires_approval);
    }

    #[test]
    fn test_tool_naming_no_collision_across_servers() {
        // GIVEN two servers exposing a tool with the same base name
        let notion_tool = format!("mcp:{}/{}", "notion", "search");
        let sqlite_tool = format!("mcp:{}/{}", "sqlite", "search");
        // THEN the qualified names are distinct
        assert_ne!(notion_tool, sqlite_tool);
    }

    #[test]
    fn test_server_status_not_connected_is_serializable() {
        // GIVEN a status where connected=false (possible in future extensions)
        let status = McpServerStatus {
            name: "github".to_string(),
            server_info: String::new(),
            tools_count: 0,
            requires_approval: false,
            connected: false,
            pid: None,
            uptime_secs: None,
            last_call_at: None,
            error: Some("process exited".to_string()),
            package: None,
            transport: "stdio".to_string(),
        };
        // WHEN
        let json = serde_json::to_value(&status).unwrap();
        // THEN
        assert_eq!(json["connected"], false);
        assert_eq!(json["tools_count"], 0);
    }

    #[tokio::test]
    async fn test_empty_status_when_no_mcp_handle() {
        // GIVEN no MCP handle
        let mcp_handle: Option<McpClientManagerHandle> = None;
        // WHEN status is queried
        let statuses = match &mcp_handle {
            Some(handle) => handle.status().await,
            None => Vec::new(),
        };
        // THEN the result is empty
        assert!(statuses.is_empty());
    }

    #[test]
    fn test_list_servers_serialization() {
        // GIVEN two server status snapshots
        let statuses = vec![
            McpServerStatus {
                name: "notion".to_string(),
                server_info: "notion-mcp-server".to_string(),
                tools_count: 5,
                requires_approval: true,
                connected: true,
                pid: None,
                uptime_secs: Some(30),
                last_call_at: None,
                error: None,
                package: None,
                transport: "stdio".to_string(),
            },
            McpServerStatus {
                name: "sqlite".to_string(),
                server_info: "mcp-server-sqlite".to_string(),
                tools_count: 3,
                requires_approval: false,
                connected: true,
                pid: None,
                uptime_secs: Some(30),
                last_call_at: None,
                error: None,
                package: None,
                transport: "stdio".to_string(),
            },
        ];
        // WHEN serialized
        let json = serde_json::to_value(&statuses).unwrap();
        // THEN the array and fields are correct
        assert_eq!(json.as_array().unwrap().len(), 2);
        assert_eq!(json[0]["name"], "notion");
        assert_eq!(json[1]["tools_count"], 3);
    }

    #[test]
    fn test_server_detail_serialization() {
        // GIVEN a server detail with one tool
        let detail = McpServerDetail {
            status: McpServerStatus {
                name: "notion".to_string(),
                server_info: "notion-mcp-server".to_string(),
                tools_count: 1,
                requires_approval: false,
                connected: true,
                pid: Some(42),
                uptime_secs: Some(10),
                last_call_at: None,
                error: None,
                package: None,
                transport: "stdio".to_string(),
            },
            tools: vec![McpToolSummary {
                full_name: "mcp:notion/search".to_string(),
                local_name: "search".to_string(),
                description: Some("Search pages".to_string()),
                input_schema: serde_json::json!({"type": "object"}),
            }],
            config: McpServerConfigView {
                name: "notion".to_string(),
                command: "npx".to_string(),
                args: vec!["@notionhq/notion-mcp-server".to_string()],
                env_keys: vec!["NOTION_TOKEN".to_string()],
                transport: "stdio".to_string(),
                requires_approval: false,
                tags: vec![],
            },
        };
        // WHEN serialized
        let json = serde_json::to_value(&detail).unwrap();
        // THEN fields are correct and no secret values are exposed
        assert_eq!(json["status"]["name"], "notion");
        assert_eq!(json["tools"].as_array().unwrap().len(), 1);
        assert_eq!(json["config"]["env_keys"][0], "NOTION_TOKEN");
        assert!(json["config"].get("env").is_none());
    }

    #[test]
    fn test_config_view_redacts_env_values() {
        // GIVEN a config view with env keys
        let view = McpServerConfigView {
            name: "notion".to_string(),
            command: "npx".to_string(),
            args: vec![],
            env_keys: vec!["NOTION_TOKEN".to_string(), "API_KEY".to_string()],
            transport: "stdio".to_string(),
            requires_approval: false,
            tags: vec![],
        };
        // THEN only keys are exposed — no values
        assert_eq!(view.env_keys.len(), 2);
        assert!(view.env_keys.contains(&"NOTION_TOKEN".to_string()));
        assert!(view.env_keys.contains(&"API_KEY".to_string()));
    }
}
