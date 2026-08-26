//! JSON-RPC sends and the resource reads built on them.
//!
//! Split out of `session.rs`: the dispatch task stays in the parent, the
//! request and notification senders, the tool call, and the resource reads
//! live here.

use std::sync::atomic::Ordering;

use tokio::sync::oneshot;

use crate::jsonrpc::{JsonRpcNotification, JsonRpcRequest, JsonRpcResponse};
use crate::protocol::{ToolCallParams, ToolCallResult};
use crate::session::format_stderr_hint;
use crate::session::retry::{with_transport_retry, McpRetryConfig};
use crate::session::{McpSession, McpSessionError};

impl McpSession {
    /// Send a JSON-RPC request and wait for the response, with a hard timeout.
    ///
    /// Inserts a [`oneshot::Sender`] into `pending`, writes the serialised request
    /// to the transport, then awaits the response on the matching receiver.
    /// On timeout, the pending entry is removed to prevent map growth.
    pub(super) async fn send_request(
        &self,
        method: &str,
        params: Option<serde_json::Value>,
        timeout_secs: u64,
    ) -> Result<serde_json::Value, McpSessionError> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel::<JsonRpcResponse>();

        self.pending.lock().await.insert(id, tx);

        let request = JsonRpcRequest::new(id, method, params);
        let json = serde_json::to_string(&request)
            .map_err(|e| McpSessionError::SerdeError(e.to_string()))?;

        self.transport.send(&json).await.map_err(|e| match e {
            // Preserve the WWW-Authenticate header structurally so the
            // orchestration layer (Phase 4) can drive the MCP HTTP OAuth flow
            // without re-parsing it out of a stringified error message.
            crate::transport::TransportError::Unauthorized { www_authenticate } => {
                McpSessionError::Unauthorized {
                    server: self.config.name.clone(),
                    www_authenticate,
                }
            }
            other => McpSessionError::StdinClosed {
                server: self.config.name.clone(),
                cause: other.to_string(),
            },
        })?;

        let duration = std::time::Duration::from_secs(timeout_secs);

        match tokio::time::timeout(duration, rx).await {
            Ok(Ok(response)) => {
                if let Some(err) = response.error {
                    return Err(McpSessionError::JsonRpcError {
                        server: self.config.name.clone(),
                        code: err.code,
                        message: err.message,
                    });
                }
                response
                    .result
                    .ok_or_else(|| McpSessionError::InitializeFailed {
                        server: self.config.name.clone(),
                        cause: "server returned a response with neither result nor error"
                            .to_string(),
                    })
            }
            Ok(Err(_)) => Err(McpSessionError::ServerExited {
                server: self.config.name.clone(),
            }),
            Err(_) => {
                // Timed out: remove the stale pending entry to avoid map growth.
                self.pending.lock().await.remove(&id);
                Err(McpSessionError::InitializeTimeout {
                    server: self.config.name.clone(),
                    timeout_secs,
                    stderr_hint: format_stderr_hint(&self.transport.stderr_tail()),
                })
            }
        }
    }
    /// Send a JSON-RPC notification (fire-and-forget; no response is expected).
    pub(super) async fn send_notification(
        &self,
        method: &str,
        params: Option<serde_json::Value>,
    ) -> Result<(), McpSessionError> {
        let notification = JsonRpcNotification::new(method, params);
        let json = serde_json::to_string(&notification)
            .map_err(|e| McpSessionError::SerdeError(e.to_string()))?;
        self.transport
            .send(&json)
            .await
            .map_err(|e| McpSessionError::StdinClosed {
                server: self.config.name.clone(),
                cause: e.to_string(),
            })
    }
    /// Execute a tool on this MCP server via `tools/call`.
    ///
    /// Serialises `tool_name` and `arguments` into a `tools/call` JSON-RPC request,
    /// sends it through the transport, and waits for the response. The timeout
    /// applied is `call_timeout_secs` from the server configuration.
    ///
    /// Transport errors (`StdinClosed`, `ServerExited`) are retried up to 3 times with
    /// exponential backoff (1s, 2s, 4s, capped at 8s) before the error is propagated.
    ///
    /// Returns the raw [`ToolCallResult`] so the caller can inspect `is_error` and
    /// route content accordingly. Deserialisation failures are surfaced as
    /// [`McpSessionError::ToolCallFailed`].
    pub async fn call_tool(
        &self,
        tool_name: &str,
        arguments: Option<serde_json::Value>,
    ) -> Result<ToolCallResult, McpSessionError> {
        with_transport_retry(&McpRetryConfig::DEFAULT, &self.config.name, || {
            self.call_tool_once(tool_name, arguments.clone())
        })
        .await
    }
    /// Single attempt at a `tools/call` request, without retry.
    pub(super) async fn call_tool_once(
        &self,
        tool_name: &str,
        arguments: Option<serde_json::Value>,
    ) -> Result<ToolCallResult, McpSessionError> {
        let params = ToolCallParams {
            name: tool_name.to_string(),
            arguments,
        };

        let params_value = serde_json::to_value(&params)
            .map_err(|e| McpSessionError::SerdeError(e.to_string()))?;

        let response = self
            .send_request(
                "tools/call",
                Some(params_value),
                self.config.call_timeout_secs,
            )
            .await
            .map_err(|e| match e {
                McpSessionError::InitializeTimeout { .. } => McpSessionError::ToolCallTimeout {
                    server: self.config.name.clone(),
                    tool: tool_name.to_string(),
                    timeout_secs: self.config.call_timeout_secs,
                },
                other => other,
            })?;

        serde_json::from_value(response).map_err(|e| McpSessionError::ToolCallFailed {
            server: self.config.name.clone(),
            tool: tool_name.to_string(),
            cause: e.to_string(),
        })
    }
    /// List the resources exposed by the server (`resources/list`).
    ///
    /// Returns an empty result when the server does not advertise the
    /// `resources` capability; callers should branch on
    /// [`McpSession::capabilities`] to avoid the round-trip in that case.
    pub async fn list_resources(
        &self,
    ) -> Result<crate::protocol::ResourcesListResult, McpSessionError> {
        let response = self
            .send_request("resources/list", None, self.config.call_timeout_secs)
            .await?;
        serde_json::from_value(response).map_err(|e| McpSessionError::ToolCallFailed {
            server: self.config.name.clone(),
            tool: "resources/list".into(),
            cause: e.to_string(),
        })
    }
    /// Read a resource's content (`resources/read`).
    pub async fn read_resource(
        &self,
        uri: &str,
    ) -> Result<crate::protocol::ResourcesReadResult, McpSessionError> {
        let params = crate::protocol::ResourcesReadParams {
            uri: uri.to_owned(),
        };
        let params_value = serde_json::to_value(&params)
            .map_err(|e| McpSessionError::SerdeError(e.to_string()))?;
        let response = self
            .send_request(
                "resources/read",
                Some(params_value),
                self.config.call_timeout_secs,
            )
            .await?;
        serde_json::from_value(response).map_err(|e| McpSessionError::ToolCallFailed {
            server: self.config.name.clone(),
            tool: "resources/read".into(),
            cause: e.to_string(),
        })
    }
}
