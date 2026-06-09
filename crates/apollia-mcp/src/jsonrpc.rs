//! JSON-RPC 2.0 types for MCP transport.
//!
//! These types cover the three message kinds defined by JSON-RPC 2.0:
//! - [`JsonRpcRequest`]: a call that expects a response (has an `id`)
//! - [`JsonRpcResponse`]: the server's reply (carries either `result` or `error`)
//! - [`JsonRpcNotification`]: a fire-and-forget message (no `id`, no response expected)
//!
//! `JsonRpcError` is the error object embedded in a [`JsonRpcResponse`].

use serde::{Deserialize, Serialize};

/// A JSON-RPC 2.0 request sent to the MCP server.
///
/// `jsonrpc` is always `"2.0"`. `params` is omitted from the serialized JSON
/// when `None`, as required by the JSON-RPC 2.0 spec.
#[derive(Debug, Clone, Serialize)]
pub struct JsonRpcRequest {
    /// Protocol version, always `"2.0"`.
    pub jsonrpc: &'static str,
    /// Request identifier used to correlate the response.
    pub id: u64,
    /// Method name to invoke (e.g. `"initialize"`, `"tools/list"`).
    pub method: String,
    /// Optional structured parameters for the method.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
}

impl JsonRpcRequest {
    /// Constructs a new request with the given `id`, `method`, and optional `params`.
    ///
    /// `jsonrpc` is always set to `"2.0"`.
    pub fn new(id: u64, method: impl Into<String>, params: Option<serde_json::Value>) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            method: method.into(),
            params,
        }
    }
}

/// A JSON-RPC 2.0 response received from the MCP server.
///
/// Exactly one of `result` or `error` is present in a well-formed response.
/// `id` is `None` only when the server cannot determine which request caused
/// the error (e.g. parse error).
#[derive(Debug, Deserialize)]
pub struct JsonRpcResponse {
    /// Protocol version, always `"2.0"`.
    pub jsonrpc: String,
    /// Correlates this response to a prior request.
    pub id: Option<u64>,
    /// Successful result payload (present when no error occurred).
    pub result: Option<serde_json::Value>,
    /// Error payload (present when the request failed).
    pub error: Option<JsonRpcError>,
}

/// The error object contained in a [`JsonRpcResponse`] when a request fails.
#[derive(Debug, Deserialize)]
pub struct JsonRpcError {
    /// Numeric error code as defined by JSON-RPC 2.0 or MCP.
    pub code: i64,
    /// Human-readable description of the error.
    pub message: String,
    /// Optional additional error data provided by the server.
    pub data: Option<serde_json::Value>,
}

/// A JSON-RPC 2.0 notification sent to the MCP server.
///
/// Notifications have no `id` and require no response from the server.
/// `params` is omitted from the serialized JSON when `None`.
#[derive(Debug, Clone, Serialize)]
pub struct JsonRpcNotification {
    /// Protocol version, always `"2.0"`.
    pub jsonrpc: &'static str,
    /// Notification method name (e.g. `"notifications/initialized"`).
    pub method: String,
    /// Optional structured parameters for the notification.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
}

impl JsonRpcNotification {
    /// Constructs a new notification with the given `method` and optional `params`.
    ///
    /// `jsonrpc` is always set to `"2.0"`.
    pub fn new(method: impl Into<String>, params: Option<serde_json::Value>) -> Self {
        Self {
            jsonrpc: "2.0",
            method: method.into(),
            params,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_request_serialization_conforms_to_jsonrpc() {
        // GIVEN
        let request = JsonRpcRequest::new(1, "initialize", Some(json!({})));
        // WHEN
        let json_str = serde_json::to_string(&request).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        // THEN
        assert_eq!(value["jsonrpc"], "2.0");
        assert_eq!(value["id"], 1);
        assert_eq!(value["method"], "initialize");
        assert_eq!(value["params"], json!({}));
    }

    #[test]
    fn test_request_without_params_omits_field() {
        // GIVEN
        let request = JsonRpcRequest::new(1, "tools/list", None);
        // WHEN
        let json_str = serde_json::to_string(&request).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        // THEN
        assert!(value.get("params").is_none());
    }

    #[test]
    fn test_response_with_result() {
        // GIVEN
        let json_str = r#"{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2024-11-05"}}"#;
        // WHEN
        let response: JsonRpcResponse = serde_json::from_str(json_str).unwrap();
        // THEN
        assert!(response.result.is_some());
        assert!(response.error.is_none());
        assert_eq!(response.id, Some(1));
    }

    #[test]
    fn test_response_with_error() {
        // GIVEN
        let json_str =
            r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32600,"message":"Invalid Request"}}"#;
        // WHEN
        let response: JsonRpcResponse = serde_json::from_str(json_str).unwrap();
        // THEN
        assert!(response.result.is_none());
        assert!(response.error.is_some());
        assert_eq!(response.error.unwrap().code, -32600);
    }

    #[test]
    fn test_notification_has_no_id() {
        // GIVEN
        let notification = JsonRpcNotification::new("notifications/initialized", None);
        // WHEN
        let json_str = serde_json::to_string(&notification).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        // THEN
        assert!(value.get("id").is_none());
        assert_eq!(value["jsonrpc"], "2.0");
        assert_eq!(value["method"], "notifications/initialized");
    }

    #[test]
    fn test_request_roundtrip_via_raw_json() {
        // GIVEN
        let request = JsonRpcRequest::new(42, "tools/call", Some(json!({"name": "test"})));
        // WHEN
        let json_str = serde_json::to_string(&request).unwrap();
        let raw: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        // THEN
        assert_eq!(raw["jsonrpc"], "2.0");
        assert_eq!(raw["id"], 42);
        assert_eq!(raw["method"], "tools/call");
        assert_eq!(raw["params"]["name"], "test");
    }
}
