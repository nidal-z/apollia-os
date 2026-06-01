//! MCP protocol types for the Model Context Protocol.
//!
//! Pinned to revision **2025-11-25**.
//! Implements the JSON-RPC payloads for the capabilities Apollia v0.1.0 cares
//! about: tools (existing), resources, prompts, logging, plus the client-side
//! capabilities (roots, sampling, elicitation) and the progress / cancellation
//! notifications.
//!
//! Earlier revisions of this file targeted spec `2024-11-05`. Type names stay
//! stable so external callers do not break; new fields are additive
//! `Option<…>` or `#[serde(default)]` so existing payloads keep deserializing.

use serde::{Deserialize, Serialize};

/// MCP protocol revision Apollia targets (pinned per `MCP-SPEC-PIN.md`).
pub const APOLLIA_MCP_PROTOCOL_VERSION: &str = "2025-11-25";

/// Parameters for the MCP `initialize` request.
#[derive(Debug, Serialize)]
pub struct InitializeParams {
    /// Protocol version the client implements (currently `"2025-11-25"`).
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
/// All fields are optional; an absent field means "Apollia does not implement
/// this capability". Apollia v0.1.0 advertises `roots`, `sampling`, and
/// `elicitation`.
#[derive(Debug, Default, Serialize)]
pub struct ClientCapabilities {
    /// Filesystem / resource roots exposed to the server.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub roots: Option<RootsCapability>,
    /// Server-initiated sampling (LLM completion) requests.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sampling: Option<SamplingCapability>,
    /// Server-initiated user elicitation requests.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub elicitation: Option<ElicitationCapability>,
}

/// Roots capability advertisement.
#[derive(Debug, Default, Serialize)]
pub struct RootsCapability {
    /// Whether the client emits `notifications/roots/list_changed`.
    #[serde(rename = "listChanged", skip_serializing_if = "Option::is_none")]
    pub list_changed: Option<bool>,
}

/// Sampling capability advertisement (presence = enabled).
#[derive(Debug, Default, Serialize)]
pub struct SamplingCapability {}

/// Elicitation capability advertisement (presence = enabled).
#[derive(Debug, Default, Serialize)]
pub struct ElicitationCapability {}

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
#[derive(Debug, Default, Deserialize)]
pub struct ServerCapabilities {
    /// Tool-related capabilities; `None` if the server exposes no tools.
    #[serde(default)]
    pub tools: Option<ToolsCapability>,
    /// Resource-related capabilities; `None` if the server exposes no resources.
    #[serde(default)]
    pub resources: Option<ResourcesCapability>,
    /// Prompt-related capabilities; `None` if the server exposes no prompts.
    #[serde(default)]
    pub prompts: Option<PromptsCapability>,
    /// Logging capability: when present, the client can call `logging/setLevel`.
    #[serde(default)]
    pub logging: Option<serde_json::Value>,
    /// Completions capability for argument autocompletion (post-v0.1.0).
    #[serde(default)]
    pub completions: Option<serde_json::Value>,
}

/// Tool-specific capability flags from the server.
#[derive(Debug, Default, Deserialize)]
pub struct ToolsCapability {
    /// Whether the server can notify the client when the tool list changes.
    #[serde(rename = "listChanged", default)]
    pub list_changed: Option<bool>,
}

/// Resources capability flags from the server.
#[derive(Debug, Default, Deserialize)]
pub struct ResourcesCapability {
    /// Whether the server supports `resources/subscribe` + `notifications/resources/updated`.
    #[serde(default)]
    pub subscribe: Option<bool>,
    /// Whether the server emits `notifications/resources/list_changed`.
    #[serde(rename = "listChanged", default)]
    pub list_changed: Option<bool>,
}

/// Prompts capability flags from the server.
#[derive(Debug, Default, Deserialize)]
pub struct PromptsCapability {
    /// Whether the server emits `notifications/prompts/list_changed`.
    #[serde(rename = "listChanged", default)]
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

// ─── Resources (server capability) ───────────────────────────────────────────

/// A resource exposed by an MCP server.
///
/// Resources are addressable by URI (file://, https://, custom schemes). The
/// agent ReAct loop reads them via the implicit `mcp_resources.read` tool;
/// users can also pin them through the desktop @-mention picker.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct McpResource {
    /// Stable URI identifying the resource.
    pub uri: String,
    /// Display name for the UI.
    pub name: String,
    /// Optional one-line description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// MIME type when known (e.g. `"text/plain"`).
    #[serde(rename = "mimeType", default, skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
}

/// Result of `resources/list`.
#[derive(Debug, Deserialize)]
pub struct ResourcesListResult {
    /// Listed resources.
    pub resources: Vec<McpResource>,
    /// Pagination cursor for the next page (absent on the last page).
    #[serde(rename = "nextCursor", default)]
    pub next_cursor: Option<String>,
}

/// Parameters for `resources/read`.
#[derive(Debug, Serialize)]
pub struct ResourcesReadParams {
    /// Resource URI to read.
    pub uri: String,
}

/// Single resource content payload.
#[derive(Debug, Deserialize)]
pub struct ResourceContent {
    /// Resource URI.
    pub uri: String,
    /// MIME type when known.
    #[serde(rename = "mimeType", default)]
    pub mime_type: Option<String>,
    /// Plain text content (when the resource is textual).
    #[serde(default)]
    pub text: Option<String>,
    /// Base64-encoded blob (when the resource is binary).
    #[serde(default)]
    pub blob: Option<String>,
}

/// Result of `resources/read`.
#[derive(Debug, Deserialize)]
pub struct ResourcesReadResult {
    /// All content variants the resource exposes.
    pub contents: Vec<ResourceContent>,
}

// ─── Prompts (server capability) ─────────────────────────────────────────────

/// A prompt template exposed by an MCP server.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct McpPrompt {
    /// Stable prompt name (e.g. `"summarize-page"`).
    pub name: String,
    /// One-line description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Optional argument declarations.
    #[serde(default)]
    pub arguments: Vec<PromptArgument>,
}

/// Argument declaration for a prompt template.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PromptArgument {
    /// Argument name.
    pub name: String,
    /// Optional description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Whether the argument is required.
    #[serde(default)]
    pub required: bool,
}

/// Result of `prompts/list`.
#[derive(Debug, Deserialize)]
pub struct PromptsListResult {
    /// Available prompts.
    pub prompts: Vec<McpPrompt>,
}

/// Parameters for `prompts/get`.
#[derive(Debug, Serialize)]
pub struct PromptsGetParams {
    /// Prompt name to retrieve.
    pub name: String,
    /// Arguments to fill in (server-defined shape).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arguments: Option<serde_json::Value>,
}

/// Result of `prompts/get`.
#[derive(Debug, Deserialize)]
pub struct PromptsGetResult {
    /// Optional human-readable description of the assembled prompt.
    #[serde(default)]
    pub description: Option<String>,
    /// Messages composing the prompt, fed to the conversation as a system prefix.
    pub messages: Vec<PromptMessage>,
}

/// Single message in a `prompts/get` response.
#[derive(Debug, Deserialize)]
pub struct PromptMessage {
    /// `"user"` or `"assistant"` (kept opaque for v0.1.0).
    pub role: String,
    /// Message content (matches the `ToolCallContent` discriminator).
    pub content: serde_json::Value,
}

// ─── Sampling (server → client request) ──────────────────────────────────────

/// Parameters for `sampling/createMessage` (server → client request).
///
/// Apollia routes this through `apollia_llm::LlmRouter` after HITL pre-approval.
/// Rate-limiting and budget enforcement live in the handler.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SamplingCreateMessageParams {
    /// Conversation context the server wants the LLM to complete.
    pub messages: Vec<serde_json::Value>,
    /// Optional model preferences (priority hints).
    #[serde(
        rename = "modelPreferences",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub model_preferences: Option<serde_json::Value>,
    /// Optional system prompt.
    #[serde(
        rename = "systemPrompt",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub system_prompt: Option<String>,
    /// Optional context inclusion policy (`none`, `thisServer`, `allServers`).
    #[serde(
        rename = "includeContext",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub include_context: Option<String>,
    /// Maximum tokens to generate.
    #[serde(rename = "maxTokens", default, skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    /// Temperature.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
}

/// Result the client returns to the server for `sampling/createMessage`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SamplingCreateMessageResult {
    /// Role of the generated message (typically `"assistant"`).
    pub role: String,
    /// Generated content payload (text item per spec).
    pub content: serde_json::Value,
    /// Identifier of the model that produced the result.
    pub model: String,
    /// Optional reason the model stopped (`endTurn`, `stopSequence`, `maxTokens`, …).
    #[serde(
        rename = "stopReason",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub stop_reason: Option<String>,
}

// ─── Elicitation (server → client request) ───────────────────────────────────

/// Parameters for `elicitation/create` (server → client request).
///
/// Apollia routes this through the existing `chat.user_input_required` inbox
/// pipeline; `AskUserForm` consumes the JSON Schema directly. No new UI
/// component required.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ElicitationCreateParams {
    /// Human-readable message shown to the user above the form.
    pub message: String,
    /// JSON Schema (2020-12) describing the expected response shape.
    #[serde(rename = "requestedSchema")]
    pub requested_schema: serde_json::Value,
}

/// Result the client returns to the server for `elicitation/create`.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "action", rename_all = "camelCase")]
pub enum ElicitationCreateResult {
    /// User submitted a response matching the schema.
    Accept {
        /// User-provided content matching `requested_schema`.
        content: serde_json::Value,
    },
    /// User explicitly declined.
    Decline,
    /// User cancelled (closed the form, timeout, …).
    Cancel,
}

// ─── Roots (server → client request) ─────────────────────────────────────────

/// A filesystem / URI root the client exposes to the server.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct McpRoot {
    /// Root URI (e.g. `"file:///home/user/projects/apollia"`).
    pub uri: String,
    /// Optional display name shown to the user.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// Result returned for `roots/list` (server → client request).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RootsListResult {
    /// Roots the client authorises this server to traverse.
    pub roots: Vec<McpRoot>,
}

// ─── Progress + cancellation notifications ───────────────────────────────────

/// Payload of `notifications/progress`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ProgressNotification {
    /// Progress token attached when the original request was sent.
    #[serde(rename = "progressToken")]
    pub progress_token: serde_json::Value,
    /// Current progress value.
    pub progress: f64,
    /// Optional total against which `progress` should be interpreted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total: Option<f64>,
    /// Optional human-readable update.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// Payload of `notifications/cancelled`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CancelNotification {
    /// JSON-RPC request id that should be cancelled.
    #[serde(rename = "requestId")]
    pub request_id: serde_json::Value,
    /// Optional human-readable reason.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_initialize_params_serialization() {
        // GIVEN
        let params = InitializeParams {
            protocol_version: APOLLIA_MCP_PROTOCOL_VERSION.to_string(),
            capabilities: ClientCapabilities::default(),
            client_info: ClientInfo {
                name: "apollia-runtime".to_string(),
                version: "0.1.0".to_string(),
            },
        };
        // WHEN
        let value = serde_json::to_value(&params).unwrap();
        // THEN
        assert_eq!(value["protocolVersion"], APOLLIA_MCP_PROTOCOL_VERSION);
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
    fn test_client_capabilities_advertises_roots_sampling_elicitation() {
        // GIVEN apollia's default capability set
        let caps = ClientCapabilities {
            roots: Some(RootsCapability {
                list_changed: Some(true),
            }),
            sampling: Some(SamplingCapability::default()),
            elicitation: Some(ElicitationCapability::default()),
        };
        // WHEN serialized
        let value = serde_json::to_value(&caps).unwrap();
        // THEN all three keys are present in the wire payload
        assert!(value.get("roots").is_some());
        assert!(value.get("sampling").is_some());
        assert!(value.get("elicitation").is_some());
        assert_eq!(value["roots"]["listChanged"], true);
    }

    #[test]
    fn test_client_capabilities_default_omits_capabilities() {
        // GIVEN the default (no capability)
        let caps = ClientCapabilities::default();
        // WHEN serialized
        let value = serde_json::to_value(&caps).unwrap();
        // THEN unset capabilities are absent from the wire payload
        assert!(value.get("roots").is_none());
        assert!(value.get("sampling").is_none());
        assert!(value.get("elicitation").is_none());
    }

    #[test]
    fn test_server_capabilities_deserializes_resources_and_prompts_typed() {
        // GIVEN a server response advertising resources + prompts capabilities
        let json_str = r#"{
            "protocolVersion": "2025-11-25",
            "capabilities": {
                "resources": { "subscribe": true, "listChanged": true },
                "prompts": { "listChanged": true }
            },
            "serverInfo": { "name": "test" }
        }"#;
        // WHEN deserialized
        let result: InitializeResult = serde_json::from_str(json_str).unwrap();
        // THEN the typed fields are populated
        let resources = result.capabilities.resources.expect("resources");
        assert_eq!(resources.subscribe, Some(true));
        let prompts = result.capabilities.prompts.expect("prompts");
        assert_eq!(prompts.list_changed, Some(true));
    }

    #[test]
    fn test_resources_list_result_deserializes() {
        let json_str = r#"{
            "resources": [
                { "uri": "file:///doc.txt", "name": "doc", "mimeType": "text/plain" }
            ],
            "nextCursor": "page-2"
        }"#;
        let result: ResourcesListResult = serde_json::from_str(json_str).unwrap();
        assert_eq!(result.resources.len(), 1);
        assert_eq!(result.resources[0].uri, "file:///doc.txt");
        assert_eq!(result.next_cursor.as_deref(), Some("page-2"));
    }

    #[test]
    fn test_prompts_list_result_deserializes() {
        let json_str = r#"{
            "prompts": [
                { "name": "summarize", "description": "Summarize a doc", "arguments": [{"name":"uri","required":true}] }
            ]
        }"#;
        let result: PromptsListResult = serde_json::from_str(json_str).unwrap();
        assert_eq!(result.prompts.len(), 1);
        assert_eq!(result.prompts[0].name, "summarize");
        assert!(result.prompts[0].arguments[0].required);
    }

    #[test]
    fn test_elicitation_result_accept_serializes_with_action_tag() {
        let r = ElicitationCreateResult::Accept {
            content: serde_json::json!({ "answer": "yes" }),
        };
        let value = serde_json::to_value(&r).unwrap();
        assert_eq!(value["action"], "accept");
        assert_eq!(value["content"]["answer"], "yes");
    }

    #[test]
    fn test_elicitation_result_decline_serializes_action_only() {
        let r = ElicitationCreateResult::Decline;
        let value = serde_json::to_value(&r).unwrap();
        assert_eq!(value["action"], "decline");
    }

    #[test]
    fn test_roots_list_result_round_trips() {
        let roots = RootsListResult {
            roots: vec![McpRoot {
                uri: "file:///workspace".into(),
                name: Some("workspace".into()),
            }],
        };
        let value = serde_json::to_value(&roots).unwrap();
        assert_eq!(value["roots"][0]["uri"], "file:///workspace");
        let parsed: RootsListResult = serde_json::from_value(value).unwrap();
        assert_eq!(parsed.roots.len(), 1);
    }

    #[test]
    fn test_progress_notification_deserializes() {
        let json_str = r#"{
            "progressToken": "tok-1",
            "progress": 0.5,
            "total": 1.0,
            "message": "halfway"
        }"#;
        let p: ProgressNotification = serde_json::from_str(json_str).unwrap();
        assert_eq!(p.progress, 0.5);
        assert_eq!(p.message.as_deref(), Some("halfway"));
    }

    #[test]
    fn test_cancel_notification_deserializes() {
        let json_str = r#"{ "requestId": 42, "reason": "user cancelled" }"#;
        let c: CancelNotification = serde_json::from_str(json_str).unwrap();
        assert_eq!(c.reason.as_deref(), Some("user cancelled"));
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
