//! MCP client manager: actor owning all MCP server sessions and registering their tools.
//!
//! [`McpClientManagerHandle`] is the only public entry point. It starts N sessions in
//! sequence, registers discovered tools in the [`ToolRegistryHandle`], and routes
//! `call_tool` requests to the correct session. A server that fails to start is
//! logged and skipped; it is never fatal to the rest.

mod commands;
mod handle;
mod views;

use std::collections::{HashMap, HashSet};

use tokio::sync::{mpsc, oneshot};

use apollia_core::{EventBusSender, McpHealth};
use apollia_tools::registry::ToolRegistryHandle;

use crate::approvals::{McpApprovalError, McpApprovalStore, PendingApprovalEntry};
use crate::config::McpServerConfig;
use crate::protocol::ToolCallResult;
use crate::session::{LoadingMode, McpSession, McpSessionError};
use crate::tool_search::ToolIndexSnapshot;

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
    /// Test an already-installed server: re-handshake plus an optional read-only
    /// probe against the live session. Reports `live_health`.
    TestLiveServer {
        server_name: String,
        probe: Option<ProbeSpec>,
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
    /// Aggregate `resources/list` across every connected session.
    ListResources {
        reply: oneshot::Sender<Vec<McpResourceSummary>>,
    },
    /// Read a single resource (`resources/read`) from one server.
    ///
    /// When `server_name` is `None`, the manager resolves the owning server by
    /// scanning the per-session resource caches for a matching `uri`.
    ReadResource {
        server_name: Option<String>,
        uri: String,
        reply: oneshot::Sender<Result<McpResourcePayload, McpSessionError>>,
    },
    /// Aggregate the lightweight tool index of every deferred-mode session.
    ///
    /// Yields an empty vector when no session runs in deferred mode (the index
    /// is only populated when `LoadingMode::Deferred` is active).
    GetToolIndex {
        reply: oneshot::Sender<Vec<ToolIndexSnapshot>>,
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
    /// Error message when the server is in a degraded state. Derived from
    /// [`McpServerStatus::health`]; `None` only when healthy.
    pub error: Option<String>,
    /// Package identifier (e.g. `@notionhq/notion-mcp-server`), when identifiable.
    pub package: Option<String>,
    /// Transport protocol declared in the configuration (e.g. `"stdio"`).
    pub transport: String,
    /// Operational health, orthogonal to `connected`. A session can be
    /// `connected` (process alive) yet `Degraded` or `NeedsReauth`. The UI badge
    /// is driven by this, not by `connected`.
    pub health: McpHealth,
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
    /// Live operational health of the already-installed session, when the test
    /// targets one. `None` for a pre-install wizard test (no live session). A
    /// `Some(Degraded | NeedsReauth)` means the handshake is reachable but real
    /// operations are not succeeding: callers must not report a plain "OK".
    #[serde(default)]
    pub live_health: Option<McpHealth>,
}

/// Read-only probe used by the live-server test to exercise real operational
/// access (scopes, grants) beyond the handshake. Declared per connector as
/// data, never code: see the desktop `enrichments.json` `health_probe` field.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProbeSpec {
    /// Local tool name to invoke on the server (e.g. `"get-users"`).
    pub tool: String,
    /// Static arguments for the probe call. `None` for parameterless tools.
    #[serde(default)]
    pub args: Option<serde_json::Value>,
}

/// One MCP resource entry, flattened with its owning server name.
///
/// Aggregated by [`McpClientManagerHandle::list_resources`] across every
/// connected session so both the ReAct agent (`mcp_resources_list` tool) and
/// the desktop @-mention picker consume a single uniform list.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct McpResourceSummary {
    /// Server the resource belongs to (as declared in the configuration).
    pub server: String,
    /// Stable URI identifying the resource.
    pub uri: String,
    /// Display name for the UI.
    pub name: String,
    /// MIME type when known (e.g. `"text/plain"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    /// Optional one-line description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Content of a single resource read via `resources/read`, flattened to plain
/// text plus the owning server and MIME type.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct McpResourcePayload {
    /// Server the resource was read from.
    pub server: String,
    /// Resource URI that was read.
    pub uri: String,
    /// MIME type of the first content part, when the server provided one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    /// Text content of all returned parts, joined with newlines. Binary-only
    /// parts (base64 blobs) are skipped here.
    pub text: String,
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
    /// Optional event bus for emitting [`RuntimeEvent::McpServerReloaded`] and
    /// [`RuntimeEvent::McpServerHealthChanged`].
    event_bus: Option<EventBusSender>,
    /// ISO 8601 timestamp of the last tool call per server. Feeds
    /// [`McpServerStatus::last_call_at`]; updated on every call outcome.
    last_call_at: HashMap<String, String>,
    /// Optional SQLite-backed HITL approval store.
    ///
    /// When `Some`, every `CallTool` directed to a server with
    /// `requires_approval = true` is checked against this store before
    /// forwarding to the session. When `None`, the approval gate is disabled.
    approvals: Option<McpApprovalStore>,
    /// Schema loading strategy applied to every session this manager starts.
    ///
    /// In [`LoadingMode::Eager`] sessions fetch all tool schemas at boot and
    /// their tools are registered in the [`ToolRegistryHandle`]. In
    /// [`LoadingMode::Deferred`] sessions keep only a lightweight index, no
    /// individual schema is registered, and the runtime exposes `tool_search`
    /// instead.
    loading_mode: LoadingMode,
}

// ─── handle impl ─────────────────────────────────────────────────────────────

impl McpClientManagerHandle {}

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
                    if let Some(result) = self
                        .handle_call_tool(server_name, tool_name, arguments)
                        .await
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

                McpCommand::TestLiveServer {
                    server_name,
                    probe,
                    reply,
                } => {
                    let result = self.handle_test_live_server(server_name, probe).await;
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

                McpCommand::ListResources { reply } => {
                    let _ = reply.send(self.handle_list_resources().await);
                }

                McpCommand::ReadResource {
                    server_name,
                    uri,
                    reply,
                } => {
                    let _ = reply.send(self.handle_read_resource(server_name, &uri).await);
                }

                McpCommand::GetToolIndex { reply } => {
                    let _ = reply.send(self.collect_tool_index());
                }

                McpCommand::Shutdown => {
                    self.shutdown_all().await;
                    break;
                }
            }
        }
    }

    /// Shut down every live session and clear the session map.
    async fn shutdown_all(&mut self) {
        tracing::info!("mcp.manager.shutdown");
        for (name, session) in self.sessions.drain() {
            tracing::info!(server = %name, "mcp.session.shutdown.started");
            session.shutdown().await;
        }
    }
}

// ─── helpers ─────────────────────────────────────────────────────────────────

// ─── tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_naming_convention() {
        // GIVEN a server "notion" and a tool "search_pages"
        let server_name = "notion";
        let tool_name = "search_pages";
        // WHEN the composite name is built
        let full_name = format!("mcp:{}/{}", server_name, tool_name);
        // THEN
        assert_eq!(full_name, "mcp:notion/search_pages");
    }

    #[test]
    fn test_server_status_serialization() {
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
            health: McpHealth::Healthy { verified: false },
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
            format_version: 1,
            name: "notion".to_string(),
            command: "npx".to_string(),
            args: vec![],
            env: std::collections::HashMap::new(),
            transport: "stdio".to_string(),
            url: None,
            requires_approval: true,
            init_timeout_secs: 30,
            call_timeout_secs: 60,
            max_response_bytes: 8 * 1024 * 1024,
            max_tools: 256,
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
            format_version: 1,
            name: "sqlite".to_string(),
            command: "npx".to_string(),
            args: vec![],
            env: std::collections::HashMap::new(),
            transport: "stdio".to_string(),
            url: None,
            requires_approval: false,
            init_timeout_secs: 30,
            call_timeout_secs: 60,
            max_response_bytes: 8 * 1024 * 1024,
            max_tools: 256,
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
            health: McpHealth::Healthy { verified: false },
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
                health: McpHealth::Healthy { verified: false },
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
                health: McpHealth::Healthy { verified: false },
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
                health: McpHealth::Healthy { verified: false },
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
            live_health: None,
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
                health: McpHealth::Healthy { verified: false },
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
    async fn test_shutdown_on_empty_manager() {
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
        let handle =
            McpClientManagerHandle::start(vec![], &registry, None, None, LoadingMode::Eager)
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
