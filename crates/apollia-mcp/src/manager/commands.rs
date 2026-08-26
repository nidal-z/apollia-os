//! The manager actor's command handlers.
//!
//! Split out of `manager.rs`: the actor loop stays in the parent, the arm of
//! each `McpCommand` (server lifecycle, tool calls, approvals, resources)
//! lives here.

use apollia_core::{McpHealth, RuntimeEvent};

use crate::approvals::{McpApprovalError, PendingApprovalEntry};
use crate::config::{DefaultMcpSecretResolver, McpServerConfig};
use crate::health::OpOutcome;
use crate::manager::views::{build_status, register_session_tools_in_registry};
use crate::manager::{
    McpClientManager, McpConnectionTestResult, McpResourcePayload, McpResourceSummary,
    McpServerStatus, McpToolSummary, ProbeSpec,
};
use crate::protocol::{extract_text_parts, ToolCallResult};
use crate::session::{LoadingMode, McpSession, McpSessionError};

impl McpClientManager {
    /// Register all tools from a session into the tool registry.
    ///
    /// Uses the `mcp:<server>/<tool>` naming convention. Failures are logged at
    /// `warn` level and do not abort the registration of remaining tools.
    pub(super) async fn register_session_tools(&self, server_name: &str, session: &McpSession) {
        // Deferred sessions never register their schemas; the runtime exposes
        // the synthetic `tool_search` tool over the lightweight index instead.
        if self.loading_mode == LoadingMode::Deferred {
            return;
        }
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
    pub(super) async fn handle_add_server(
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

        let session =
            McpSession::start_with_mode(config, Some(&DefaultMcpSecretResolver), self.loading_mode)
                .await?;
        tracing::info!(
            server = %name,
            tools = session.tools().len(),
            indexed = session.tool_index().len(),
            "mcp.server.added"
        );
        self.register_session_tools(&name, &session).await;
        let status = build_status(
            &name,
            &session,
            self.last_call_at.get(&name).map(String::as_str),
        );
        self.sessions.insert(name, session);
        Ok(status)
    }
    /// Shutdown the session named `name` and remove it from the managed set.
    ///
    /// Returns [`McpSessionError::ServerExited`] when the name is not found.
    pub(super) async fn handle_remove_server(&mut self, name: &str) -> Result<(), McpSessionError> {
        match self.sessions.remove(name) {
            Some(session) => {
                session.shutdown().await;
                tracing::info!(server = %name, "mcp.server.removed");
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
    pub(super) async fn handle_reload_server(&mut self, name: &str) -> Result<(), McpSessionError> {
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

        let new_session = match McpSession::start_with_mode(
            config,
            Some(&DefaultMcpSecretResolver),
            self.loading_mode,
        )
        .await
        {
            Ok(s) => s,
            Err(e) => {
                self.reloading.remove(name);
                tracing::error!(
                    server = %name,
                    error = %e,
                    detail = "the server left the managed set",
                    "mcp.server.reload.failed"
                );
                return Err(e);
            }
        };

        let new_tools: Vec<String> = new_session.tools().iter().map(|t| t.name.clone()).collect();

        tracing::info!(
            server = %name,
            old_tools = ?old_tools,
            new_tools = ?new_tools,
            "mcp.server.reload.completed"
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
    pub(super) async fn handle_test_connection(
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
            live_health: None,
        };
        session.shutdown().await;
        Ok(result)
    }
    /// Test an already-installed server: re-handshake for reachability, then run
    /// an optional read-only probe against the live session to exercise real
    /// operational access. The probe bypasses HITL (it is a system health check,
    /// not an agent action) and updates the live session's health, so the badge
    /// reflects the verdict and a [`RuntimeEvent::McpServerHealthChanged`] fires.
    pub(super) async fn handle_test_live_server(
        &mut self,
        server_name: String,
        probe: Option<ProbeSpec>,
    ) -> Result<McpConnectionTestResult, McpSessionError> {
        let config = match self.sessions.get(&server_name) {
            Some(session) => session.config().clone(),
            None => {
                return Err(McpSessionError::ServerExited {
                    server: server_name,
                });
            }
        };

        let mut result = Self::handle_test_connection(config).await?;

        if let Some(probe) = probe {
            // Only run a probe whose tool the server actually exposes. A
            // misconfigured or absent probe tool then degrades gracefully to a
            // reachability-only verdict instead of a false "not found" failure.
            let probe_available = self
                .sessions
                .get(&server_name)
                .map(|s| s.tools().iter().any(|t| t.name == probe.tool))
                .unwrap_or(false);
            if probe_available {
                let call = {
                    let Some(session) = self.sessions.get(&server_name) else {
                        return Err(McpSessionError::ServerExited {
                            server: server_name,
                        });
                    };
                    session.call_tool(&probe.tool, probe.args).await
                };
                self.record_call_outcome(&server_name, &call);
            } else {
                tracing::debug!(
                    server = %server_name,
                    probe = %probe.tool,
                    "mcp.health_probe_skipped_unknown_tool"
                );
            }
        }

        result.live_health = self.sessions.get(&server_name).map(|s| s.health().clone());
        Ok(result)
    }
    /// Resolve a [`McpCommand::CallTool`] command.
    ///
    /// Returns `Some(result)` to be forwarded to the caller, or `None` when a
    /// pending-approval reply has already been sent by this method.
    pub(super) async fn handle_call_tool(
        &mut self,
        server_name: String,
        tool_name: String,
        arguments: Option<serde_json::Value>,
    ) -> Option<Result<ToolCallResult, McpSessionError>> {
        if self.reloading.contains(&server_name) {
            return Some(Err(McpSessionError::ServerReloading {
                server: server_name,
            }));
        }

        // Approval gate first; its immutable borrow is fully released before the
        // await below.
        if let Some(pending) = self.register_pending_approval(&server_name, &tool_name, &arguments)
        {
            return pending;
        }

        // In deferred mode, warm the schema cache on first use. This mutable
        // borrow is opened and dropped in its own scope before the immutable
        // borrow taken by the call below. The fetch is best-effort: a failure
        // here is logged and ignored, since `call_tool` does not require the
        // schema to execute.
        if self.loading_mode == LoadingMode::Deferred {
            if let Some(session) = self.sessions.get_mut(&server_name) {
                if let Err(e) = session.fetch_tool_schema(&tool_name).await {
                    tracing::warn!(
                        server = %server_name,
                        tool = %tool_name,
                        error = %e,
                        detail = "the tool call continues on the deferred index",
                        "mcp.schema.fetch.failed"
                    );
                }
            }
        }

        // Build and await the call inside a scope so the immutable borrow of
        // `self.sessions` ends before `record_call_outcome` takes `&mut self`.
        let result = {
            let session = match self.sessions.get(&server_name) {
                Some(session) => session,
                None => {
                    return Some(Err(McpSessionError::ServerExited {
                        server: server_name,
                    }));
                }
            };
            session.call_tool(&tool_name, arguments).await
        };

        self.record_call_outcome(&server_name, &result);
        Some(result)
    }
    /// Classify a tool-call outcome and update the session's health, emitting
    /// [`RuntimeEvent::McpServerHealthChanged`] only when the state changes.
    pub(super) fn record_call_outcome(
        &mut self,
        server_name: &str,
        result: &Result<ToolCallResult, McpSessionError>,
    ) {
        let now = chrono::Utc::now().to_rfc3339();
        self.last_call_at
            .insert(server_name.to_string(), now.clone());

        // Keep the joined error text alive past the borrow handed to next_health.
        let error_text = match result {
            Ok(r) if r.is_error.unwrap_or(false) => Some(extract_text_parts(&r.content)),
            _ => None,
        };
        let outcome = match (result, error_text.as_deref()) {
            (Ok(_), Some(text)) => OpOutcome::ToolError(text),
            (Ok(_), None) => OpOutcome::Success,
            (Err(e), _) => OpOutcome::SessionError(e),
        };

        let Some(session) = self.sessions.get_mut(server_name) else {
            return;
        };
        let prev = session.health().clone();
        let next = crate::health::next_health(&prev, outcome, &now);
        if next != prev {
            session.set_health(next.clone());
            self.emit_health_changed(server_name, next);
        }
    }
    /// Publish a health transition on the event bus (no-op without a bus).
    pub(super) fn emit_health_changed(&self, name: &str, health: McpHealth) {
        if let Some(ref tx) = self.event_bus {
            let _ = tx.send(RuntimeEvent::McpServerHealthChanged {
                name: name.to_string(),
                health,
            });
        }
        tracing::info!(server = %name, "mcp.health_changed");
    }
    /// If the session requires approval and the call is not yet approved, register
    /// a pending approval and return the wrapped reply (`Some(Some(Err(..)))` =
    /// reply with PendingApproval; `Some(None)` = reply already swallowed on
    /// registration failure path). Returns `None` when no gating applies and the
    /// call should proceed.
    pub(super) fn register_pending_approval(
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
                    "mcp.approval.register.failed"
                );
                None
            }
        }
    }
    /// Resolve a [`McpCommand::RestartServer`] command.
    pub(super) async fn handle_restart_server(
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
        let new_session =
            McpSession::start_with_mode(config, Some(&DefaultMcpSecretResolver), self.loading_mode)
                .await?;
        // A restart resets the live session; a stale last_call_at would be
        // misleading, so clear it and report no prior call.
        self.last_call_at.remove(&server_name);
        let status = build_status(&server_name, &new_session, None);
        self.sessions.insert(server_name, new_session);
        Ok(status)
    }
    /// Toggle the per-tool approval requirement for a live session.
    pub(super) fn handle_set_approval(
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
                    "mcp.server.approval.updated"
                );
                Ok(())
            }
            None => Err(McpSessionError::ServerExited {
                server: server_name,
            }),
        }
    }
    /// Approve tool access via the configured approval store (no-op if none).
    pub(super) fn handle_approve_tool_access(
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
                    reason = "no approval store is configured",
                    "mcp.tool.approve.ignored"
                );
                Ok(())
            }
        }
    }
    /// Revoke tool access via the configured approval store (no-op if none).
    pub(super) fn handle_revoke_tool_access(
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
                    reason = "no approval store is configured",
                    "mcp.tool.revoke.ignored"
                );
                Ok(())
            }
        }
    }
    /// List pending tool approvals from the store (empty if none configured).
    pub(super) fn handle_list_pending_approvals(
        &self,
    ) -> Result<Vec<PendingApprovalEntry>, McpApprovalError> {
        match &self.approvals {
            Some(store) => store.list_pending(),
            None => Ok(Vec::new()),
        }
    }
    /// Aggregate `resources/list` across every connected session.
    ///
    /// Servers that do not advertise the `resources` capability are skipped
    /// without a round-trip. A per-server failure is logged and skipped; it
    /// never aborts the aggregation of the others.
    pub(super) async fn handle_list_resources(&self) -> Vec<McpResourceSummary> {
        let mut out: Vec<McpResourceSummary> = Vec::new();
        for (server_name, session) in &self.sessions {
            if session.capabilities().resources.is_none() {
                continue;
            }
            match session.list_resources().await {
                Ok(result) => {
                    for resource in result.resources {
                        out.push(McpResourceSummary {
                            server: server_name.clone(),
                            uri: resource.uri,
                            name: resource.name,
                            mime_type: resource.mime_type,
                            description: resource.description,
                        });
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        server = %server_name,
                        error = %e,
                        "mcp.resources_list_failed"
                    );
                }
            }
        }
        out
    }
    /// Resolve and read a single resource via `resources/read`.
    ///
    /// When `server_name` is `None`, the owning server is resolved by matching
    /// `uri` against each connected session's resource list.
    pub(super) async fn handle_read_resource(
        &self,
        server_name: Option<String>,
        uri: &str,
    ) -> Result<McpResourcePayload, McpSessionError> {
        let resolved = match server_name {
            Some(name) => name,
            None => self.resolve_resource_server(uri).await.ok_or_else(|| {
                McpSessionError::ServerExited {
                    server: format!("(none exposes resource {uri})"),
                }
            })?,
        };

        let session =
            self.sessions
                .get(&resolved)
                .ok_or_else(|| McpSessionError::ServerExited {
                    server: resolved.clone(),
                })?;

        let result = session.read_resource(uri).await?;
        let mime_type = result.contents.iter().find_map(|c| c.mime_type.clone());
        let text = result
            .contents
            .iter()
            .filter_map(|c| c.text.as_deref())
            .collect::<Vec<_>>()
            .join("\n");

        Ok(McpResourcePayload {
            server: resolved,
            uri: uri.to_string(),
            mime_type,
            text,
        })
    }
    /// Find the first connected server that exposes a resource with `uri`.
    pub(super) async fn resolve_resource_server(&self, uri: &str) -> Option<String> {
        for (server_name, session) in &self.sessions {
            if session.capabilities().resources.is_none() {
                continue;
            }
            if let Ok(result) = session.list_resources().await {
                if result.resources.iter().any(|r| r.uri == uri) {
                    return Some(server_name.clone());
                }
            }
        }
        None
    }
}
