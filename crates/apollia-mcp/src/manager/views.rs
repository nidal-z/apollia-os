//! The read-only views the manager answers with.
//!
//! Split out of `manager.rs`: the actor stays in the parent, the projections
//! that turn a live session into a status, a detail, or a tool index live
//! here, along with the registry registration they feed.

use apollia_core::{McpHealth, SandboxProfile};
use apollia_tools::descriptor::{McpTransport, ToolDescriptor, ToolKind};
use apollia_tools::registry::ToolRegistryHandle;
use apollia_tools::{ActionFamily, FamilySource};

use crate::classify::{classify_configured_tool, classify_mcp_tool};
use crate::config::McpServerConfig;
use crate::manager::{
    McpClientManager, McpServerConfigView, McpServerDetail, McpServerStatus, McpToolSummary,
};
use crate::session::{McpSession, McpSessionError};
use crate::tool_search::ToolIndexSnapshot;

impl McpClientManager {
    /// Aggregate the lightweight tool index across every session, then the
    /// tools kept for the servers that have none.
    ///
    /// Each [`ToolIndexEntry`] is enriched with its owning server name, the
    /// server's configured tags and the family the tool is classified in,
    /// producing a [`ToolIndexSnapshot`] usable by the synthetic `tool_search`
    /// tool. Sessions running in [`LoadingMode::Eager`] contribute nothing,
    /// since their index is empty, and nothing is kept for them.
    ///
    /// [`ToolIndexEntry`]: crate::session::ToolIndexEntry
    /// [`LoadingMode::Eager`]: crate::session::LoadingMode::Eager
    pub(super) fn collect_tool_index(&self) -> Vec<ToolIndexSnapshot> {
        let mut index = Vec::new();
        for session in self.sessions.values() {
            let server_name = session.server_name();
            let hints = if self.request_servers.contains(server_name) {
                ServerHints::Ignored
            } else {
                ServerHints::Read
            };
            for entry in session.tool_index() {
                index.push(index_snapshot(
                    session.config(),
                    hints,
                    IndexedTool {
                        name: &entry.name,
                        description: entry.description.clone(),
                        annotations: entry.annotations,
                        input_schema: session.cached_tool_schema(&entry.name).cloned(),
                    },
                ));
            }
        }
        for (server_name, kept) in &self.kept_index {
            if !self.sessions.contains_key(server_name) {
                index.extend(kept.iter().cloned());
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
}
/// Whether the annotations a server declares on its tools classify them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ServerHints {
    /// A connection someone added: its hints classify, as
    /// [`classify_mcp_tool`] reads them.
    Read,
    /// A server nobody added as a connection: nothing it declares classifies.
    Ignored,
}

/// One tool of a server's index, before it is classified.
pub(super) struct IndexedTool<'a> {
    pub(super) name: &'a str,
    pub(super) description: Option<String>,
    pub(super) annotations: Option<crate::protocol::McpToolAnnotations>,
    pub(super) input_schema: Option<serde_json::Value>,
}

/// The index entry of `tool` on the server `config` describes, classified as
/// the registry would classify it: the selection entry, then the server's
/// hints when `hints` lets them count, else unclassified.
pub(super) fn index_snapshot(
    config: &McpServerConfig,
    hints: ServerHints,
    tool: IndexedTool<'_>,
) -> ToolIndexSnapshot {
    let (family, family_source) = match hints {
        ServerHints::Read => classify_configured_tool(config, tool.name, tool.annotations.as_ref()),
        ServerHints::Ignored => classify_mcp_tool(None, None, config.has_account()),
    };
    ToolIndexSnapshot {
        server_name: config.name.clone(),
        tool_name: tool.name.to_string(),
        description: tool.description,
        tags: config.tags.clone(),
        input_schema: tool.input_schema,
        annotations: tool.annotations,
        family,
        family_source,
    }
}

/// The registry names of the tools `session` exposes, `mcp:<server>/<tool>`.
pub(super) fn registry_names(server_name: &str, session: &McpSession) -> Vec<String> {
    session_tool_names(session)
        .into_iter()
        .map(|tool| format!("mcp:{server_name}/{tool}"))
        .collect()
}

/// Register every tool exposed by a freshly started session in the tool registry
/// under the `mcp:<server>/<tool>` naming convention.
///
/// Callers in [`LoadingMode::Deferred`] skip this step entirely: schemas are not
/// loaded at boot and the runtime exposes the synthetic `tool_search` tool
/// instead, so the registry stays free of `mcp:` descriptors.
///
/// Each descriptor carries the action family [`classify_configured_tool`] decides
/// for the tool, and its source; see [`descriptor_risk`] for the two fields
/// derived from it. Whether a call may run is decided on that family, by
/// governance, when an agent makes it.
///
/// `hints` says whether the server's own annotations may classify its tools.
/// A server nobody added as a connection, such as one a request brings, is
/// read with [`ServerHints::Ignored`]: its tools stay unclassified whatever
/// it declares about them.
pub(super) async fn register_session_tools_in_registry(
    tool_registry: &ToolRegistryHandle,
    server_name: &str,
    tags: &[String],
    session: &McpSession,
    hints: ServerHints,
) {
    for tool_def in session.tools() {
        let descriptor = tool_descriptor(server_name, tags, session.config(), tool_def, hints);
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

/// The registry descriptor of `tool_def`, a tool of the server `config`
/// describes, classified as `hints` allows.
pub(super) fn tool_descriptor(
    server_name: &str,
    tags: &[String],
    config: &McpServerConfig,
    tool_def: &crate::protocol::McpToolDefinition,
    hints: ServerHints,
) -> ToolDescriptor {
    let (family, family_source) = match hints {
        ServerHints::Read => {
            classify_configured_tool(config, &tool_def.name, tool_def.annotations.as_ref())
        }
        ServerHints::Ignored => classify_mcp_tool(None, None, config.has_account()),
    };
    let (is_read_only, risk_score) = descriptor_risk(family, family_source);
    let mut tool_tags = vec!["mcp".to_string(), server_name.to_string()];
    tool_tags.extend(tags.iter().cloned());

    let (server_url, transport) = descriptor_endpoint(config, server_name);
    ToolDescriptor {
        name: format!("mcp:{}/{}", server_name, tool_def.name),
        version: "1.0.0".to_string(),
        description: tool_def
            .description
            .clone()
            .unwrap_or_else(|| format!("MCP tool from {}", server_name)),
        kind: ToolKind::McpServer {
            server_url,
            transport,
            tool_name: tool_def.name.clone(),
        },
        input_schema: tool_def.input_schema.clone(),
        output_schema: None,
        sandbox_profile: SandboxProfile::NetworkRestricted,
        tags: tool_tags,
        family,
        family_source,
        // Nobody has designated which argument of an MCP tool is its target.
        target_arg: None,
        is_read_only,
        risk_score,
        approval_risk_level: None,
        impact_description: None,
        reject_reason_required: false,
    }
}

/// The `is_read_only` flag and `risk_score` of an MCP tool's descriptor,
/// derived from its family.
///
/// `is_read_only` lets a call run concurrently and before a plan is approved,
/// so it is granted only to a reading family taken from the curated
/// selection: a server's own read-only hint classifies the tool but is not
/// trusted to unlock that path, nor is the operator's family. The score follows the family: a read is low,
/// a write, a send or code is medium, and an unclassified tool, sensitive by
/// rule, scores as a write.
pub(super) fn descriptor_risk(family: ActionFamily, source: FamilySource) -> (bool, u8) {
    let reads = matches!(family, ActionFamily::Read | ActionFamily::ReadPersonal);
    let is_read_only = reads && source == FamilySource::Selection;
    let risk_score = if reads { 1 } else { 6 };
    (is_read_only, risk_score)
}

/// The endpoint a registered descriptor reports for a session: where the server
/// is reached, and over which wire protocol.
///
/// Both are read from the configuration the session was started with. A
/// remote server registered as a local subprocess is a descriptor that
/// contradicts the configuration it came from, which is why neither value is
/// written as a constant here.
///
/// `sse` maps to [`McpTransport::Http`]: server-sent events run over HTTP, and
/// the descriptor enum draws its line at the wire protocol, not the framing.
pub(super) fn descriptor_endpoint(
    config: &McpServerConfig,
    server_name: &str,
) -> (String, McpTransport) {
    match config.transport.as_str() {
        "streamable-http" | "sse" => (
            config
                .url
                .clone()
                .unwrap_or_else(|| format!("{}://{}", config.transport, server_name)),
            McpTransport::Http,
        ),
        _ => (format!("stdio://{server_name}"), McpTransport::Stdio),
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
        connected: true,
        dormant: false,
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
/// Name the tools a session exposes, regardless of loading mode.
///
/// A deferred session holds no loaded schemas, so reading `tools()` alone
/// answers "no tools" for a server that published forty. Anything that asks
/// what a session exposes goes through here.
pub(super) fn session_tool_names(session: &McpSession) -> Vec<String> {
    if session.tools().is_empty() {
        session
            .tool_index()
            .iter()
            .map(|t| t.name.clone())
            .collect()
    } else {
        session.tools().iter().map(|t| t.name.clone()).collect()
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
        tags: config.tags.clone(),
    };

    McpServerDetail {
        status: build_status(name, session, last_call_at),
        tools,
        config: config_view,
    }
}

// ─── tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn config_with(transport: &str, url: Option<&str>) -> McpServerConfig {
        McpServerConfig {
            format_version: 1,
            name: "notion".to_string(),
            command: String::new(),
            args: vec![],
            env: std::collections::HashMap::new(),
            transport: transport.to_string(),
            url: url.map(str::to_string),
            init_timeout_secs: 30,
            call_timeout_secs: 60,
            max_response_bytes: 8 * 1024 * 1024,
            max_tools: 256,
            tags: vec![],
            tool_families: Default::default(),
            operator_families: Default::default(),
            origin: None,
        }
    }

    #[test]
    fn test_descriptor_endpoint_follows_a_remote_transport() {
        // GIVEN a server configured over streamable HTTP at a remote URL
        let config = config_with("streamable-http", Some("https://mcp.notion.com/mcp"));

        // WHEN the endpoint of its registered descriptor is built
        let (url, transport) = descriptor_endpoint(&config, "notion");

        // THEN it reports the URL and the wire protocol the configuration
        // declared, not a local subprocess
        assert_eq!(url, "https://mcp.notion.com/mcp");
        assert!(matches!(transport, McpTransport::Http));
    }

    #[test]
    fn test_descriptor_endpoint_follows_an_sse_transport() {
        // GIVEN a server configured over SSE
        let config = config_with("sse", Some("https://example.test/sse"));

        // WHEN its descriptor endpoint is built
        let (url, transport) = descriptor_endpoint(&config, "notion");

        // THEN SSE is reported as the HTTP wire protocol it runs on
        assert_eq!(url, "https://example.test/sse");
        assert!(matches!(transport, McpTransport::Http));
    }

    #[test]
    fn test_descriptor_endpoint_keeps_stdio_local() {
        // GIVEN a server configured as a local subprocess
        let config = config_with("stdio", None);

        // WHEN its descriptor endpoint is built
        let (url, transport) = descriptor_endpoint(&config, "notion");

        // THEN the local form is kept: reporting every server as remote would
        // be the same defect as reporting every server as local
        assert_eq!(url, "stdio://notion");
        assert!(matches!(transport, McpTransport::Stdio));
    }

    #[test]
    fn test_descriptor_endpoint_without_url_names_the_transport() {
        // GIVEN a remote transport whose URL is absent from the configuration
        let config = config_with("sse", None);

        // WHEN its descriptor endpoint is built
        let (url, transport) = descriptor_endpoint(&config, "notion");

        // THEN the placeholder names the declared transport rather than
        // claiming a subprocess that was never spawned
        assert_eq!(url, "sse://notion");
        assert!(matches!(transport, McpTransport::Http));
    }

    #[test]
    fn test_descriptor_risk_follows_the_family() {
        // GIVEN a read from the selection, a read from the server, a write
        //       and an unclassified tool
        // WHEN their descriptor flags are derived
        let selection_read = descriptor_risk(ActionFamily::ReadPersonal, FamilySource::Selection);
        let server_read = descriptor_risk(ActionFamily::Read, FamilySource::Server);
        let write = descriptor_risk(ActionFamily::WriteOrDelete, FamilySource::Server);
        let unclassified = descriptor_risk(ActionFamily::Unclassified, FamilySource::Unsourced);
        // THEN only the selection's read is read-only, and every read scores low
        assert_eq!(selection_read, (true, 1));
        assert_eq!(server_read, (false, 1));
        assert_eq!(write, (false, 6));
        assert_eq!(unclassified, (false, 6));
    }
}
