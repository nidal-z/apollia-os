//! Incoming JSON-RPC request types for the MCP stdio server.
//!
//! These types represent the three MCP methods handled by [`McpStdioServer`]:
//! `initialize`, `tools/list`, and `tools/call`.
//!
//! Deserialization is performed from a raw `serde_json::Value` rather than a
//! `#[serde(tag = "method")]` enum because MCP request params are nested objects
//! with optional presence; raw dispatch avoids serde tag limitations.

use serde::Deserialize;

/// Parameters embedded in a `tools/call` JSON-RPC request.
#[derive(Debug, Deserialize)]
pub struct CallToolParams {
    /// Name of the tool to invoke.
    pub name: String,
    /// JSON arguments forwarded to the tool executor.
    pub arguments: serde_json::Value,
}

/// Parameters embedded in an `initialize` JSON-RPC request.
#[derive(Debug, Deserialize)]
pub struct InitializeParams {
    /// MCP protocol version declared by the client (e.g. `"2024-11-05"`).
    #[serde(rename = "protocolVersion")]
    pub protocol_version: String,
    /// Opaque client identity object (name, version, …).
    #[serde(rename = "clientInfo")]
    pub client_info: serde_json::Value,
}

/// Parsed representation of an incoming MCP JSON-RPC request.
///
/// Produced by [`parse_request`] from a raw `serde_json::Value`. The `id` field
/// is preserved so every response can be correlated back to its originating request.
#[derive(Debug)]
pub enum McpRequest {
    /// Client handshake: must be the first message in every session.
    Initialize {
        /// JSON-RPC request id.
        id: serde_json::Value,
        /// Handshake parameters.
        params: InitializeParams,
    },
    /// List all tools exposed by this server.
    ListTools {
        /// JSON-RPC request id.
        id: serde_json::Value,
    },
    /// Invoke a specific tool.
    CallTool {
        /// JSON-RPC request id.
        id: serde_json::Value,
        /// Tool name + arguments.
        params: CallToolParams,
    },
    /// Notification: no response expected.
    ///
    /// Notifications carry no `id` field per JSON-RPC 2.0 spec.
    Notification {
        /// Notification method name.
        method: String,
    },
}

/// Parse a raw JSON-RPC value into a typed [`McpRequest`].
///
/// Returns `Err(id)` carrying the original request id (or `null`) when the
/// message cannot be mapped to a known method, so the caller can emit a
/// proper JSON-RPC error response.
///
/// # Errors
///
/// - Returns `Err(id)` when `method` is missing, not a string, or unrecognised.
/// - Returns `Err(id)` when `params` is present but cannot be deserialized into
///   the expected parameter type for that method.
pub fn parse_request(raw: serde_json::Value) -> Result<McpRequest, serde_json::Value> {
    let id = raw.get("id").cloned().unwrap_or(serde_json::Value::Null);

    let method = match raw.get("method").and_then(|v| v.as_str()) {
        Some(m) => m.to_string(),
        None => return Err(id),
    };

    // Notifications have no `id` field.
    if raw.get("id").is_none() {
        return Ok(McpRequest::Notification { method });
    }

    let params = raw
        .get("params")
        .cloned()
        .unwrap_or(serde_json::Value::Null);

    match method.as_str() {
        "initialize" => {
            let p: InitializeParams = serde_json::from_value(params).map_err(|_| id.clone())?;
            Ok(McpRequest::Initialize { id, params: p })
        }
        "tools/list" => Ok(McpRequest::ListTools { id }),
        "tools/call" => {
            let p: CallToolParams = serde_json::from_value(params).map_err(|_| id.clone())?;
            Ok(McpRequest::CallTool { id, params: p })
        }
        _ => Err(id),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parse_initialize_request() {
        // GIVEN
        let raw = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "clientInfo": { "name": "test-client" }
            }
        });
        // WHEN
        let req = parse_request(raw).expect("should parse");
        // THEN
        assert!(matches!(req, McpRequest::Initialize { .. }));
    }

    #[test]
    fn parse_tools_list_request() {
        // GIVEN
        let raw = json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" });
        // WHEN
        let req = parse_request(raw).expect("should parse");
        // THEN
        assert!(matches!(req, McpRequest::ListTools { .. }));
    }

    #[test]
    fn parse_tools_call_request() {
        // GIVEN
        let raw = json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": { "name": "bash_executor", "arguments": { "command": "echo hi" } }
        });
        // WHEN
        let req = parse_request(raw).expect("should parse");
        // THEN
        assert!(matches!(req, McpRequest::CallTool { .. }));
    }

    #[test]
    fn parse_unknown_method_returns_err_with_id() {
        // GIVEN
        let raw = json!({ "jsonrpc": "2.0", "id": 99, "method": "unknown/method" });
        // WHEN
        let result = parse_request(raw);
        // THEN
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), json!(99));
    }

    #[test]
    fn parse_notification_has_no_id() {
        // GIVEN a notifications/initialized message (no id field)
        let raw = json!({ "jsonrpc": "2.0", "method": "notifications/initialized" });
        // WHEN
        let req = parse_request(raw).expect("notification should parse");
        // THEN
        assert!(matches!(req, McpRequest::Notification { .. }));
    }
}
