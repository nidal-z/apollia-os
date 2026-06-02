//! MCP tool executor: bridges the [`ToolExecutor`] dispatch interface to MCP sessions.
//!
//! One [`McpToolExecutor`] instance is created per MCP tool discovered during session
//! initialisation. It carries the server and tool names so that [`ToolExecutor::execute`]
//! can route the call to the correct [`McpClientManagerHandle`] without re-parsing the
//! tool name at call time.
//!
//! The [`McpToolExecutor::parse_tool_name`] helper is provided for callers (e.g. the
//! Supervisor) that need to decompose a `"mcp:{server}/{tool}"` identifier into its
//! constituent parts before constructing an executor.
//!
//! ## HITL gate
//!
//! Call [`McpToolExecutor::with_hitl`] to enable the Human-in-the-Loop approval gate.
//! When enabled, [`ToolExecutor::execute`] suspends before routing to the MCP session
//! if either the server-level or agent-level approval flag is active.

use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};

use apollia_core::PendingApprovals;
use serde_json::Value;

use apollia_tools::executor::{ToolExecutionError, ToolExecutor};

use crate::manager::McpClientManagerHandle;
use crate::protocol::extract_text_parts;

// ─── public types ────────────────────────────────────────────────────────────

/// [`ToolExecutor`] implementation that routes calls to an MCP server.
///
/// Each instance is bound to a single `(server, tool)` pair. The full tool name
/// (in the `"mcp:{server}/{tool}"` format) is stored at construction time and
/// returned by [`ToolExecutor::name`], enabling exact-match routing in the
/// [`ToolDispatcher`].
///
/// Construct via [`McpToolExecutor::new`]. Chain [`McpToolExecutor::with_hitl`] to
/// enable the HITL approval gate. Use [`McpToolExecutor::parse_tool_name`] to split
/// a composite name before calling the constructor.
///
/// [`ToolDispatcher`]: apollia_tools::executor::ToolDispatcher
pub struct McpToolExecutor {
    mcp_manager: McpClientManagerHandle,
    /// Full qualified name, e.g. `"mcp:notion/search_pages"`.
    full_name: String,
    server_name: String,
    tool_name: String,
    /// HITL approval registry. `None` means the gate is disabled and all calls
    /// proceed immediately.
    pending_approvals: Option<Arc<PendingApprovals>>,
    /// Agent-level tools that require human approval before execution.
    ///
    /// Populated from `AgentManifest::tools_requiring_approval` at construction time
    /// via [`with_hitl`].
    ///
    /// [`with_hitl`]: McpToolExecutor::with_hitl
    tools_requiring_approval: Vec<String>,
    /// Monotonic counter used to generate a unique key per `execute()` invocation.
    ///
    /// Ensures concurrent calls to the same tool produce distinct approval keys in
    /// [`PendingApprovals`].
    call_counter: AtomicU64,
}

impl McpToolExecutor {
    /// Create a new executor bound to `server_name` and `tool_name`.
    ///
    /// The resulting [`ToolExecutor::name`] will be `"mcp:{server_name}/{tool_name}"`.
    /// HITL is disabled by default; call [`with_hitl`] to enable it.
    ///
    /// [`with_hitl`]: McpToolExecutor::with_hitl
    pub fn new(
        mcp_manager: McpClientManagerHandle,
        server_name: impl Into<String>,
        tool_name: impl Into<String>,
    ) -> Self {
        let server_name = server_name.into();
        let tool_name = tool_name.into();
        let full_name = format!("mcp:{}/{}", server_name, tool_name);
        Self {
            mcp_manager,
            full_name,
            server_name,
            tool_name,
            pending_approvals: None,
            tools_requiring_approval: Vec::new(),
            call_counter: AtomicU64::new(0),
        }
    }

    /// Enable the HITL approval gate for this executor.
    ///
    /// When enabled, [`execute`] will suspend before routing to the MCP session if
    /// the server-level flag (`requires_approval` in `mcp.toml`) or the agent-level
    /// list (`tools_requiring_approval` in the agent manifest) indicates that this
    /// tool requires human approval.
    ///
    /// `tools_requiring_approval` should be taken from `AgentManifest::tools_requiring_approval`
    /// and passed verbatim; the gate checks whether `self.full_name` appears in the list.
    ///
    /// [`execute`]: ToolExecutor::execute
    pub fn with_hitl(
        mut self,
        pending_approvals: Arc<PendingApprovals>,
        tools_requiring_approval: Vec<String>,
    ) -> Self {
        self.pending_approvals = Some(pending_approvals);
        self.tools_requiring_approval = tools_requiring_approval;
        self
    }

    /// Parse `"mcp:{server}/{tool}"` into `(server_name, tool_name)`.
    ///
    /// Returns `None` when the input does not carry the `"mcp:"` prefix, contains
    /// no `'/'` separator, or has an empty server or tool segment.
    pub fn parse_tool_name(name: &str) -> Option<(&str, &str)> {
        let stripped = name.strip_prefix("mcp:")?;
        let slash = stripped.find('/')?;
        let server = &stripped[..slash];
        let tool = &stripped[slash + 1..];
        if server.is_empty() || tool.is_empty() {
            return None;
        }
        Some((server, tool))
    }

    /// Check if HITL approval is required and suspend execution until resolved.
    ///
    /// Two gates are evaluated:
    /// 1. **Server-level**: the MCP server has `requires_approval = true` in `mcp.toml`.
    /// 2. **Agent-level**: the tool's full name appears in `tools_requiring_approval`.
    ///
    /// If either fires, a unique key is registered in `pending_approvals` and the call
    /// blocks until a human resolves it. A rejection returns
    /// [`ToolExecutionError::ExecutionFailed`] with code `"hitl_rejected"`.
    async fn check_hitl_gate(&self, pending: &PendingApprovals) -> Result<(), ToolExecutionError> {
        let server_requires = self
            .mcp_manager
            .server_requires_approval(&self.server_name)
            .await;
        let agent_requires = self.tools_requiring_approval.contains(&self.full_name);

        if !server_requires && !agent_requires {
            return Ok(());
        }

        let call_id = self.call_counter.fetch_add(1, Ordering::Relaxed);
        let approval_key = format!("{}:{}", self.full_name, call_id);

        let reason = if server_requires {
            format!(
                "MCP server '{}' requires approval for all tools",
                self.server_name
            )
        } else {
            format!("Agent requires approval for tool '{}'", self.full_name)
        };

        tracing::info!(
            tool = %self.full_name,
            approval_key = %approval_key,
            %reason,
            "MCP tool execution suspended pending human approval"
        );

        let rx = pending.register(&approval_key);
        let response = rx.await.map_err(|_| ToolExecutionError::ExecutionFailed {
            code: "hitl_channel_closed".to_string(),
            message: format!("HITL approval channel closed for tool '{}'", self.full_name),
        })?;

        if response.approved {
            tracing::info!(
                tool = %self.full_name,
                approval_key = %approval_key,
                "MCP tool execution approved by user"
            );
            Ok(())
        } else {
            let reject_reason = response.reason.unwrap_or_default();
            Err(ToolExecutionError::ExecutionFailed {
                code: "hitl_rejected".to_string(),
                message: format!("HITL approval rejected: {}", reject_reason),
            })
        }
    }
}

// ─── ToolExecutor impl ───────────────────────────────────────────────────────

#[async_trait::async_trait]
impl ToolExecutor for McpToolExecutor {
    /// The fully-qualified MCP tool name: `"mcp:{server}/{tool}"`.
    fn name(&self) -> &str {
        &self.full_name
    }

    /// Execute the bound MCP tool with `input` as the argument payload.
    ///
    /// When a HITL gate is configured (via [`with_hitl`]), the call suspends before
    /// routing to the MCP session until a human approves or rejects it.
    ///
    /// Routes the call through [`McpClientManagerHandle::call_tool`] and
    /// converts the [`ToolCallResult`] into a JSON object of the form
    /// `{"content": "…"}` where the value is all text parts joined with `"\n"`.
    ///
    /// # Errors
    ///
    /// - [`ToolExecutionError::ExecutionFailed`] with code `"hitl_rejected"` when the
    ///   HITL gate is active and the human rejects the approval request.
    /// - [`ToolExecutionError::ExecutionFailed`] with code `"hitl_channel_closed"` when
    ///   the approval channel is dropped before a decision is received.
    /// - [`ToolExecutionError::ExecutionFailed`] when the MCP session returns an
    ///   error, or when the `ToolCallResult` itself carries `is_error = true`.
    ///
    /// [`with_hitl`]: McpToolExecutor::with_hitl
    /// [`ToolCallResult`]: crate::protocol::ToolCallResult
    async fn execute(&self, input: Value) -> Result<Value, ToolExecutionError> {
        if let Some(pending) = &self.pending_approvals {
            self.check_hitl_gate(pending).await?;
        }

        let result = self
            .mcp_manager
            .call_tool(&self.server_name, &self.tool_name, Some(input))
            .await
            .map_err(|e| ToolExecutionError::ExecutionFailed {
                code: "mcp_session_error".to_string(),
                message: e.to_string(),
            })?;

        if result.is_error.unwrap_or(false) {
            let error_text = extract_text_parts(&result.content);
            return Err(ToolExecutionError::ExecutionFailed {
                code: "mcp_tool_error".to_string(),
                message: error_text,
            });
        }

        let content = extract_text_parts(&result.content);
        Ok(serde_json::json!({ "content": content }))
    }
}

// ─── tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::ToolCallContent;

    #[test]
    fn parse_tool_name_valid() {
        // GIVEN
        let name = "mcp:notion/search_pages";
        // WHEN
        let result = McpToolExecutor::parse_tool_name(name);
        // THEN
        assert_eq!(result, Some(("notion", "search_pages")));
    }

    #[test]
    fn parse_tool_name_with_hyphen() {
        // GIVEN
        let name = "mcp:brave-search/web_search";
        // WHEN
        let result = McpToolExecutor::parse_tool_name(name);
        // THEN
        assert_eq!(result, Some(("brave-search", "web_search")));
    }

    #[test]
    fn parse_tool_name_no_prefix() {
        // GIVEN
        let name = "bash_executor";
        // WHEN
        let result = McpToolExecutor::parse_tool_name(name);
        // THEN
        assert_eq!(result, None);
    }

    #[test]
    fn parse_tool_name_no_slash() {
        // GIVEN
        let name = "mcp:notion";
        // WHEN
        let result = McpToolExecutor::parse_tool_name(name);
        // THEN
        assert_eq!(result, None);
    }

    #[test]
    fn parse_tool_name_empty_server() {
        // GIVEN
        let name = "mcp:/search";
        // WHEN
        let result = McpToolExecutor::parse_tool_name(name);
        // THEN
        assert_eq!(result, None);
    }

    #[test]
    fn parse_tool_name_empty_tool() {
        // GIVEN
        let name = "mcp:notion/";
        // WHEN
        let result = McpToolExecutor::parse_tool_name(name);
        // THEN
        assert_eq!(result, None);
    }

    #[test]
    fn executor_name_follows_convention() {
        // GIVEN the server "notion" and tool "search_pages"
        // We can't construct a real McpToolExecutor without a live manager, but
        // the full_name computation is verified via parse_tool_name round-trip.
        let server = "notion";
        let tool = "search_pages";
        let full = format!("mcp:{}/{}", server, tool);
        // WHEN parsed back
        let parsed = McpToolExecutor::parse_tool_name(&full);
        // THEN round-trips correctly
        assert_eq!(parsed, Some((server, tool)));
    }

    #[test]
    fn extract_text_parts_single() {
        // GIVEN
        let content = vec![ToolCallContent::Text {
            text: "hello world".to_string(),
        }];
        // WHEN
        let result = extract_text_parts(&content);
        // THEN
        assert_eq!(result, "hello world");
    }

    #[test]
    fn extract_text_parts_multiple_joined_with_newline() {
        // GIVEN
        let content = vec![
            ToolCallContent::Text {
                text: "line one".to_string(),
            },
            ToolCallContent::Text {
                text: "line two".to_string(),
            },
        ];
        // WHEN
        let result = extract_text_parts(&content);
        // THEN
        assert_eq!(result, "line one\nline two");
    }

    #[test]
    fn extract_text_parts_skips_non_text() {
        // GIVEN a mix of text and image content
        let content = vec![
            ToolCallContent::Text {
                text: "text only".to_string(),
            },
            ToolCallContent::Image {
                data: "base64".to_string(),
                mime_type: "image/png".to_string(),
            },
        ];
        // WHEN
        let result = extract_text_parts(&content);
        // THEN only text is included
        assert_eq!(result, "text only");
    }

    #[test]
    fn extract_text_parts_empty() {
        // GIVEN an empty content slice
        let content: Vec<ToolCallContent> = vec![];
        // WHEN
        let result = extract_text_parts(&content);
        // THEN
        assert_eq!(result, "");
    }

    #[test]
    fn server_requires_approval_config_flag_readable() {
        // GIVEN a server config with requires_approval=true
        let config = crate::config::McpServerConfig {
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
        // WHEN / THEN the flag is set
        assert!(config.requires_approval);
    }

    #[test]
    fn agent_level_tools_requiring_approval_lookup() {
        // GIVEN a list of tools that require approval
        let tools_requiring: Vec<String> = vec![
            "mcp:notion/create_page".to_string(),
            "mcp:notion/delete_page".to_string(),
        ];
        // WHEN searching for a specific tool
        let requires = tools_requiring.contains(&"mcp:notion/create_page".to_string());
        // THEN it is found
        assert!(requires);
    }

    #[test]
    fn agent_level_tools_requiring_approval_absent_tool() {
        // GIVEN a list of tools that require approval
        let tools_requiring: Vec<String> = vec!["mcp:notion/create_page".to_string()];
        // WHEN searching for a tool not in the list
        let requires = tools_requiring.contains(&"mcp:notion/search_pages".to_string());
        // THEN it is not found
        assert!(!requires);
    }

    #[test]
    fn hitl_rejection_error_format() {
        // GIVEN a rejected approval
        let error = ToolExecutionError::ExecutionFailed {
            code: "hitl_rejected".to_string(),
            message: "HITL approval rejected: user declined".to_string(),
        };
        // THEN the display output is descriptive
        assert!(error.to_string().contains("HITL approval rejected"));
        assert!(error.to_string().contains("user declined"));
    }

    #[tokio::test]
    async fn hitl_gate_approve_unblocks_execution() {
        // GIVEN a PendingApprovals registry
        use apollia_core::{InputResponseData, PendingApprovals};
        use std::sync::Arc;

        let pending = Arc::new(PendingApprovals::new());
        let approval_key = "mcp:notion/create_page:0";

        // Register the approval before the gate fires
        let rx = pending.register(approval_key);

        // WHEN the gate is resolved as approved
        pending
            .resolve(
                approval_key,
                InputResponseData {
                    approved: true,
                    reason: None,
                    context: serde_json::Value::Null,
                    responded_at: "2026-01-01T00:00:00Z".to_string(),
                },
            )
            .expect("resolve must succeed");

        // THEN the receiver yields approved=true
        let response = rx.await.expect("channel must not close");
        assert!(response.approved);
    }

    #[tokio::test]
    async fn hitl_gate_reject_yields_reason() {
        // GIVEN a PendingApprovals registry
        use apollia_core::{InputResponseData, PendingApprovals};
        use std::sync::Arc;

        let pending = Arc::new(PendingApprovals::new());
        let approval_key = "mcp:notion/delete_page:0";

        let rx = pending.register(approval_key);

        // WHEN the gate is resolved as rejected with a reason
        pending
            .resolve(
                approval_key,
                InputResponseData {
                    approved: false,
                    reason: Some("Too destructive".to_string()),
                    context: serde_json::Value::Null,
                    responded_at: "2026-01-01T00:00:00Z".to_string(),
                },
            )
            .expect("resolve must succeed");

        // THEN the receiver yields approved=false with the reason preserved
        let response = rx.await.expect("channel must not close");
        assert!(!response.approved);
        assert_eq!(response.reason.as_deref(), Some("Too destructive"));
    }
}
