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
//! An executor decides nothing about whether a call may run: that is the
//! decision of governance, taken on the action family of the tool before the
//! call reaches it.

use std::future::Future;
use std::pin::Pin;

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
/// Construct via [`McpToolExecutor::new`]. Use
/// [`McpToolExecutor::parse_tool_name`] to split a composite name before
/// calling the constructor.
///
/// [`ToolDispatcher`]: apollia_tools::executor::ToolDispatcher
pub struct McpToolExecutor {
    mcp_manager: McpClientManagerHandle,
    /// Full qualified name, e.g. `"mcp:notion/search_pages"`.
    full_name: String,
    server_name: String,
    tool_name: String,
}

impl McpToolExecutor {
    /// Create a new executor bound to `server_name` and `tool_name`.
    ///
    /// The resulting [`ToolExecutor::name`] will be `"mcp:{server_name}/{tool_name}"`.
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
        }
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
}

// ─── ToolExecutor impl ───────────────────────────────────────────────────────

impl ToolExecutor for McpToolExecutor {
    /// The fully-qualified MCP tool name: `"mcp:{server}/{tool}"`.
    fn name(&self) -> &str {
        &self.full_name
    }

    /// Execute the bound MCP tool with `input` as the argument payload.
    ///
    /// Routes the call through [`McpClientManagerHandle::call_tool`] and
    /// converts the [`ToolCallResult`] with [`tool_output`].
    ///
    /// # Errors
    ///
    /// - [`ToolExecutionError::ExecutionFailed`] when the MCP session returns an
    ///   error, or when the `ToolCallResult` itself carries `is_error = true`.
    ///
    /// [`ToolCallResult`]: crate::protocol::ToolCallResult
    fn execute(
        &self,
        input: Value,
    ) -> Pin<Box<dyn Future<Output = Result<Value, ToolExecutionError>> + Send + '_>> {
        Box::pin(async move {
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

            Ok(tool_output(&result))
        })
    }
}

/// What an agent receives from a successful MCP tool call.
///
/// `{"content": "…"}`, every text part joined with `"\n"`, and, when the server
/// sent a structured result, `"structured"` holding it as the server built it.
/// An agent reading an object used to parse the JSON text of `content` back;
/// `content` stays as it was, so an agent written against it is unchanged.
///
/// [`ToolCallResult`]: crate::protocol::ToolCallResult
pub(crate) fn tool_output(result: &crate::protocol::ToolCallResult) -> Value {
    let content = extract_text_parts(&result.content);
    match &result.structured_content {
        Some(structured) => serde_json::json!({ "content": content, "structured": structured }),
        None => serde_json::json!({ "content": content }),
    }
}

// ─── agent executor assembly ──────────────────────────────────────────────────

/// Build every agent-facing MCP [`ToolExecutor`] for the tools currently exposed
/// by `handle`.
///
/// Returns, in order: the two read-only resource executors, then one
/// [`McpToolExecutor`] per tool of each connected server. `server_detail` yields
/// the deferred tool index as well as eager tool lists, so the result is complete
/// in both [`LoadingMode`]s.
///
/// This is the single source of truth shared by the chat, desktop, and CLI agent
/// dispatchers. Callers append the returned executors to their [`ToolDispatcher`]
/// via `build_dispatcher_with`.
///
/// [`LoadingMode`]: crate::session::LoadingMode
/// [`ToolDispatcher`]: apollia_tools::executor::ToolDispatcher
pub async fn build_agent_tool_executors(
    handle: &McpClientManagerHandle,
) -> Vec<Box<dyn ToolExecutor>> {
    let mut execs: Vec<Box<dyn ToolExecutor>> =
        crate::mcp_resources::build_mcp_resource_executors(&Some(handle.clone()));
    for status in handle.status().await {
        // A dormant server's tools are served too: their call starts it.
        if !status.connected && !status.dormant {
            continue;
        }
        let Some(detail) = handle.server_detail(&status.name).await else {
            continue;
        };
        for tool in detail.tools {
            execs.push(Box::new(McpToolExecutor::new(
                handle.clone(),
                status.name.clone(),
                tool.local_name,
            )));
        }
    }
    execs
}

// ─── tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::ToolCallContent;

    #[test]
    fn a_structured_result_reaches_the_agent_as_an_object() {
        // GIVEN a result carrying structuredContent and its text serialization
        let result: crate::protocol::ToolCallResult = serde_json::from_value(serde_json::json!({
            "content": [{"type": "text", "text": "{\"lignes\": [{\"id\": \"d-1\"}]}"}],
            "structuredContent": {"lignes": [{"id": "d-1"}]}
        }))
        .unwrap();

        // WHEN it is turned into the agent's output
        let output = tool_output(&result);

        // THEN the object is there as the server built it, and the text is unchanged
        assert_eq!(output["structured"]["lignes"][0]["id"], "d-1");
        assert_eq!(output["content"], "{\"lignes\": [{\"id\": \"d-1\"}]}");
    }

    #[test]
    fn a_text_only_result_keeps_its_historical_shape() {
        // GIVEN a result with text parts and no structuredContent
        let result: crate::protocol::ToolCallResult = serde_json::from_value(serde_json::json!({
            "content": [{"type": "text", "text": "5"}]
        }))
        .unwrap();

        // WHEN it is turned into the agent's output
        let output = tool_output(&result);

        // THEN it is exactly `{"content": "5"}`
        assert_eq!(output, serde_json::json!({"content": "5"}));
    }

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

    #[tokio::test]
    async fn build_agent_tool_executors_no_servers_yields_only_resource_tools() {
        // GIVEN a manager with no connected servers
        use crate::session::LoadingMode;
        use apollia_tools::registry::ToolRegistryHandle;
        let registry = ToolRegistryHandle::start();
        let handle = McpClientManagerHandle::start(vec![], &registry, None, LoadingMode::Eager)
            .await
            .expect("manager start failed");

        // WHEN we assemble the agent executors
        let execs = build_agent_tool_executors(&handle).await;

        // THEN only the two read-only resource executors are present (no
        // per-server tools), and none carry a `mcp:` tool name.
        assert_eq!(
            execs.len(),
            2,
            "expected exactly the two resource executors"
        );
        assert!(
            execs.iter().all(|e| !e.name().starts_with("mcp:")),
            "resource executors are not per-server `mcp:` tools"
        );

        registry.shutdown().await;
    }
}
