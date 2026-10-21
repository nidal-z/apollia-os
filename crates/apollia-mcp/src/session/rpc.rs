//! JSON-RPC sends and the resource reads built on them.
//!
//! Split out of `session.rs`: the dispatch task stays in the parent, the
//! request and notification senders, the tool call, and the resource reads
//! live here.
//!
//! The sends live on [`RpcLink`], which a session shares through an `Arc`
//! with every [`SessionCaller`] it hands out, so a tool call can run in a task
//! of its own while the manager actor goes on answering.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use tokio::sync::{oneshot, Mutex};

use crate::jsonrpc::{JsonRpcNotification, JsonRpcRequest, JsonRpcResponse};
use crate::protocol::{ToolCallParams, ToolCallResult, ToolsListResult};
use crate::session::format_stderr_hint;
use crate::session::retry::{with_transport_retry, McpRetryConfig};
use crate::session::{McpSession, McpSessionError};
use crate::transport::McpTransport;

/// The JSON-RPC link of one session: its transport, the requests waiting for
/// their response, and the request counter. Shared by the session and the
/// callers it hands out; the dispatch task, which holds it too, routes the
/// responses into `pending`.
pub(crate) struct RpcLink {
    /// Server name, for the errors.
    server: String,
    /// Transport that handles all byte-level send/recv with the server.
    pub(super) transport: Arc<dyn McpTransport>,
    /// Pending in-flight requests: request ID to reply oneshot.
    pub(super) pending: Arc<Mutex<HashMap<u64, oneshot::Sender<JsonRpcResponse>>>>,
    /// Monotonically increasing request ID counter.
    next_id: AtomicU64,
}

impl RpcLink {
    pub(super) fn new(server: String, transport: Arc<dyn McpTransport>) -> Self {
        Self {
            server,
            transport,
            pending: Arc::default(),
            next_id: AtomicU64::new(1),
        }
    }

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
                    server: self.server.clone(),
                    www_authenticate,
                }
            }
            other => McpSessionError::StdinClosed {
                server: self.server.clone(),
                cause: other.to_string(),
            },
        })?;

        let duration = std::time::Duration::from_secs(timeout_secs);

        match tokio::time::timeout(duration, rx).await {
            Ok(Ok(response)) => {
                if let Some(err) = response.error {
                    return Err(McpSessionError::JsonRpcError {
                        server: self.server.clone(),
                        code: err.code,
                        message: err.message,
                    });
                }
                response
                    .result
                    .ok_or_else(|| McpSessionError::InitializeFailed {
                        server: self.server.clone(),
                        cause: "server returned a response with neither result nor error"
                            .to_string(),
                    })
            }
            Ok(Err(_)) => Err(McpSessionError::ServerExited {
                server: self.server.clone(),
            }),
            Err(_) => {
                // Timed out: remove the stale pending entry to avoid map growth.
                self.pending.lock().await.remove(&id);
                Err(McpSessionError::InitializeTimeout {
                    server: self.server.clone(),
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
                server: self.server.clone(),
                cause: e.to_string(),
            })
    }
}

/// A clonable handle on the JSON-RPC link of one session, to call its tools
/// from a task of its own.
///
/// It holds the link, not the session: the session stays with the manager
/// actor, which alone mutates its health and caches. A call through a caller
/// whose session has been shut down fails with the transport error, as a call
/// on the session itself would.
#[derive(Clone)]
pub struct SessionCaller {
    rpc: Arc<RpcLink>,
    call_timeout_secs: u64,
    init_timeout_secs: u64,
}

impl SessionCaller {
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
        with_transport_retry(&McpRetryConfig::DEFAULT, &self.rpc.server, || {
            self.call_tool_once(tool_name, arguments.clone())
        })
        .await
    }
    /// Single attempt at a `tools/call` request, without retry.
    async fn call_tool_once(
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
            .rpc
            .send_request("tools/call", Some(params_value), self.call_timeout_secs)
            .await
            .map_err(|e| match e {
                McpSessionError::InitializeTimeout { .. } => McpSessionError::ToolCallTimeout {
                    server: self.rpc.server.clone(),
                    tool: tool_name.to_string(),
                    timeout_secs: self.call_timeout_secs,
                },
                other => other,
            })?;

        serde_json::from_value(response).map_err(|e| McpSessionError::ToolCallFailed {
            server: self.rpc.server.clone(),
            tool: tool_name.to_string(),
            cause: e.to_string(),
        })
    }
    /// Read every tool schema the server lists (`tools/list`), name to schema.
    ///
    /// What a deferred session caches when a tool appears that its index did
    /// not hold. A failure names `tool_name`, the tool the schemas were
    /// fetched for.
    pub(crate) async fn list_tool_schemas(
        &self,
        tool_name: &str,
    ) -> Result<Vec<(String, serde_json::Value)>, McpSessionError> {
        let failed = |cause: String| McpSessionError::SchemaFetchFailed {
            server: self.rpc.server.clone(),
            tool: tool_name.to_string(),
            cause,
        };
        let response = self
            .rpc
            .send_request("tools/list", None, self.init_timeout_secs)
            .await
            .map_err(|e| failed(e.to_string()))?;
        let result: ToolsListResult =
            serde_json::from_value(response).map_err(|e| failed(e.to_string()))?;
        Ok(result
            .tools
            .into_iter()
            .map(|tool| (tool.name, tool.input_schema))
            .collect())
    }
}

impl McpSession {
    /// A caller on this session's link, to run a tool call in a task of its
    /// own. See [`SessionCaller`].
    #[must_use]
    pub fn caller(&self) -> SessionCaller {
        SessionCaller {
            rpc: Arc::clone(&self.rpc),
            call_timeout_secs: self.config.call_timeout_secs,
            init_timeout_secs: self.config.init_timeout_secs,
        }
    }
    /// Send a JSON-RPC request on this session's link. See
    /// [`RpcLink::send_request`].
    pub(super) async fn send_request(
        &self,
        method: &str,
        params: Option<serde_json::Value>,
        timeout_secs: u64,
    ) -> Result<serde_json::Value, McpSessionError> {
        self.rpc.send_request(method, params, timeout_secs).await
    }
    /// Send a JSON-RPC notification on this session's link.
    pub(super) async fn send_notification(
        &self,
        method: &str,
        params: Option<serde_json::Value>,
    ) -> Result<(), McpSessionError> {
        self.rpc.send_notification(method, params).await
    }
    /// Execute a tool on this MCP server via `tools/call`. See
    /// [`SessionCaller::call_tool`].
    pub async fn call_tool(
        &self,
        tool_name: &str,
        arguments: Option<serde_json::Value>,
    ) -> Result<ToolCallResult, McpSessionError> {
        self.caller().call_tool(tool_name, arguments).await
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
