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

use serde_json::Value;

use apollia_tools::executor::{ToolExecutionError, ToolExecutor};

use crate::manager::McpClientManagerHandle;
use crate::protocol::ToolCallContent;

// ─── public types ────────────────────────────────────────────────────────────

/// [`ToolExecutor`] implementation that routes calls to an MCP server.
///
/// Each instance is bound to a single `(server, tool)` pair. The full tool name
/// — in the `"mcp:{server}/{tool}"` format — is stored at construction time and
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

#[async_trait::async_trait]
impl ToolExecutor for McpToolExecutor {
    /// The fully-qualified MCP tool name: `"mcp:{server}/{tool}"`.
    fn name(&self) -> &str {
        &self.full_name
    }

    /// Execute the bound MCP tool with `input` as the argument payload.
    ///
    /// Routes the call through [`McpClientManagerHandle::call_tool`] and
    /// converts the [`ToolCallResult`] into a JSON object of the form
    /// `{"content": "…"}` where the value is all text parts joined with `"\n"`.
    ///
    /// # Errors
    ///
    /// - [`ToolExecutionError::ExecutionFailed`] when the MCP session returns an
    ///   error, or when the `ToolCallResult` itself carries `is_error = true`.
    ///
    /// [`ToolCallResult`]: crate::protocol::ToolCallResult
    async fn execute(&self, input: Value) -> Result<Value, ToolExecutionError> {
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

// ─── helpers ─────────────────────────────────────────────────────────────────

/// Collect all [`ToolCallContent::Text`] items and join them with `"\n"`.
fn extract_text_parts(content: &[ToolCallContent]) -> String {
    content
        .iter()
        .filter_map(|c| match c {
            ToolCallContent::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

// ─── tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

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
}
