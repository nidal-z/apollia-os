//! The manager's handle, one method per command.
//!
//! Split out of `manager.rs`: the actor stays in the parent, the cloneable
//! sender every caller holds lives here.

use std::collections::{HashMap, HashSet};

use tokio::sync::{mpsc, oneshot};

use apollia_core::{EventBusSender, RuntimeEvent};
use apollia_tools::registry::ToolRegistryHandle;

use crate::approvals::{McpApprovalError, McpApprovalStore, PendingApprovalEntry};
use crate::config::{DefaultMcpSecretResolver, McpServerConfig};
use crate::manager::views::{log_session_start_error, register_session_tools_in_registry};
use crate::manager::{
    McpClientManager, McpClientManagerHandle, McpCommand, McpConnectionTestResult,
    McpResourcePayload, McpResourceSummary, McpServerDetail, McpServerStatus, ProbeSpec,
};
use crate::protocol::ToolCallResult;
use crate::session::{LoadingMode, McpSession, McpSessionError};
use crate::tool_search::ToolIndexSnapshot;

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
    ///
    /// `loading_mode` selects the schema loading strategy for every session: in
    /// [`LoadingMode::Eager`] all tool schemas are fetched at boot and registered;
    /// in [`LoadingMode::Deferred`] only a lightweight index is kept and no
    /// individual schema is registered.
    pub async fn start(
        configs: Vec<McpServerConfig>,
        tool_registry: &ToolRegistryHandle,
        event_bus: Option<EventBusSender>,
        approvals: Option<McpApprovalStore>,
        loading_mode: LoadingMode,
    ) -> Result<Self, McpSessionError> {
        let (tx, rx) = mpsc::channel(32);
        let mut sessions: HashMap<String, McpSession> = HashMap::new();

        for config in configs {
            let server_name = config.name.clone();
            let requires_approval = config.requires_approval;
            let tags = config.tags.clone();

            match McpSession::start_with_mode(config, Some(&DefaultMcpSecretResolver), loading_mode)
                .await
            {
                Ok(session) => {
                    tracing::info!(
                        server = %server_name,
                        tools = session.tools().len(),
                        indexed = session.tool_index().len(),
                        "mcp.server.connected"
                    );

                    // Eager only: deferred sessions keep their schemas out of the
                    // registry and rely on the synthetic `tool_search` tool.
                    if loading_mode == LoadingMode::Eager {
                        register_session_tools_in_registry(
                            tool_registry,
                            &server_name,
                            requires_approval,
                            &tags,
                            &session,
                        )
                        .await;
                    }

                    sessions.insert(server_name, session);
                }
                Err(e) => {
                    log_session_start_error(&server_name, &e);
                    // Surface a never-started server as NeedsReauth/Unavailable so
                    // the UI does not silently drop it. The actor is not yet
                    // running, so emit directly on the bus.
                    if let Some(ref tx) = event_bus {
                        let _ = tx.send(RuntimeEvent::McpServerHealthChanged {
                            name: server_name.clone(),
                            health: crate::health::from_start_error(&e),
                        });
                    }
                }
            }
        }

        let actor = McpClientManager {
            sessions,
            rx,
            tool_registry: tool_registry.clone(),
            reloading: HashSet::new(),
            event_bus,
            last_call_at: HashMap::new(),
            approvals,
            loading_mode,
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
    /// Collect the lightweight tool index across all deferred-mode sessions.
    ///
    /// Each entry carries the fully qualified `mcp:<server>/<tool>` identity
    /// material plus the owning server's tags, ready to feed the synthetic
    /// `tool_search` tool. Returns an empty `Vec` when no session runs in
    /// deferred mode, when no server is connected, or when the actor has already
    /// shut down.
    pub async fn get_tool_index(&self) -> Vec<ToolIndexSnapshot> {
        let (reply_tx, reply_rx) = oneshot::channel();
        if self
            .tx
            .send(McpCommand::GetToolIndex { reply: reply_tx })
            .await
            .is_err()
        {
            return Vec::new();
        }
        reply_rx.await.unwrap_or_default()
    }
    /// Aggregate `resources/list` across every connected MCP server.
    ///
    /// Returns one [`McpResourceSummary`] per resource, tagged with its owning
    /// server. Servers that do not advertise the `resources` capability are
    /// skipped without a round-trip. Returns an empty `Vec` when no server is
    /// connected or the actor has already shut down.
    pub async fn list_resources(&self) -> Vec<McpResourceSummary> {
        let (reply_tx, reply_rx) = oneshot::channel();
        if self
            .tx
            .send(McpCommand::ListResources { reply: reply_tx })
            .await
            .is_err()
        {
            return Vec::new();
        }
        reply_rx.await.unwrap_or_default()
    }
    /// Read a single resource (`resources/read`).
    ///
    /// When `server_name` is `Some`, the call is routed directly to that
    /// session. When `None`, the manager resolves the owning server by matching
    /// `uri` against the per-session resource list.
    ///
    /// # Errors
    ///
    /// Returns [`McpSessionError::ServerExited`] when the actor channel is
    /// closed, when the named server is unknown, or when no connected server
    /// exposes a resource with the given `uri`. Propagates any
    /// [`McpSessionError`] raised by the underlying `resources/read` call.
    pub async fn read_resource(
        &self,
        server_name: Option<&str>,
        uri: &str,
    ) -> Result<McpResourcePayload, McpSessionError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(McpCommand::ReadResource {
                server_name: server_name.map(str::to_string),
                uri: uri.to_string(),
                reply: reply_tx,
            })
            .await
            .map_err(|_| McpSessionError::ServerExited {
                server: server_name.unwrap_or("?").to_string(),
            })?;
        reply_rx.await.map_err(|_| McpSessionError::ServerExited {
            server: server_name.unwrap_or("?").to_string(),
        })?
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
    /// Test an already-installed server: re-handshake for reachability plus an
    /// optional read-only probe against the live session. The returned
    /// [`McpConnectionTestResult::live_health`] carries the operational verdict.
    pub async fn test_live_server(
        &self,
        server_name: &str,
        probe: Option<ProbeSpec>,
    ) -> Result<McpConnectionTestResult, McpSessionError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(McpCommand::TestLiveServer {
                server_name: server_name.to_string(),
                probe,
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
