//! MCP protocol types for the Model Context Protocol (spec 2024-11-05).
//!
//! These types represent the `params` and `result` fields of JSON-RPC messages
//! exchanged with MCP servers: `initialize`, `tools/list`, and `tools/call`.

use serde::{Deserialize, Serialize};

/// Parameters for the MCP `initialize` request.
#[derive(Debug, Serialize)]
pub struct InitializeParams {
    /// Protocol version the client implements (e.g. `"2024-11-05"`).
    #[serde(rename = "protocolVersion")]
    pub protocol_version: String,
    /// Capabilities the client advertises to the server.
    pub capabilities: ClientCapabilities,
    /// Identity of the connecting client.
    #[serde(rename = "clientInfo")]
    pub client_info: ClientInfo,
}

/// Client capability advertisement sent during `initialize`.
///
/// Empty in protocol V1; reserved for future extensions.
#[derive(Debug, Serialize)]
pub struct ClientCapabilities {
    // V1: no special capabilities declared
}

/// Identity block sent by the client during `initialize`.
#[derive(Debug, Serialize)]
pub struct ClientInfo {
    /// Human-readable client name (e.g. `"apollia-runtime"`).
    pub name: String,
    /// Client version string (e.g. `"0.1.0"`).
    pub version: String,
}

/// Result returned by the server for the `initialize` request.
#[derive(Debug, Deserialize)]
pub struct InitializeResult {
    /// Protocol version the server implements.
    #[serde(rename = "protocolVersion")]
    pub protocol_version: String,
    /// Capabilities advertised by the server.
    pub capabilities: ServerCapabilities,
    /// Identity of the connected server.
    #[serde(rename = "serverInfo")]
    pub server_info: ServerInfo,
}

/// Capabilities advertised by the server in `initialize` response.
#[derive(Debug, Deserialize)]
pub struct ServerCapabilities {
    /// Tool-related capabilities; `None` if the server exposes no tools.
    pub tools: Option<ToolsCapability>,
    /// Resource-related capabilities (opaque in V1).
    pub resources: Option<serde_json::Value>,
    /// Prompt-related capabilities (opaque in V1).
    pub prompts: Option<serde_json::Value>,
}

/// Tool-specific capability flags from the server.
#[derive(Debug, Deserialize)]
pub struct ToolsCapability {
    /// Whether the server can notify the client when the tool list changes.
    #[serde(rename = "listChanged")]
    pub list_changed: Option<bool>,
}

/// Identity block returned by the server in `initialize` response.
#[derive(Debug, Deserialize)]
pub struct ServerInfo {
    /// Human-readable server name.
    pub name: String,
    /// Server version string; may be absent.
    pub version: Option<String>,
}

/// A single tool definition from a `tools/list` response.
#[derive(Debug, Deserialize)]
pub struct McpToolDefinition {
    /// Unique tool name within the server (e.g. `"search_pages"`).
    pub name: String,
    /// Human-readable description of what the tool does.
    pub description: Option<String>,
    /// JSON Schema describing the tool's input arguments.
    #[serde(rename = "inputSchema")]
    pub input_schema: serde_json::Value,
}

/// Result of a `tools/list` request.
#[derive(Debug, Deserialize)]
pub struct ToolsListResult {
    /// All tools exposed by the server.
    pub tools: Vec<McpToolDefinition>,
}

/// Parameters for the `tools/call` request.
#[derive(Debug, Serialize)]
pub struct ToolCallParams {
    /// Name of the tool to invoke.
    pub name: String,
    /// Input arguments as a JSON object, or `None` for argument-less tools.
    pub arguments: Option<serde_json::Value>,
}

/// Result of a `tools/call` request.
#[derive(Debug, Deserialize)]
pub struct ToolCallResult {
    /// Ordered list of content items produced by the tool.
    pub content: Vec<ToolCallContent>,
    /// `true` if the tool itself reported an error condition.
    #[serde(rename = "isError")]
    pub is_error: Option<bool>,
}

/// A single content item in a `tools/call` result.
///
/// Discriminated by the `"type"` field in the JSON representation.
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ToolCallContent {
    /// Plain-text output.
    Text {
        /// The text produced by the tool.
        text: String,
    },
    /// Base-64-encoded binary image.
    Image {
        /// Base-64-encoded image data.
        data: String,
        /// MIME type of the image (e.g. `"image/png"`).
        #[serde(rename = "mimeType")]
        mime_type: String,
    },
    /// An embedded resource reference.
    Resource {
        /// Resource descriptor (opaque in V1).
        resource: serde_json::Value,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_initialize_params_serialization() {
        // GIVEN
        let params = InitializeParams {
            protocol_version: "2024-11-05".to_string(),
            capabilities: ClientCapabilities {},
            client_info: ClientInfo {
                name: "apollia-runtime".to_string(),
                version: "0.1.0".to_string(),
            },
        };
        // WHEN
        let value = serde_json::to_value(&params).unwrap();
        // THEN
        assert_eq!(value["protocolVersion"], "2024-11-05");
        assert!(value.get("clientInfo").is_some());
        assert_eq!(value["clientInfo"]["name"], "apollia-runtime");
    }

    #[test]
    fn test_tools_list_result_deserialization() {
        // GIVEN
        let json_str = r#"{
            "tools": [
                {"name": "search", "description": "Search pages", "inputSchema": {"type": "object"}},
                {"name": "create", "description": "Create page", "inputSchema": {"type": "object"}},
                {"name": "update", "inputSchema": {"type": "object"}}
            ]
        }"#;
        // WHEN
        let result: ToolsListResult = serde_json::from_str(json_str).unwrap();
        // THEN
        assert_eq!(result.tools.len(), 3);
        assert_eq!(result.tools[0].name, "search");
        assert_eq!(
            result.tools[0].description,
            Some("Search pages".to_string())
        );
        assert!(result.tools[2].description.is_none());
    }

    #[test]
    fn test_tool_call_result_text_content() {
        // GIVEN
        let json_str = r#"{
            "content": [{"type": "text", "text": "hello"}],
            "isError": false
        }"#;
        // WHEN
        let result: ToolCallResult = serde_json::from_str(json_str).unwrap();
        // THEN
        assert_eq!(result.content.len(), 1);
        assert!(matches!(&result.content[0], ToolCallContent::Text { text } if text == "hello"));
        assert_eq!(result.is_error, Some(false));
    }

    #[test]
    fn test_tool_call_content_image() {
        // GIVEN
        let json_str = r#"{"type": "image", "data": "base64data", "mimeType": "image/png"}"#;
        // WHEN
        let content: ToolCallContent = serde_json::from_str(json_str).unwrap();
        // THEN
        assert!(
            matches!(content, ToolCallContent::Image { ref data, ref mime_type }
                if data == "base64data" && mime_type == "image/png"
            )
        );
    }

    #[test]
    fn test_tool_call_content_resource() {
        // GIVEN
        let json_str = r#"{"type": "resource", "resource": {"uri": "file:///test"}}"#;
        // WHEN
        let content: ToolCallContent = serde_json::from_str(json_str).unwrap();
        // THEN
        assert!(matches!(content, ToolCallContent::Resource { .. }));
    }

    #[test]
    fn test_initialize_result_deserialization() {
        // GIVEN
        let json_str = r#"{
            "protocolVersion": "2024-11-05",
            "capabilities": {"tools": {"listChanged": true}},
            "serverInfo": {"name": "test-server", "version": "1.0.0"}
        }"#;
        // WHEN
        let result: InitializeResult = serde_json::from_str(json_str).unwrap();
        // THEN
        assert_eq!(result.protocol_version, "2024-11-05");
        assert_eq!(result.server_info.name, "test-server");
        assert!(result.capabilities.tools.is_some());
    }

    #[test]
    fn test_tool_call_params_serialization() {
        // GIVEN
        let params = ToolCallParams {
            name: "search_pages".to_string(),
            arguments: Some(json!({"query": "test"})),
        };
        // WHEN
        let value = serde_json::to_value(&params).unwrap();
        // THEN
        assert_eq!(value["name"], "search_pages");
        assert_eq!(value["arguments"]["query"], "test");
    }

    #[test]
    fn test_server_capabilities_no_tools() {
        // GIVEN: a server that advertises no tool capability
        let json_str = r#"{
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "serverInfo": {"name": "minimal"}
        }"#;
        // WHEN
        let result: InitializeResult = serde_json::from_str(json_str).unwrap();
        // THEN
        assert!(result.capabilities.tools.is_none());
        assert!(result.server_info.version.is_none());
    }

    #[test]
    fn test_tool_call_params_no_arguments() {
        // GIVEN
        let params = ToolCallParams {
            name: "ping".to_string(),
            arguments: None,
        };
        // WHEN
        let value = serde_json::to_value(&params).unwrap();
        // THEN
        assert_eq!(value["name"], "ping");
        assert!(value["arguments"].is_null());
    }
}
