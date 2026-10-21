//! The work the manager detaches from its loop: tool calls and connection
//! tests.
//!
//! Each runs in a task of its own over a [`crate::session::SessionCaller`],
//! so the actor goes on answering while a server is slow. A call's outcome
//! comes back to the actor as a message, [`McpCommand::CallFinished`] or
//! [`McpCommand::LiveTestFinished`]: the actor records it in the session's
//! health in the order the outcomes arrive, publishes, then answers the
//! caller. The actor stays the only writer of its sessions.

use tokio::sync::{mpsc, oneshot};

use crate::config::McpServerConfig;
use crate::manager::views::session_tool_names;
use crate::manager::{McpClientManager, McpCommand, McpConnectionTestResult, ProbeSpec};
use crate::protocol::ToolCallResult;
use crate::session::McpSessionError;

/// The answer of a tool call.
pub(super) type CallReply = oneshot::Sender<Result<ToolCallResult, McpSessionError>>;
/// The answer of a connection test.
pub(super) type TestReply = oneshot::Sender<Result<McpConnectionTestResult, McpSessionError>>;

impl McpClientManager {
    /// The sender a detached task reports its outcome on, while a handle
    /// still holds the channel open.
    fn report_sender(&self) -> Option<mpsc::Sender<McpCommand>> {
        self.commands.upgrade()
    }

    /// Start a tool call in a task of its own. In deferred mode, a tool the
    /// session's schema cache does not hold has its schema fetched first, on
    /// a best-effort basis: the call does not need it to run.
    pub(super) fn spawn_call_tool(
        &self,
        server_name: String,
        tool_name: String,
        arguments: Option<serde_json::Value>,
        reply: CallReply,
    ) {
        if self.reloading.contains(&server_name) {
            let _ = reply.send(Err(McpSessionError::ServerReloading {
                server: server_name,
            }));
            return;
        }
        let Some(session) = self.sessions.get(&server_name) else {
            let _ = reply.send(Err(McpSessionError::ServerExited {
                server: server_name,
            }));
            return;
        };
        let Some(report) = self.report_sender() else {
            let _ = reply.send(Err(McpSessionError::ServerExited {
                server: server_name,
            }));
            return;
        };
        let caller = session.caller();
        let fetch_schema = session.needs_schema_fetch(&tool_name);
        tokio::spawn(async move {
            let schemas = if fetch_schema {
                match caller.list_tool_schemas(&tool_name).await {
                    Ok(schemas) => Some(schemas),
                    Err(e) => {
                        tracing::warn!(
                            server = %server_name,
                            tool = %tool_name,
                            error = %e,
                            detail = "the tool call continues on the deferred index",
                            "mcp.schema.fetch.failed"
                        );
                        None
                    }
                }
            } else {
                None
            };
            let result = caller.call_tool(&tool_name, arguments).await;
            report_outcome(
                report,
                McpCommand::CallFinished {
                    server_name,
                    schemas,
                    result,
                    reply,
                },
            )
            .await;
        });
    }

    /// Record a finished tool call: cache the schemas it fetched, update the
    /// session's health, publish, then answer the caller.
    pub(super) fn finish_call_tool(
        &mut self,
        server_name: &str,
        schemas: Option<Vec<(String, serde_json::Value)>>,
        result: Result<ToolCallResult, McpSessionError>,
        reply: CallReply,
    ) {
        if let Some(schemas) = schemas {
            if let Some(session) = self.sessions.get_mut(server_name) {
                session.cache_tool_schemas(schemas);
            }
        }
        self.record_call_outcome(server_name, &result);
        self.publish();
        let _ = reply.send(result);
    }

    /// Test a configuration in a task of its own: no session is kept and no
    /// state of the actor changes, so the task answers the caller itself.
    pub(super) fn spawn_test_connection(config: McpServerConfig, reply: TestReply) {
        tokio::spawn(async move {
            let _ = reply.send(Self::handle_test_connection(config).await);
        });
    }

    /// Test an installed server in a task of its own: a fresh handshake for
    /// reachability, then the optional read-only probe on the live session.
    /// A probe whose tool the server does not expose is skipped, and the
    /// verdict is then reachability only.
    pub(super) fn spawn_test_live_server(
        &self,
        server_name: String,
        probe: Option<ProbeSpec>,
        reply: TestReply,
    ) {
        let Some(session) = self.sessions.get(&server_name) else {
            let _ = reply.send(Err(McpSessionError::ServerExited {
                server: server_name,
            }));
            return;
        };
        let Some(report) = self.report_sender() else {
            let _ = reply.send(Err(McpSessionError::ServerExited {
                server: server_name,
            }));
            return;
        };
        let config = session.config().clone();
        let caller = session.caller();
        let probe = probe.filter(|probe| {
            let available = session_tool_names(session).contains(&probe.tool);
            if !available {
                tracing::debug!(
                    server = %server_name,
                    probe = %probe.tool,
                    "mcp.health_probe_skipped_unknown_tool"
                );
            }
            available
        });
        tokio::spawn(async move {
            let test = Self::handle_test_connection(config).await;
            let probe_result = match (&test, probe) {
                (Ok(_), Some(probe)) => Some(caller.call_tool(&probe.tool, probe.args).await),
                _ => None,
            };
            report_outcome(
                report,
                McpCommand::LiveTestFinished {
                    server_name,
                    test,
                    probe_result,
                    reply,
                },
            )
            .await;
        });
    }

    /// Record a finished live test: the probe's outcome updates the session's
    /// health, and the answer carries the health that results.
    pub(super) fn finish_test_live_server(
        &mut self,
        server_name: &str,
        test: Result<McpConnectionTestResult, McpSessionError>,
        probe_result: Option<Result<ToolCallResult, McpSessionError>>,
        reply: TestReply,
    ) {
        if let Some(probe_result) = probe_result {
            self.record_call_outcome(server_name, &probe_result);
            self.publish();
        }
        let answer = test.map(|mut result| {
            result.live_health = self.sessions.get(server_name).map(|s| s.health().clone());
            result
        });
        let _ = reply.send(answer);
    }
}

/// Hand an outcome back to the actor. When the actor has stopped, the
/// outcome is answered to the caller directly, without a health to record.
async fn report_outcome(report: mpsc::Sender<McpCommand>, outcome: McpCommand) {
    let Err(mpsc::error::SendError(outcome)) = report.send(outcome).await else {
        return;
    };
    match outcome {
        McpCommand::CallFinished { result, reply, .. } => {
            let _ = reply.send(result);
        }
        McpCommand::LiveTestFinished { test, reply, .. } => {
            let _ = reply.send(test);
        }
        _ => {}
    }
}
