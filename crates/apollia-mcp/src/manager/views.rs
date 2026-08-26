//! The read-only views the manager answers with.
//!
//! Split out of `manager.rs`: the actor stays in the parent, the projections
//! that turn a live session into a status, a detail, or a tool index live
//! here, along with the registry registration they feed.

use apollia_core::{McpHealth, SandboxProfile};
use apollia_tools::descriptor::{McpTransport, ToolDescriptor, ToolKind};
use apollia_tools::registry::ToolRegistryHandle;

use crate::manager::{
    McpClientManager, McpServerConfigView, McpServerDetail, McpServerStatus, McpToolSummary,
};
use crate::session::{McpSession, McpSessionError};
use crate::tool_search::ToolIndexSnapshot;

impl McpClientManager {
    /// Collect an enriched status snapshot for every live session.
    pub(super) fn collect_statuses(&self) -> Vec<McpServerStatus> {
        self.sessions
            .iter()
            .map(|(name, session)| {
                build_status(
                    name,
                    session,
                    self.last_call_at.get(name).map(String::as_str),
                )
            })
            .collect()
    }
    /// Aggregate the lightweight tool index across every session.
    ///
    /// Each [`ToolIndexEntry`] is enriched with its owning server name and the
    /// server's configured tags, producing a [`ToolIndexSnapshot`] usable by the
    /// synthetic `tool_search` tool. Sessions running in [`LoadingMode::Eager`]
    /// contribute nothing, since their index is empty.
    ///
    /// [`ToolIndexEntry`]: crate::session::ToolIndexEntry
    pub(super) fn collect_tool_index(&self) -> Vec<ToolIndexSnapshot> {
        let mut index = Vec::new();
        for session in self.sessions.values() {
            let server_name = session.server_name().to_string();
            let tags = session.config().tags.clone();
            for entry in session.tool_index() {
                index.push(ToolIndexSnapshot {
                    server_name: server_name.clone(),
                    tool_name: entry.name.clone(),
                    description: entry.description.clone(),
                    tags: tags.clone(),
                    input_schema: session.cached_tool_schema(&entry.name).cloned(),
                });
            }
        }
        index
    }
    /// Build the detail view for a single server, if it exists.
    pub(super) fn server_detail(&self, server_name: &str) -> Option<McpServerDetail> {
        self.sessions.get(server_name).map(|session| {
            build_detail(
                server_name,
                session,
                self.last_call_at.get(server_name).map(String::as_str),
            )
        })
    }
    /// Whether the named server requires per-tool approval (false if unknown).
    pub(super) fn server_requires_approval(&self, server_name: &str) -> bool {
        self.sessions
            .get(server_name)
            .map(|s| s.requires_approval())
            .unwrap_or(false)
    }
}
/// Register every tool exposed by a freshly started session in the tool registry
/// under the `mcp:<server>/<tool>` naming convention.
///
/// Callers in [`LoadingMode::Deferred`] skip this step entirely: schemas are not
/// loaded at boot and the runtime exposes the synthetic `tool_search` tool
/// instead, so the registry stays free of `mcp:` descriptors.
pub(super) async fn register_session_tools_in_registry(
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
                    "mcp.tool.registered"
                );
            }
            Err(e) => {
                tracing::warn!(
                    server = %server_name,
                    tool = %tool_def.name,
                    error = %e,
                    "mcp.tool.register.failed"
                );
            }
        }
    }
}
/// Log a session start failure, distinguishing the expected OAuth-not-yet-stored
/// user state (warning) from genuine runtime failures (error).
pub(super) fn log_session_start_error(server_name: &str, e: &McpSessionError) {
    // OAuth-not-yet-configured is expected user state, not a runtime
    // failure, so emit as a warning to keep log scans for ERROR clean.
    let message = e.to_string();
    if message.contains("MCP OAuth token not yet stored") {
        tracing::warn!(
            server = %server_name,
            error = %e,
            reason = "OAuth is not configured yet",
            "mcp.server.skipped"
        );
    } else {
        tracing::error!(
            server = %server_name,
            error = %e,
            reason = "the server failed to start",
            "mcp.server.skipped"
        );
    }
}
/// Build an enriched [`McpServerStatus`] snapshot from a live session.
pub(super) fn build_status(
    name: &str,
    session: &McpSession,
    last_call_at: Option<&str>,
) -> McpServerStatus {
    let health = session.health().clone();
    let error = match &health {
        McpHealth::Healthy { .. } => None,
        McpHealth::Degraded { last_error, .. } => Some(last_error.clone()),
        McpHealth::NeedsReauth { reason } | McpHealth::Unavailable { reason } => {
            Some(reason.clone())
        }
    };
    McpServerStatus {
        name: name.to_string(),
        server_info: session.server_info().name.clone(),
        tools_count: session_tool_count(session),
        requires_approval: session.requires_approval(),
        connected: true,
        pid: session.pid(),
        uptime_secs: Some(session.uptime_secs()),
        last_call_at: last_call_at.map(str::to_string),
        error,
        package: None,
        transport: session.config().transport.clone(),
        health,
    }
}
/// Count the tools a session exposes, regardless of loading mode.
///
/// Eager sessions report their fully loaded `tools`; deferred sessions report
/// their lightweight `tool_index`, so the UI tool count is correct in both modes.
pub(super) fn session_tool_count(session: &McpSession) -> usize {
    if session.tools().is_empty() {
        session.tool_index().len()
    } else {
        session.tools().len()
    }
}
/// Build a [`McpServerDetail`] from a live session, redacting secret env values.
pub(super) fn build_detail(
    name: &str,
    session: &McpSession,
    last_call_at: Option<&str>,
) -> McpServerDetail {
    let config = session.config();
    // Deferred sessions hold no schemas, only the lightweight index; surface
    // those tools with a null `input_schema` placeholder so the detail view is
    // not blank before a schema is fetched on demand.
    let tools = if session.tools().is_empty() {
        session
            .tool_index()
            .iter()
            .map(|t| McpToolSummary {
                full_name: format!("mcp:{}/{}", name, t.name),
                local_name: t.name.clone(),
                description: t.description.clone(),
                input_schema: serde_json::Value::Null,
            })
            .collect()
    } else {
        session
            .tools()
            .iter()
            .map(|t| McpToolSummary {
                full_name: format!("mcp:{}/{}", name, t.name),
                local_name: t.name.clone(),
                description: t.description.clone(),
                input_schema: t.input_schema.clone(),
            })
            .collect()
    };

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
        status: build_status(name, session, last_call_at),
        tools,
        config: config_view,
    }
}
