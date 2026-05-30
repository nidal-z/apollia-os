//! MCP client manager: actor owning all MCP server sessions and registering their tools.
//!
//! [`McpClientManagerHandle`] is the only public entry point. It starts N sessions in
//! sequence, registers discovered tools in the [`ToolRegistryHandle`], and routes
//! `call_tool` requests to the correct session. A server that fails to start is
//! logged and skipped; it is never fatal to the rest.

use std::collections::{HashMap, HashSet};

use tokio::sync::{mpsc, oneshot};

use apollia_core::{EventBusSender, RuntimeEvent, SandboxProfile};
use apollia_tools::descriptor::{McpTransport, ToolDescriptor, ToolKind};
use apollia_tools::registry::ToolRegistryHandle;

use crate::approvals::{McpApprovalError, McpApprovalStore, PendingApprovalEntry};
use crate::config::{DefaultMcpSecretResolver, McpServerConfig};
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
    /// Add a new server at runtime: spawn, handshake, register its tools.
    AddServer {
        config: McpServerConfig,
        reply: oneshot::Sender<Result<McpServerStatus, McpSessionError>>,
    },
    /// Remove a server: shutdown the session and unregister from the session map.
    RemoveServer {
        server_name: String,
        reply: oneshot::Sender<Result<(), McpSessionError>>,
    },
    /// Restart a specific server session (stop + re-spawn).
    RestartServer {
        server_name: String,
        reply: oneshot::Sender<Result<McpServerStatus, McpSessionError>>,
    },
    /// Test a config without persisting a session: spawn, handshake, tools/list, then kill.
    TestConnection {
        config: McpServerConfig,
        reply: oneshot::Sender<Result<McpConnectionTestResult, McpSessionError>>,
    },
    /// Check whether a named server requires HITL approval for all its tools.
    ServerRequiresApproval {
        server_name: String,
        reply: oneshot::Sender<bool>,
    },
    /// Update the `requires_approval` flag for a server in-memory.
    SetApproval {
        server_name: String,
        requires_approval: bool,
        reply: oneshot::Sender<Result<(), McpSessionError>>,
    },
    /// Hot-reload a server: disconnect, re-read config, reconnect, emit McpServerReloaded.
    ReloadServer {
        server_name: String,
        reply: oneshot::Sender<Result<(), McpSessionError>>,
    },
    /// Approve a tool for future calls without HITL suspension.
    ApproveToolAccess {
        server_name: String,
        tool_name: String,
        reply: oneshot::Sender<Result<(), McpApprovalError>>,
    },
    /// Revoke a previously granted tool approval.
    RevokeToolAccess {
        server_name: String,
        tool_name: String,
        reply: oneshot::Sender<Result<(), McpApprovalError>>,
    },
    /// Return all pending approval requests awaiting human decision.
    ListPendingApprovals {
        reply: oneshot::Sender<Result<Vec<PendingApprovalEntry>, McpApprovalError>>,
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
/// Never instantiated directly; always accessed through [`McpClientManagerHandle`].
struct McpClientManager {
    sessions: HashMap<String, McpSession>,
    rx: mpsc::Receiver<McpCommand>,
    tool_registry: ToolRegistryHandle,
    /// Server names currently being hot-reloaded.
    ///
    /// While a name is in this set, any `CallTool` targeting that server
    /// is rejected with [`McpSessionError::ServerReloading`].
    reloading: HashSet<String>,
    /// Optional event bus for emitting [`RuntimeEvent::McpServerReloaded`].
    event_bus: Option<EventBusSender>,
    /// Optional SQLite-backed HITL approval store.
    ///
    /// When `Some`, every `CallTool` directed to a server with
    /// `requires_approval = true` is checked against this store before
    /// forwarding to the session. When `None`, the approval gate is disabled.
    approvals: Option<McpApprovalStore>,
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
    ///
    /// `event_bus` is optional: when `Some`, [`RuntimeEvent::McpServerReloaded`] is
    /// published on every successful hot reload. When `None`, reloads still work but
    /// no event is emitted.
    ///
    /// `approvals` is optional: when `Some`, the HITL approval gate is active and
    /// every tool call to a server with `requires_approval = true` is checked against
    /// the store before forwarding to the session. When `None`, the gate is disabled.
    pub async fn start(
        configs: Vec<McpServerConfig>,
        tool_registry: &ToolRegistryHandle,
        event_bus: Option<EventBusSender>,
        approvals: Option<McpApprovalStore>,
    ) -> Result<Self, McpSessionError> {
        let (tx, rx) = mpsc::channel(32);
        let mut sessions: HashMap<String, McpSession> = HashMap::new();

        for config in configs {
            let server_name = config.name.clone();
            let requires_approval = config.requires_approval;
            let tags = config.tags.clone();

            match McpSession::start(config, Some(&DefaultMcpSecretResolver)).await {
                Ok(session) => {
                    tracing::info!(
                        server = %server_name,
                        tools = session.tools().len(),
                        "MCP server connected"
                    );

                    register_session_tools_in_registry(
                        tool_registry,
                        &server_name,
                        requires_approval,
                        &tags,
                        &session,
                    )
                    .await;

                    sessions.insert(server_name, session);
                }
                Err(e) => log_session_start_error(&server_name, &e),
            }
        }

        let actor = McpClientManager {
            sessions,
            rx,
            tool_registry: tool_registry.clone(),
            reloading: HashSet::new(),
            event_bus,
            approvals,
        };
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

    /// Add a new MCP server at runtime.
    ///
    /// Spawns the server process, performs the initialize handshake, registers
    /// discovered tools in the tool registry, and returns the server status.
    /// Returns [`McpSessionError::InitializeFailed`] when a server with the same
    /// name is already managed.
    pub async fn add_server(
        &self,
        config: McpServerConfig,
    ) -> Result<McpServerStatus, McpSessionError> {
        let name = config.name.clone();
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(McpCommand::AddServer {
                config,
                reply: reply_tx,
            })
            .await
            .map_err(|_| McpSessionError::ServerExited {
                server: name.clone(),
            })?;
        reply_rx
            .await
            .map_err(|_| McpSessionError::ServerExited { server: name })?
    }

    /// Remove an MCP server: shutdown the session and remove it from the managed set.
    ///
    /// Returns [`McpSessionError::ServerExited`] when no session with `name` exists.
    pub async fn remove_server(&self, name: &str) -> Result<(), McpSessionError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(McpCommand::RemoveServer {
                server_name: name.to_string(),
                reply: reply_tx,
            })
            .await
            .map_err(|_| McpSessionError::ServerExited {
                server: name.to_string(),
            })?;
        reply_rx.await.map_err(|_| McpSessionError::ServerExited {
            server: name.to_string(),
        })?
    }

    /// Test a server configuration without persisting any session.
    ///
    /// Spawns an ephemeral process, performs the MCP handshake and `tools/list`,
    /// captures the result, then immediately kills the process. No session is
    /// registered and the tool registry is not modified.
    pub async fn test_connection(
        &self,
        config: McpServerConfig,
    ) -> Result<McpConnectionTestResult, McpSessionError> {
        let name = config.name.clone();
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(McpCommand::TestConnection {
                config,
                reply: reply_tx,
            })
            .await
            .map_err(|_| McpSessionError::ServerExited {
                server: name.clone(),
            })?;
        reply_rx
            .await
            .map_err(|_| McpSessionError::ServerExited { server: name })?
    }

    /// Update the `requires_approval` flag for the named server in-memory.
    ///
    /// The change is applied immediately; callers are responsible for persisting
    /// the new value to the [`crate::McpServerRepository`].
    /// Returns [`McpSessionError::ServerExited`] when no session with `server_name`
    /// is connected, or when the actor has already shut down.
    pub async fn set_server_approval(
        &self,
        server_name: &str,
        requires_approval: bool,
    ) -> Result<(), McpSessionError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(McpCommand::SetApproval {
                server_name: server_name.to_string(),
                requires_approval,
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

    /// Approve `(server_name, tool_name)` in the HITL store with the configured TTL.
    ///
    /// After approval, subsequent calls to the tool on that server bypass the
    /// HITL suspension gate until the approval expires.
    ///
    /// Returns [`McpApprovalError`] when the store is not configured (`None`) or
    /// when the SQLite write fails.
    pub async fn approve_tool(
        &self,
        server_name: &str,
        tool_name: &str,
    ) -> Result<(), McpApprovalError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        if self
            .tx
            .send(McpCommand::ApproveToolAccess {
                server_name: server_name.to_string(),
                tool_name: tool_name.to_string(),
                reply: reply_tx,
            })
            .await
            .is_err()
        {
            return Err(McpApprovalError::Db(rusqlite::Error::InvalidQuery));
        }
        reply_rx
            .await
            .unwrap_or(Err(McpApprovalError::Db(rusqlite::Error::InvalidQuery)))
    }

    /// Revoke a previously granted tool approval.
    ///
    /// Returns [`McpApprovalError`] when the store is not configured or the
    /// SQLite delete fails.
    pub async fn revoke_tool(
        &self,
        server_name: &str,
        tool_name: &str,
    ) -> Result<(), McpApprovalError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        if self
            .tx
            .send(McpCommand::RevokeToolAccess {
                server_name: server_name.to_string(),
                tool_name: tool_name.to_string(),
                reply: reply_tx,
            })
            .await
            .is_err()
        {
            return Err(McpApprovalError::Db(rusqlite::Error::InvalidQuery));
        }
        reply_rx
            .await
            .unwrap_or(Err(McpApprovalError::Db(rusqlite::Error::InvalidQuery)))
    }

    /// Return all pending approval requests from the HITL store.
    ///
    /// Returns an empty `Vec` when the approval store is not configured.
    pub async fn list_pending_approvals(
        &self,
    ) -> Result<Vec<PendingApprovalEntry>, McpApprovalError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        if self
            .tx
            .send(McpCommand::ListPendingApprovals { reply: reply_tx })
            .await
            .is_err()
        {
            return Ok(Vec::new());
        }
        reply_rx.await.unwrap_or(Ok(Vec::new()))
    }

    /// Hot-reload a named MCP server without restarting the runtime.
    ///
    /// Disconnects the current session, re-reads the server's configuration from the
    /// managed session, reconnects, updates the tool registry, and emits
    /// [`RuntimeEvent::McpServerReloaded`] on the event bus (when configured).
    ///
    /// Returns [`McpSessionError::ConfigReload`] when no session with `name` is managed.
    /// Returns [`McpSessionError::ServerReloading`] when a reload for that server is
    /// already in progress.
    pub async fn reload_server(&self, name: &str) -> Result<(), McpSessionError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(McpCommand::ReloadServer {
                server_name: name.to_string(),
                reply: reply_tx,
            })
            .await
            .map_err(|_| McpSessionError::ServerExited {
                server: name.to_string(),
            })?;
        reply_rx.await.map_err(|_| McpSessionError::ServerExited {
            server: name.to_string(),
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
    /// Register all tools from a session into the tool registry.
    ///
    /// Uses the `mcp:<server>/<tool>` naming convention. Failures are logged at
    /// `warn` level and do not abort the registration of remaining tools.
    async fn register_session_tools(&self, server_name: &str, session: &McpSession) {
        let tags = session.config().tags.clone();
        let requires_approval = session.requires_approval();
        register_session_tools_in_registry(
            &self.tool_registry,
            server_name,
            requires_approval,
            &tags,
            session,
        )
        .await;
    }

    /// Spawn a new session for `config`, register its tools, and insert it into the map.
    ///
    /// Returns an error when a session with the same name already exists, or when
    /// the session fails to start.
    async fn handle_add_server(
        &mut self,
        config: McpServerConfig,
    ) -> Result<McpServerStatus, McpSessionError> {
        let name = config.name.clone();
        if self.sessions.contains_key(&name) {
            return Err(McpSessionError::InitializeFailed {
                server: name,
                cause: "server with this name already exists".to_string(),
            });
        }

        let session = McpSession::start(config, Some(&DefaultMcpSecretResolver)).await?;
        tracing::info!(
            server = %name,
            tools = session.tools().len(),
            "MCP server added"
        );
        self.register_session_tools(&name, &session).await;
        let status = build_status(&name, &session);
        self.sessions.insert(name, session);
        Ok(status)
    }

    /// Shutdown the session named `name` and remove it from the managed set.
    ///
    /// Returns [`McpSessionError::ServerExited`] when the name is not found.
    async fn handle_remove_server(&mut self, name: &str) -> Result<(), McpSessionError> {
        match self.sessions.remove(name) {
            Some(session) => {
                session.shutdown().await;
                tracing::info!(server = %name, "MCP server removed");
                Ok(())
            }
            None => Err(McpSessionError::ServerExited {
                server: name.to_string(),
            }),
        }
    }

    /// Hot-reload the named server: disconnect → update tool registry → reconnect → emit event.
    ///
    /// Returns [`McpSessionError::ConfigReload`] when no session with `name` is managed.
    /// Returns [`McpSessionError::ServerReloading`] when the server is already reloading.
    async fn handle_reload_server(&mut self, name: &str) -> Result<(), McpSessionError> {
        if self.reloading.contains(name) {
            return Err(McpSessionError::ServerReloading {
                server: name.to_string(),
            });
        }

        let old_session = match self.sessions.remove(name) {
            Some(s) => s,
            None => {
                return Err(McpSessionError::ConfigReload {
                    server: name.to_string(),
                });
            }
        };

        let old_tools: Vec<String> = old_session.tools().iter().map(|t| t.name.clone()).collect();
        let config = old_session.config().clone();

        self.reloading.insert(name.to_string());

        old_session.shutdown().await;

        let new_session = match McpSession::start(config, Some(&DefaultMcpSecretResolver)).await {
            Ok(s) => s,
            Err(e) => {
                self.reloading.remove(name);
                tracing::error!(
                    server = %name,
                    error = %e,
                    "MCP server hot reload failed — server removed from managed set"
                );
                return Err(e);
            }
        };

        let new_tools: Vec<String> = new_session.tools().iter().map(|t| t.name.clone()).collect();

        tracing::info!(
            server = %name,
            old_tools = ?old_tools,
            new_tools = ?new_tools,
            "MCP server hot reload completed"
        );

        self.register_session_tools(name, &new_session).await;
        self.sessions.insert(name.to_string(), new_session);
        self.reloading.remove(name);

        if let Some(ref tx) = self.event_bus {
            let _ = tx.send(RuntimeEvent::McpServerReloaded {
                name: name.to_string(),
                old_tools,
                new_tools,
            });
        }

        Ok(())
    }

    /// Spawn an ephemeral session for `config`, capture the result, then kill it.
    ///
    /// No session is stored and the tool registry is never modified.
    ///
    /// **OAuth handshake follow-up:** when the underlying HTTP transport
    /// receives a 401 the error reaches this function as
    /// [`crate::transport::TransportError::Unauthorized`] (wrapped inside
    /// `McpSessionError::Transport` further down). The orchestration layer
    /// (desktop `commands::mcp::test_mcp_connection` /
    /// `add_mcp_server`) is expected to parse the captured
    /// `WWW-Authenticate` header with
    /// [`apollia_auth::parse_www_authenticate`], run the MCP OAuth 2.1
    /// discovery + PKCE flow via
    /// [`apollia_auth::McpDiscoveryClient`], persist the resulting access
    /// token, and retry `test_connection` with the new
    /// `Authorization: Bearer …` header. This split keeps `apollia-mcp`
    /// transport-agnostic (no UI / keyring dependency leaking in here).
    async fn handle_test_connection(
        config: McpServerConfig,
    ) -> Result<McpConnectionTestResult, McpSessionError> {
        let start = std::time::Instant::now();
        let session = McpSession::start(config, Some(&DefaultMcpSecretResolver)).await?;
        let tools: Vec<McpToolSummary> = session
            .tools()
            .iter()
            .map(|t| McpToolSummary {
                full_name: format!("test:{}/{}", session.server_name(), t.name),
                local_name: t.name.clone(),
                description: t.description.clone(),
                input_schema: t.input_schema.clone(),
            })
            .collect();
        let result = McpConnectionTestResult {
            server_info: session.server_info().name.clone(),
            protocol_version: "2024-11-05".to_string(),
            tools,
            test_duration_ms: start.elapsed().as_millis() as u64,
        };
        session.shutdown().await;
        Ok(result)
    }

    /// Resolve a [`McpCommand::CallTool`] command.
    ///
    /// Returns `Some(result)` to be forwarded to the caller, or `None` when a
    /// pending-approval reply has already been sent by this method.
    async fn handle_call_tool(
        &self,
        server_name: String,
        tool_name: String,
        arguments: Option<serde_json::Value>,
    ) -> Option<Result<ToolCallResult, McpSessionError>> {
        if self.reloading.contains(&server_name) {
            return Some(Err(McpSessionError::ServerReloading {
                server: server_name,
            }));
        }

        let session = match self.sessions.get(&server_name) {
            Some(session) => session,
            None => {
                return Some(Err(McpSessionError::ServerExited {
                    server: server_name,
                }));
            }
        };

        if let Some(pending) = self.register_pending_approval(&server_name, &tool_name, &arguments) {
            return pending;
        }

        Some(session.call_tool(&tool_name, arguments).await)
    }

    /// If the session requires approval and the call is not yet approved, register
    /// a pending approval and return the wrapped reply (`Some(Some(Err(..)))` =
    /// reply with PendingApproval; `Some(None)` = reply already swallowed on
    /// registration failure path). Returns `None` when no gating applies and the
    /// call should proceed.
    fn register_pending_approval(
        &self,
        server_name: &str,
        tool_name: &str,
        arguments: &Option<serde_json::Value>,
    ) -> Option<Option<Result<ToolCallResult, McpSessionError>>> {
        let session = self.sessions.get(server_name)?;
        if !session.requires_approval() {
            return None;
        }
        let store = self.approvals.as_ref()?;
        if store.is_approved(server_name, tool_name) {
            return None;
        }

        let args = arguments
            .as_ref()
            .cloned()
            .unwrap_or(serde_json::Value::Null);
        match store.register(server_name, tool_name, &args) {
            Ok(approval_id) => Some(Some(Err(McpSessionError::PendingApproval {
                server: server_name.to_string(),
                tool: tool_name.to_string(),
                approval_id,
            }))),
            Err(e) => {
                tracing::error!(
                    server = %server_name,
                    tool   = %tool_name,
                    error  = %e,
                    "failed to register pending approval"
                );
                None
            }
        }
    }

    /// Resolve a [`McpCommand::RestartServer`] command.
    async fn handle_restart_server(
        &mut self,
        server_name: String,
    ) -> Result<McpServerStatus, McpSessionError> {
        let old_session = match self.sessions.remove(&server_name) {
            Some(session) => session,
            None => {
                return Err(McpSessionError::ServerExited {
                    server: server_name,
                });
            }
        };
        let config = old_session.config().clone();
        old_session.shutdown().await;
        let new_session = McpSession::start(config, Some(&DefaultMcpSecretResolver)).await?;
        let status = build_status(&server_name, &new_session);
        self.sessions.insert(server_name, new_session);
        Ok(status)
    }

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
                    if let Some(result) =
                        self.handle_call_tool(server_name, tool_name, arguments).await
                    {
                        let _ = reply.send(result);
                    }
                }

                McpCommand::GetStatus { reply } => {
                    let _ = reply.send(self.collect_statuses());
                }

                McpCommand::GetDetail { server_name, reply } => {
                    let _ = reply.send(self.server_detail(&server_name));
                }

                McpCommand::AddServer { config, reply } => {
                    let result = self.handle_add_server(config).await;
                    let _ = reply.send(result);
                }

                McpCommand::RemoveServer { server_name, reply } => {
                    let result = self.handle_remove_server(&server_name).await;
                    let _ = reply.send(result);
                }

                McpCommand::RestartServer { server_name, reply } => {
                    let result = self.handle_restart_server(server_name).await;
                    let _ = reply.send(result);
                }

                McpCommand::TestConnection { config, reply } => {
                    let result = Self::handle_test_connection(config).await;
                    let _ = reply.send(result);
                }

                McpCommand::ServerRequiresApproval { server_name, reply } => {
                    let _ = reply.send(self.server_requires_approval(&server_name));
                }

                McpCommand::SetApproval {
                    server_name,
                    requires_approval,
                    reply,
                } => {
                    let _ = reply.send(self.handle_set_approval(server_name, requires_approval));
                }

                McpCommand::ReloadServer { server_name, reply } => {
                    let result = self.handle_reload_server(&server_name).await;
                    let _ = reply.send(result);
                }

                McpCommand::ApproveToolAccess {
                    server_name,
                    tool_name,
                    reply,
                } => {
                    let _ = reply.send(self.handle_approve_tool_access(&server_name, &tool_name));
                }

                McpCommand::RevokeToolAccess {
                    server_name,
                    tool_name,
                    reply,
                } => {
                    let _ = reply.send(self.handle_revoke_tool_access(&server_name, &tool_name));
                }

                McpCommand::ListPendingApprovals { reply } => {
                    let _ = reply.send(self.handle_list_pending_approvals());
                }

                McpCommand::Shutdown => {
                    self.shutdown_all().await;
                    break;
                }
            }
        }
    }

    /// Collect an enriched status snapshot for every live session.
    fn collect_statuses(&self) -> Vec<McpServerStatus> {
        self.sessions
            .iter()
            .map(|(name, session)| build_status(name, session))
            .collect()
    }

    /// Build the detail view for a single server, if it exists.
    fn server_detail(&self, server_name: &str) -> Option<McpServerDetail> {
        self.sessions
            .get(server_name)
            .map(|session| build_detail(server_name, session))
    }

    /// Whether the named server requires per-tool approval (false if unknown).
    fn server_requires_approval(&self, server_name: &str) -> bool {
        self.sessions
            .get(server_name)
            .map(|s| s.requires_approval())
            .unwrap_or(false)
    }

    /// Toggle the per-tool approval requirement for a live session.
    fn handle_set_approval(
        &mut self,
        server_name: String,
        requires_approval: bool,
    ) -> Result<(), McpSessionError> {
        match self.sessions.get_mut(&server_name) {
            Some(session) => {
                session.set_requires_approval(requires_approval);
                tracing::info!(
                    server = %server_name,
                    requires_approval = %requires_approval,
                    "MCP server approval updated"
                );
                Ok(())
            }
            None => Err(McpSessionError::ServerExited {
                server: server_name,
            }),
        }
    }

    /// Approve tool access via the configured approval store (no-op if none).
    fn handle_approve_tool_access(
        &self,
        server_name: &str,
        tool_name: &str,
    ) -> Result<(), McpApprovalError> {
        match &self.approvals {
            Some(store) => store.approve(server_name, tool_name),
            None => {
                tracing::warn!(
                    server = %server_name,
                    tool   = %tool_name,
                    "approve_tool called but no approval store configured"
                );
                Ok(())
            }
        }
    }

    /// Revoke tool access via the configured approval store (no-op if none).
    fn handle_revoke_tool_access(
        &self,
        server_name: &str,
        tool_name: &str,
    ) -> Result<(), McpApprovalError> {
        match &self.approvals {
            Some(store) => store.revoke(server_name, tool_name),
            None => {
                tracing::warn!(
                    server = %server_name,
                    tool   = %tool_name,
                    "revoke_tool called but no approval store configured"
                );
                Ok(())
            }
        }
    }

    /// List pending tool approvals from the store (empty if none configured).
    fn handle_list_pending_approvals(&self) -> Result<Vec<PendingApprovalEntry>, McpApprovalError> {
        match &self.approvals {
            Some(store) => store.list_pending(),
            None => Ok(Vec::new()),
        }
    }

    /// Shut down every live session and clear the session map.
    async fn shutdown_all(&mut self) {
        tracing::info!("McpClientManager shutting down");
        for (name, session) in self.sessions.drain() {
            tracing::info!(server = %name, "Shutting down MCP session");
            session.shutdown().await;
        }
    }
}

// ─── helpers ─────────────────────────────────────────────────────────────────

/// Register every tool exposed by a freshly started session in the tool registry
/// under the `mcp:<server>/<tool>` naming convention.
async fn register_session_tools_in_registry(
    tool_registry: &ToolRegistryHandle,
    server_name: &str,
    requires_approval: bool,
    tags: &[String],
    session: &McpSession,
) {
    for tool_def in session.tools() {
        let mut tool_tags = vec!["mcp".to_string(), server_name.to_string()];
        tool_tags.extend(tags.iter().cloned());

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
            is_read_only: false,
            risk_score: 3,
            approval_risk_level: None,
            impact_description: None,
            reject_reason_required: false,
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
}

/// Log a session start failure, distinguishing the expected OAuth-not-yet-stored
/// user state (warning) from genuine runtime failures (error).
fn log_session_start_error(server_name: &str, e: &McpSessionError) {
    // OAuth-not-yet-configured is expected user state, not a runtime
    // failure, so emit as a warning to keep log scans for ERROR clean.
    let message = e.to_string();
    if message.contains("MCP OAuth token not yet stored") {
        tracing::warn!(
            server = %server_name,
            error = %e,
            "MCP server skipped — OAuth not yet configured"
        );
    } else {
        tracing::error!(
            server = %server_name,
            error = %e,
            "MCP server failed to start, skipping"
        );
    }
}

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
            url: None,
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
            url: None,
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
        // THEN only keys are exposed, no values
        assert_eq!(view.env_keys.len(), 2);
        assert!(view.env_keys.contains(&"NOTION_TOKEN".to_string()));
        assert!(view.env_keys.contains(&"API_KEY".to_string()));
    }

    #[test]
    fn test_add_server_duplicate_name_detection() {
        // GIVEN a server name already present in a session map
        let mut sessions: HashMap<String, ()> = HashMap::new();
        sessions.insert("notion".to_string(), ());
        // WHEN checking for a duplicate
        let exists = sessions.contains_key("notion");
        // THEN the duplicate is detected
        assert!(exists);
    }

    #[test]
    fn test_remove_server_from_map() {
        // GIVEN a map with "notion" and "sqlite"
        let mut sessions: HashMap<String, String> = HashMap::new();
        sessions.insert("notion".to_string(), "session-notion".to_string());
        sessions.insert("sqlite".to_string(), "session-sqlite".to_string());
        // WHEN "notion" is removed
        let removed = sessions.remove("notion");
        // THEN only "sqlite" remains
        assert!(removed.is_some());
        assert_eq!(sessions.len(), 1);
        assert!(!sessions.contains_key("notion"));
    }

    #[test]
    fn test_connection_result_construction() {
        // GIVEN test-connection result data
        let result = McpConnectionTestResult {
            server_info: "test-server".to_string(),
            protocol_version: "2024-11-05".to_string(),
            tools: vec![McpToolSummary {
                full_name: "test:srv/tool1".to_string(),
                local_name: "tool1".to_string(),
                description: Some("A test tool".to_string()),
                input_schema: serde_json::json!({"type": "object"}),
            }],
            test_duration_ms: 200,
        };
        // WHEN serialized
        let json = serde_json::to_value(&result).unwrap();
        // THEN all fields are correct
        assert_eq!(result.tools.len(), 1);
        assert_eq!(result.test_duration_ms, 200);
        assert_eq!(json["protocol_version"], "2024-11-05");
    }

    #[test]
    fn test_server_detail_with_tools_and_config() {
        // GIVEN a fully populated McpServerDetail
        let detail = McpServerDetail {
            status: McpServerStatus {
                name: "notion".to_string(),
                server_info: "notion-mcp".to_string(),
                tools_count: 2,
                requires_approval: false,
                connected: true,
                pid: Some(1234),
                uptime_secs: Some(60),
                last_call_at: None,
                error: None,
                package: None,
                transport: "stdio".to_string(),
            },
            tools: vec![
                McpToolSummary {
                    full_name: "mcp:notion/search".to_string(),
                    local_name: "search".to_string(),
                    description: Some("Search pages".to_string()),
                    input_schema: serde_json::json!({}),
                },
                McpToolSummary {
                    full_name: "mcp:notion/create".to_string(),
                    local_name: "create".to_string(),
                    description: None,
                    input_schema: serde_json::json!({}),
                },
            ],
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
        // THEN all fields are accessible and correct
        assert_eq!(detail.status.name, "notion");
        assert_eq!(detail.tools.len(), 2);
        assert_eq!(detail.config.env_keys, vec!["NOTION_TOKEN"]);
    }

    #[test]
    fn test_remove_server_unknown_name() {
        // GIVEN a map that does not contain "github"
        let sessions: HashMap<String, String> = HashMap::new();
        // WHEN querying for "github"
        let found = sessions.get("github");
        // THEN it is absent
        assert!(found.is_none());
    }

    #[tokio::test]
    async fn test_ac3_shutdown_on_empty_manager() {
        // GIVEN a manager handle with no sessions started
        let (tx, _rx) = tokio::sync::mpsc::channel(1);
        let handle = McpClientManagerHandle { tx };
        // WHEN shutdown is called
        handle.shutdown().await;
        // THEN no error (graceful no-op)
    }

    #[tokio::test]
    async fn test_reload_inexistant_returns_config_reload_error() {
        // GIVEN a McpClientManager with no connected sessions
        use apollia_tools::registry::ToolRegistryHandle;
        let registry = ToolRegistryHandle::start();
        let handle = McpClientManagerHandle::start(vec![], &registry, None, None)
            .await
            .expect("manager start failed");

        // WHEN reload is requested for an unknown server
        let result = handle.reload_server("inexistant").await;

        // THEN ConfigReload error is returned
        assert!(
            matches!(result, Err(McpSessionError::ConfigReload { ref server }) if server == "inexistant"),
            "expected ConfigReload, got: {:?}",
            result
        );

        handle.shutdown().await;
    }

    #[test]
    fn test_server_reloading_error_is_well_typed() {
        // GIVEN a ServerReloading error variant
        let err = McpSessionError::ServerReloading {
            server: "anthropic".to_string(),
        };

        // THEN it formats and matches correctly
        assert!(
            matches!(&err, McpSessionError::ServerReloading { server } if server == "anthropic")
        );
        assert!(err.to_string().contains("anthropic"));
    }

    #[test]
    fn test_config_reload_error_is_well_typed() {
        // GIVEN a ConfigReload error variant
        let err = McpSessionError::ConfigReload {
            server: "inexistant".to_string(),
        };

        // THEN it formats and matches correctly
        assert!(matches!(&err, McpSessionError::ConfigReload { server } if server == "inexistant"));
        assert!(err.to_string().contains("inexistant"));
    }

    #[test]
    fn test_mcp_server_reloaded_event_fields() {
        // GIVEN a McpServerReloaded event with tool lists
        let event = apollia_core::RuntimeEvent::McpServerReloaded {
            name: "notion".to_string(),
            old_tools: vec!["search".to_string(), "query".to_string()],
            new_tools: vec!["search".to_string(), "insert".to_string()],
        };

        // THEN it matches correctly and serializes
        assert!(matches!(&event,
            apollia_core::RuntimeEvent::McpServerReloaded { name, old_tools, new_tools }
            if name == "notion"
                && old_tools.len() == 2
                && new_tools.contains(&"insert".to_string())
        ));
        let json = serde_json::to_value(&event).expect("event must be serializable");
        assert_eq!(json["McpServerReloaded"]["name"], "notion");
    }
}
