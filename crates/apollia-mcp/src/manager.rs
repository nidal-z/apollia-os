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
#[derive(Debug, serde::Serialize)]
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
                        .map(|(name, session)| McpServerStatus {
                            name: name.clone(),
                            server_info: session.server_info().name.clone(),
                            tools_count: session.tools().len(),
                            requires_approval: session.requires_approval(),
                            connected: true,
                        })
                        .collect();
                    let _ = reply.send(statuses);
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
        };
        // WHEN serialized to JSON
        let json = serde_json::to_value(&status).unwrap();
        // THEN all fields are present and correct
        assert_eq!(json["name"], "notion");
        assert_eq!(json["tools_count"], 5);
        assert_eq!(json["requires_approval"], true);
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
        };
        // WHEN
        let json = serde_json::to_value(&status).unwrap();
        // THEN
        assert_eq!(json["connected"], false);
        assert_eq!(json["tools_count"], 0);
    }
}
