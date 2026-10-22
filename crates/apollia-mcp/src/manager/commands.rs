//! The manager actor's command handlers.
//!
//! Split out of `manager.rs`: the actor loop stays in the parent, the arm of
//! each `McpCommand` (server lifecycle, tool calls, resources)
//! lives here.

use apollia_core::{McpHealth, RuntimeEvent};

use crate::config::{DefaultMcpSecretResolver, McpServerConfig};
use crate::health::OpOutcome;
use crate::manager::views::{
    build_status, register_session_tools_in_registry, registry_names, session_tool_names,
    ServerHints,
};
use crate::manager::{
    McpClientManager, McpConnectionTestResult, McpResourcePayload, McpResourceSummary,
    McpServerStatus, McpToolSummary,
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
        let hints = if self.request_servers.contains(server_name) {
            ServerHints::Ignored
        } else {
            ServerHints::Read
        };
        register_session_tools_in_registry(&self.tool_registry, server_name, &tags, session, hints)
            .await;
    }
    /// Start a session for `config` over `transport`, a server a request
    /// brought: its tools are registered unclassified, and it is remembered
    /// as a request server until it is removed.
    pub(super) async fn handle_add_request_server(
        &mut self,
        config: McpServerConfig,
        transport: Box<dyn crate::transport::McpTransport>,
    ) -> Result<McpServerStatus, McpSessionError> {
        let name = config.name.clone();
        if self.sessions.contains_key(&name) {
            return Err(McpSessionError::InitializeFailed {
                server: name,
                cause: "server with this name already exists".to_string(),
            });
        }
        let session =
            McpSession::start_with_transport(config, transport, self.loading_mode).await?;
        tracing::info!(
            server = %name,
            tools = session.tools().len(),
            indexed = session.tool_index().len(),
            "mcp.request_server.added"
        );
        self.request_servers.insert(name.clone());
        self.register_session_tools(&name, &session).await;
        let status = build_status(&name, &session, None);
        self.watch_session_end(&name, &session);
        self.sessions.insert(name, session);
        Ok(status)
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
        // A dormant server of that name gives way to the session, and comes
        // back when the session does not start.
        let dormant = self.take_dormant(&name)?;

        let session = match McpSession::start_with_mode(
            config,
            Some(&DefaultMcpSecretResolver),
            self.loading_mode,
        )
        .await
        {
            Ok(session) => session,
            Err(e) => {
                if let Some(dormant) = dormant {
                    self.dormant.insert(name, dormant);
                }
                return Err(e);
            }
        };
        tracing::info!(
            server = %name,
            tools = session.tools().len(),
            indexed = session.tool_index().len(),
            "mcp.server.added"
        );
        if let Some(dormant) = &dormant {
            self.unregister_dormant(&name, dormant).await;
        }
        self.register_session_tools(&name, &session).await;
        let status = build_status(
            &name,
            &session,
            self.last_call_at.get(&name).map(String::as_str),
        );
        self.watch_session_end(&name, &session);
        self.sessions.insert(name.clone(), session);
        self.mark_used(&name);
        Ok(status)
    }
    /// Shutdown the session named `name`, remove it from the managed set, and
    /// take the descriptors it registered out of the tool registry.
    ///
    /// Returns [`McpSessionError::ServerExited`] when the name is not found.
    pub(super) async fn handle_remove_server(&mut self, name: &str) -> Result<(), McpSessionError> {
        match self.sessions.remove(name) {
            Some(session) => {
                self.request_servers.remove(name);
                // A deferred session registered no descriptor.
                if self.loading_mode != LoadingMode::Deferred {
                    for tool in registry_names(name, &session) {
                        if let Err(e) = self.tool_registry.unregister(&tool).await {
                            tracing::warn!(
                                server = %name,
                                error = %e,
                                "mcp.server.unregister.failed"
                            );
                        }
                    }
                }
                self.last_call_at.remove(name);
                self.last_used.remove(name);
                self.in_flight.remove(name);
                session.shutdown().await;
                tracing::info!(server = %name, "mcp.server.removed");
                Ok(())
            }
            None if self.drop_dormant(name).await => {
                self.last_call_at.remove(name);
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
        if self.request_servers.contains(name) {
            return Err(McpSessionError::ConfigReload {
                server: name.to_string(),
            });
        }
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

        let old_tools: Vec<String> = session_tool_names(&old_session);
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

        let new_tools: Vec<String> = session_tool_names(&new_session);

        tracing::info!(
            server = %name,
            old_tools = ?old_tools,
            new_tools = ?new_tools,
            "mcp.server.reload.completed"
        );

        self.register_session_tools(name, &new_session).await;
        self.watch_session_end(name, &new_session);
        self.sessions.insert(name.to_string(), new_session);
        self.mark_used(name);
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
            protocol_version: session.protocol_version().to_string(),
            tools,
            test_duration_ms: start.elapsed().as_millis() as u64,
            live_health: None,
        };
        session.shutdown().await;
        Ok(result)
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
    /// Resolve a [`McpCommand::RestartServer`] command.
    pub(super) async fn handle_restart_server(
        &mut self,
        server_name: String,
    ) -> Result<McpServerStatus, McpSessionError> {
        if self.request_servers.contains(&server_name) {
            return Err(McpSessionError::ConfigReload {
                server: server_name,
            });
        }
        let old_session = match self.sessions.remove(&server_name) {
            Some(session) => session,
            None => {
                // A dormant server has no session to stop: it starts.
                let config = self.dormant.get(&server_name).map(|d| d.config.clone());
                return match config {
                    Some(config) => self.handle_add_server(config).await,
                    None => Err(McpSessionError::ServerExited {
                        server: server_name,
                    }),
                };
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
        self.watch_session_end(&server_name, &new_session);
        self.sessions.insert(server_name.clone(), new_session);
        self.mark_used(&server_name);
        Ok(status)
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
