//! Native agent-facing tools exposing MCP `resources` to the ReAct loop.
//!
//! MCP resources are exposed through two complementary, never auto-injected
//! paths. This module implements the agent-initiative path: two read-only
//! native tools the ReAct agent can call on its own initiative, mirroring how
//! `file_read` works.
//!
//! - [`McpResourcesListExecutor`] (`mcp_resources_list`) aggregates
//!   `resources/list` across every connected MCP server.
//! - [`McpResourcesReadExecutor`] (`mcp_resources_read`) reads a single
//!   resource by URI via `resources/read`.
//!
//! Both route through the same [`McpClientManagerHandle`] as the MCP tool
//! executors, so there is a single dispatch path. They are marked
//! `is_read_only = true` (concurrent batch eligible) and never inject anything
//! into the agent context on their own.

use serde_json::Value;

use apollia_core::SandboxProfile;
use apollia_tools::descriptor::{ToolDescriptor, ToolKind};
use apollia_tools::executor::{ToolExecutionError, ToolExecutor};

use crate::manager::McpClientManagerHandle;

/// Canonical tool name for the resources listing tool.
pub const MCP_RESOURCES_LIST: &str = "mcp_resources_list";
/// Canonical tool name for the resource read tool.
pub const MCP_RESOURCES_READ: &str = "mcp_resources_read";

/// Build the static descriptors for the two MCP resource tools.
///
/// Registered at supervisor boot alongside the connector and native
/// descriptors so the agent's tool catalogue advertises them regardless of
/// whether any MCP server is currently connected. The matching executors are
/// wired per agent via [`build_mcp_resource_executors`].
pub fn mcp_resource_descriptors() -> Vec<ToolDescriptor> {
    vec![list_descriptor(), read_descriptor()]
}

fn list_descriptor() -> ToolDescriptor {
    ToolDescriptor {
        name: MCP_RESOURCES_LIST.to_string(),
        version: "1.0.0".to_string(),
        description: "List the resources exposed by every connected MCP server \
            (files, pages, datasets, context). Returns an array of \
            {server, uri, name, mime_type, description}. Read-only. Call this \
            on your own initiative when you need to discover available context, \
            then read a specific entry with mcp_resources_read."
            .to_string(),
        kind: ToolKind::Native,
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {},
            "required": []
        }),
        output_schema: None,
        sandbox_profile: SandboxProfile::ReadOnly,
        tags: vec!["mcp".to_string(), "resources".to_string(), "read".to_string()],
        dangerous: false,
        is_read_only: true,
        risk_score: 1,
        approval_risk_level: None,
        impact_description: Some("Lists MCP resources across connected servers (read-only).".into()),
        reject_reason_required: false,
    }
}

fn read_descriptor() -> ToolDescriptor {
    ToolDescriptor {
        name: MCP_RESOURCES_READ.to_string(),
        version: "1.0.0".to_string(),
        description: "Read the content of a single MCP resource by its URI. \
            Pass `uri` (and optionally `server` to disambiguate when several \
            servers expose the same URI). Returns the resource text content. \
            Read-only. Use mcp_resources_list first to discover URIs."
            .to_string(),
        kind: ToolKind::Native,
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "uri": {
                    "type": "string",
                    "description": "Resource URI to read, as returned by mcp_resources_list."
                },
                "server": {
                    "type": "string",
                    "description": "Optional MCP server name to target directly. \
                        Omit to let Apollia resolve the owning server from the URI."
                }
            },
            "required": ["uri"]
        }),
        output_schema: None,
        sandbox_profile: SandboxProfile::ReadOnly,
        tags: vec!["mcp".to_string(), "resources".to_string(), "read".to_string()],
        dangerous: false,
        is_read_only: true,
        risk_score: 1,
        approval_risk_level: None,
        impact_description: Some("Reads the content of one MCP resource (read-only).".into()),
        reject_reason_required: false,
    }
}

/// Build the two MCP resource [`ToolExecutor`]s bound to `handle`.
///
/// Returns an empty `Vec` when no MCP handle is configured, mirroring how the
/// per-server MCP executors degrade: the tools then surface as absent rather
/// than failing at call time.
pub fn build_mcp_resource_executors(
    handle: &Option<McpClientManagerHandle>,
) -> Vec<Box<dyn ToolExecutor>> {
    match handle {
        Some(h) => vec![
            Box::new(McpResourcesListExecutor::new(h.clone())),
            Box::new(McpResourcesReadExecutor::new(h.clone())),
        ],
        None => Vec::new(),
    }
}

// ─── list executor ──────────────────────────────────────────────────────────

/// [`ToolExecutor`] for `mcp_resources_list`.
///
/// Aggregates `resources/list` across all connected MCP servers through the
/// shared [`McpClientManagerHandle`].
pub struct McpResourcesListExecutor {
    handle: McpClientManagerHandle,
}

impl McpResourcesListExecutor {
    /// Create a new executor bound to the MCP client manager `handle`.
    pub fn new(handle: McpClientManagerHandle) -> Self {
        Self { handle }
    }
}

#[async_trait::async_trait]
impl ToolExecutor for McpResourcesListExecutor {
    fn name(&self) -> &str {
        MCP_RESOURCES_LIST
    }

    /// List MCP resources across every connected server.
    ///
    /// # Errors
    ///
    /// Returns [`ToolExecutionError::ExecutionFailed`] only if the aggregated
    /// list cannot be serialised. An empty list (no servers, no resources) is a
    /// success, not an error.
    async fn execute(&self, _input: Value) -> Result<Value, ToolExecutionError> {
        let resources = self.handle.list_resources().await;
        serde_json::to_value(&resources)
            .map(|resources| serde_json::json!({ "resources": resources }))
            .map_err(|e| ToolExecutionError::ExecutionFailed {
                code: "serialize_failed".to_string(),
                message: e.to_string(),
            })
    }
}

// ─── read executor ──────────────────────────────────────────────────────────

/// [`ToolExecutor`] for `mcp_resources_read`.
///
/// Reads one resource by URI via `resources/read` through the shared
/// [`McpClientManagerHandle`].
pub struct McpResourcesReadExecutor {
    handle: McpClientManagerHandle,
}

impl McpResourcesReadExecutor {
    /// Create a new executor bound to the MCP client manager `handle`.
    pub fn new(handle: McpClientManagerHandle) -> Self {
        Self { handle }
    }
}

#[async_trait::async_trait]
impl ToolExecutor for McpResourcesReadExecutor {
    fn name(&self) -> &str {
        MCP_RESOURCES_READ
    }

    /// Read a single MCP resource identified by `uri`.
    ///
    /// # Errors
    ///
    /// - [`ToolExecutionError::InvalidInput`] when `uri` is missing or empty.
    /// - [`ToolExecutionError::ExecutionFailed`] when no connected server
    ///   exposes the URI, or the underlying `resources/read` call fails.
    async fn execute(&self, input: Value) -> Result<Value, ToolExecutionError> {
        let uri = input
            .get("uri")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| ToolExecutionError::InvalidInput {
                message: "missing required field `uri`".to_string(),
            })?;
        let server = input.get("server").and_then(Value::as_str);

        let payload = self
            .handle
            .read_resource(server, uri)
            .await
            .map_err(|e| ToolExecutionError::ExecutionFailed {
                code: "mcp_resource_read_failed".to_string(),
                message: e.to_string(),
            })?;

        serde_json::to_value(&payload).map_err(|e| ToolExecutionError::ExecutionFailed {
            code: "serialize_failed".to_string(),
            message: e.to_string(),
        })
    }
}

// ─── tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn descriptors_are_read_only_and_named() {
        // GIVEN the two MCP resource descriptors
        let descriptors = mcp_resource_descriptors();
        // WHEN inspecting them
        // THEN both are present, read-only, and carry the canonical names
        assert_eq!(descriptors.len(), 2);
        assert_eq!(descriptors[0].name, MCP_RESOURCES_LIST);
        assert_eq!(descriptors[1].name, MCP_RESOURCES_READ);
        assert!(descriptors.iter().all(|d| d.is_read_only));
        assert!(descriptors.iter().all(|d| !d.dangerous));
    }

    #[test]
    fn read_descriptor_requires_uri() {
        // GIVEN the read tool descriptor
        let read = read_descriptor();
        // WHEN inspecting its input schema
        let required = &read.input_schema["required"];
        // THEN `uri` is required, `server` is optional
        assert_eq!(required[0], "uri");
        assert!(read.input_schema["properties"]["server"].is_object());
    }

    #[test]
    fn build_executors_empty_without_handle() {
        // GIVEN no MCP handle
        let handle: Option<McpClientManagerHandle> = None;
        // WHEN building the resource executors
        let execs = build_mcp_resource_executors(&handle);
        // THEN none are produced (tools surface as absent, not failing)
        assert!(execs.is_empty());
    }
}
