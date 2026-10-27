//! Servers started when they are needed, and stopped when they are not.
//!
//! A manager started with a tool store launches none of its servers: each
//! one waits, dormant, with the tools it last published. Those tools serve
//! the index (deferred loading) or stay registered (eager loading), so an
//! agent sees them before any server answers. The first call of a tool of a
//! dormant server starts it in a task of its own and waits for its
//! handshake; the calls that arrive meanwhile wait with it. A server that
//! answered no call for the idle delay is stopped, and goes back to dormant
//! with what it published.
//!
//! A server with nothing kept has no tool to offer before it answers, so it
//! is started at once, in a task: the manager does not wait for it either.
//!
//! A paused or removed server leaves the dormant set, so nothing starts it.
//! A dormant server offers the resources it last listed (see `resources`).

use std::time::Duration;

use tokio::time::Instant;

use apollia_core::McpHealth;

use crate::config::{DefaultMcpSecretResolver, McpServerConfig};
use crate::manager::detached::CallReply;
use crate::manager::kept::{fetched_now, keep_session_tools};
use crate::manager::resources::{list_of, WaitingRead};
use crate::manager::views::{log_session_start_error, tool_descriptor, ServerHints};
use crate::manager::{
    McpClientManager, McpCommand, McpServerConfigView, McpServerDetail, McpServerStatus,
    McpToolSummary,
};
use crate::protocol::McpToolDefinition;
use crate::server_repository::{StoredResource, StoredTool};
use crate::session::{LoadingMode, McpSession, McpSessionError};

/// A tool call waiting for its server's handshake.
pub(super) struct WaitingCall {
    pub(super) tool_name: String,
    pub(super) arguments: Option<serde_json::Value>,
    pub(super) reply: CallReply,
}

/// A server the manager knows but runs no session of.
pub(super) struct Dormant {
    /// The configuration its next session starts with.
    pub(super) config: McpServerConfig,
    /// What it last published, kept in `mcp.db`.
    pub(super) tools: Vec<StoredTool>,
    /// The health its last session ended with, when it had one.
    pub(super) health: Option<McpHealth>,
    /// Whether a session of it is being started.
    pub(super) starting: bool,
    /// The calls waiting for that session.
    pub(super) waiting: Vec<WaitingCall>,
    /// The resource reads waiting for that session.
    pub(super) waiting_reads: Vec<WaitingRead>,
}

impl Dormant {
    pub(super) fn new(config: McpServerConfig, tools: Vec<StoredTool>) -> Self {
        Self {
            config,
            tools,
            health: None,
            starting: false,
            waiting: Vec::new(),
            waiting_reads: Vec::new(),
        }
    }
}

/// The definition a kept tool is registered under while its server sleeps.
fn definition_of(tool: &StoredTool) -> McpToolDefinition {
    McpToolDefinition {
        name: tool.name.clone(),
        title: tool.title.clone(),
        description: tool.description.clone(),
        input_schema: tool.input_schema.clone(),
        annotations: tool.annotations,
    }
}

impl McpClientManager {
    /// Register, in eager loading, the descriptors of the tools `dormant`
    /// last published, so an agent can call them before the server starts.
    pub(super) async fn register_dormant(&self, name: &str, dormant: &Dormant) {
        if self.loading_mode != LoadingMode::Eager {
            return;
        }
        for tool in &dormant.tools {
            let descriptor = tool_descriptor(
                name,
                &dormant.config.tags,
                &dormant.config,
                &definition_of(tool),
                ServerHints::Read,
            );
            if let Err(e) = self.tool_registry.register(descriptor).await {
                tracing::event!(
                    tracing::Level::WARN,
                    server = %name,
                    error = %e,
                    "mcp.tool.register.failed"
                );
            }
        }
    }

    /// Take the descriptors of a dormant server out of the registry.
    pub(super) async fn unregister_dormant(&self, name: &str, dormant: &Dormant) {
        if self.loading_mode != LoadingMode::Eager {
            return;
        }
        for tool in &dormant.tools {
            let _ = self
                .tool_registry
                .unregister(&format!("mcp:{name}/{}", tool.name))
                .await;
        }
    }

    /// Start a session of the dormant `name` in a task of its own, unless
    /// one is being started. The task also asks the new session for its
    /// resources. The outcome comes back as [`McpCommand::Started`].
    pub(super) fn begin_start(&mut self, name: &str) {
        let Some(report) = self.commands.upgrade() else {
            return;
        };
        let Some(dormant) = self.dormant.get_mut(name) else {
            return;
        };
        if dormant.starting {
            return;
        }
        dormant.starting = true;
        let config = dormant.config.clone();
        let mode = self.loading_mode;
        let server_name = name.to_string();
        tracing::event!(tracing::Level::INFO, server = %name, "mcp.server.starting");
        tokio::spawn(async move {
            let result = McpSession::start_with_mode(config, Some(&DefaultMcpSecretResolver), mode)
                .await
                .map(Box::new);
            let resources = match &result {
                Ok(session) => list_of(session).await,
                Err(_) => None,
            };
            let _ = report
                .send(McpCommand::Started {
                    server_name,
                    result,
                    resources,
                })
                .await;
        });
    }

    /// A tool call for the dormant `name`: it waits for the session its
    /// arrival starts. Hands the reply back when `name` is not dormant.
    pub(super) fn call_dormant(
        &mut self,
        name: &str,
        tool_name: String,
        arguments: Option<serde_json::Value>,
        reply: CallReply,
    ) -> Result<(), CallReply> {
        let Some(dormant) = self.dormant.get_mut(name) else {
            return Err(reply);
        };
        dormant.waiting.push(WaitingCall {
            tool_name,
            arguments,
            reply,
        });
        self.begin_start(name);
        Ok(())
    }

    /// A session of `name` started, or failed to. Started, it takes the
    /// place of the dormant server, keeps the resources it listed, and runs
    /// the reads and calls that waited for it; failed, the server stays
    /// dormant and they get the failure.
    pub(super) async fn finish_start(
        &mut self,
        name: &str,
        result: Result<Box<McpSession>, McpSessionError>,
        resources: Option<Vec<StoredResource>>,
    ) {
        let Some(mut dormant) = self.dormant.remove(name) else {
            // Removed or paused while it started: nobody waits for it.
            if let Ok(session) = result {
                session.shutdown().await;
            }
            return;
        };
        let session = match result {
            Ok(session) => *session,
            Err(e) => {
                log_session_start_error(name, &e);
                self.emit_health_changed(name, crate::health::from_start_error(&e));
                let cause = e.to_string();
                let mut first = Some(e);
                for call in dormant.waiting.drain(..) {
                    let error = first
                        .take()
                        .unwrap_or_else(|| McpSessionError::InitializeFailed {
                            server: name.to_string(),
                            cause: cause.clone(),
                        });
                    let _ = call.reply.send(Err(error));
                }
                for read in dormant.waiting_reads.drain(..) {
                    let _ = read.reply.send(Err(McpSessionError::InitializeFailed {
                        server: name.to_string(),
                        cause: cause.clone(),
                    }));
                }
                dormant.starting = false;
                self.dormant.insert(name.to_string(), dormant);
                self.publish();
                return;
            }
        };
        tracing::event!(
            tracing::Level::INFO,
            server = %name,
            tools = session.tools().len(),
            indexed = session.tool_index().len(),
            "mcp.server.connected"
        );
        self.unregister_dormant(name, &dormant).await;
        self.register_session_tools(name, &session).await;
        self.kept_index.remove(name);
        keep_session_tools(self.tool_store.as_ref(), name, &session).await;
        self.emit_health_changed(name, session.health().clone());
        self.watch_session_end(name, &session);
        self.sessions.insert(name.to_string(), session);
        self.mark_used(name);
        if let Some(resources) = resources {
            self.keep_resources(name, resources).await;
        }
        for read in dormant.waiting_reads {
            let _ = read.reply.send(self.read_live(name, &read.uri).await);
        }
        for call in dormant.waiting {
            self.spawn_call_tool(name.to_string(), call.tool_name, call.arguments, call.reply);
        }
        self.publish();
    }

    /// Forget the dormant `name`: its descriptors leave the registry and the
    /// calls waiting for it are answered. Answers whether it was dormant.
    pub(super) async fn drop_dormant(&mut self, name: &str) -> bool {
        let Some(dormant) = self.dormant.remove(name) else {
            return false;
        };
        self.unregister_dormant(name, &dormant).await;
        for call in dormant.waiting {
            let _ = call.reply.send(Err(McpSessionError::ServerExited {
                server: name.to_string(),
            }));
        }
        for read in dormant.waiting_reads {
            let _ = read.reply.send(Err(McpSessionError::ServerExited {
                server: name.to_string(),
            }));
        }
        true
    }

    /// Take the dormant `name` out of the dormant set, for a command that
    /// starts it itself; its descriptors stay registered until the caller
    /// replaces them. A server being started is refused.
    pub(super) fn take_dormant(&mut self, name: &str) -> Result<Option<Dormant>, McpSessionError> {
        if self.dormant.get(name).is_some_and(|d| d.starting) {
            return Err(McpSessionError::InitializeFailed {
                server: name.to_string(),
                cause: "the server is starting".to_string(),
            });
        }
        Ok(self.dormant.remove(name))
    }

    /// Start the dormant `name` now, as an addition does, for a command that
    /// needs its session, such as a test. Nothing to do when `name` is not
    /// dormant.
    pub(super) async fn start_dormant_now(&mut self, name: &str) -> Result<(), McpSessionError> {
        let Some(config) = self.dormant.get(name).map(|d| d.config.clone()) else {
            return Ok(());
        };
        let before = self.index_of(name);
        let result = self.handle_add_server(config).await;
        self.settle_kept(name, before).await;
        self.announce_start(name, &result);
        self.publish();
        result.map(|_| ())
    }

    /// Note a use of `name`: its idle delay counts from now.
    pub(super) fn mark_used(&mut self, name: &str) {
        self.last_used.insert(name.to_string(), Instant::now());
    }

    /// The servers that answered no call for the idle delay, none of whose
    /// calls is running.
    fn idle_servers(&self, idle: Duration, now: Instant) -> Vec<String> {
        self.sessions
            .keys()
            .filter(|name| !self.request_servers.contains(*name))
            .filter(|name| self.in_flight.get(*name).copied().unwrap_or(0) == 0)
            .filter(|name| {
                self.last_used
                    .get(*name)
                    .is_some_and(|used| *used + idle <= now)
            })
            .cloned()
            .collect()
    }

    /// When the next idle server is due to stop, when one may be.
    pub(super) fn next_idle_deadline(&self) -> Option<Instant> {
        let idle = self.idle_stop?;
        self.sessions
            .keys()
            .filter(|name| !self.request_servers.contains(*name))
            .filter(|name| self.in_flight.get(*name).copied().unwrap_or(0) == 0)
            .filter_map(|name| self.last_used.get(name))
            .map(|used| *used + idle)
            .min()
    }

    /// Stop every server idle for the idle delay. Each goes back to dormant:
    /// its tools stay in the index (deferred loading) or in the registry
    /// (eager loading), and its next call starts it again.
    pub(super) async fn stop_idle_servers(&mut self) {
        let Some(idle) = self.idle_stop else {
            return;
        };
        let idle_names = self.idle_servers(idle, Instant::now());
        if idle_names.is_empty() {
            return;
        }
        for name in idle_names {
            let before = self.index_of(&name);
            let Some(session) = self.sessions.remove(&name) else {
                continue;
            };
            let mut dormant = Dormant::new(
                session.config().clone(),
                session.stored_tools(&fetched_now()),
            );
            dormant.health = Some(session.health().clone());
            self.last_used.remove(&name);
            self.in_flight.remove(&name);
            session.shutdown().await;
            if self.loading_mode == LoadingMode::Deferred && !before.is_empty() {
                self.kept_index.insert(name.clone(), before);
            }
            tracing::event!(
                tracing::Level::INFO,
                server = %name,
                idle_secs = idle.as_secs(),
                "mcp.server.idle_stopped"
            );
            self.dormant.insert(name, dormant);
        }
        self.publish();
    }

    /// The status of the dormant `name`, as the list of servers shows it:
    /// known, not connected, starting at its next call, with the tools it
    /// last published and the health its last session ended with.
    pub(super) fn dormant_status(&self, name: &str) -> Option<McpServerStatus> {
        let dormant = self.dormant.get(name)?;
        Some(McpServerStatus {
            name: name.to_string(),
            server_info: String::new(),
            tools_count: dormant.tools.len(),
            connected: false,
            dormant: true,
            pid: None,
            uptime_secs: None,
            last_call_at: self.last_call_at.get(name).cloned(),
            error: None,
            package: None,
            transport: dormant.config.transport.clone(),
            health: dormant
                .health
                .clone()
                .unwrap_or(McpHealth::Healthy { verified: false }),
        })
    }

    /// The detail of the dormant `name`: its configuration and the tools it
    /// last published, with no session.
    pub(super) fn dormant_detail(&self, name: &str) -> Option<McpServerDetail> {
        let status = self.dormant_status(name)?;
        let dormant = self.dormant.get(name)?;
        let config = &dormant.config;
        let tools: Vec<McpToolSummary> = dormant
            .tools
            .iter()
            .map(|tool| McpToolSummary {
                full_name: format!("mcp:{name}/{}", tool.name),
                local_name: tool.name.clone(),
                description: tool.description.clone(),
                input_schema: tool.input_schema.clone(),
            })
            .collect();
        Some(McpServerDetail {
            status,
            tools,
            config: McpServerConfigView {
                name: config.name.clone(),
                command: config.command.clone(),
                args: config.args.clone(),
                env_keys: config.env.keys().cloned().collect(),
                transport: config.transport.clone(),
                tags: config.tags.clone(),
            },
        })
    }

    /// Record the operator's family for `tool` of the dormant `name`, so its
    /// next session and, in eager loading, its descriptor read it.
    pub(super) async fn reclassify_dormant(
        &mut self,
        name: &str,
        tool: &str,
        family: apollia_tools::ActionFamily,
    ) {
        let Some(dormant) = self.dormant.get_mut(name) else {
            return;
        };
        dormant
            .config
            .operator_families
            .insert(tool.to_string(), family);
        if self.loading_mode != LoadingMode::Eager {
            return;
        }
        let Some(dormant) = self.dormant.get(name) else {
            return;
        };
        let Some(kept) = dormant.tools.iter().find(|t| t.name == tool) else {
            return;
        };
        let descriptor = tool_descriptor(
            name,
            &dormant.config.tags,
            &dormant.config,
            &definition_of(kept),
            ServerHints::Read,
        );
        let full = descriptor.name.clone();
        let replaced = match self.tool_registry.unregister(&full).await {
            Ok(_) => self.tool_registry.register(descriptor).await,
            Err(e) => Err(e),
        };
        if let Err(e) = replaced {
            tracing::event!(
                tracing::Level::WARN,
                server = %name,
                error = %e,
                "mcp.tool.reclassify.failed"
            );
        }
    }
}
