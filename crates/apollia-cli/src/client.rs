//! HTTP client for communicating with the Apollia runtime.
//!
//! On **Unix** (macOS, Linux): connects via `tokio::net::UnixStream` to the
//! `--socket` path (default `~/.apollia/runtime.sock`). Filesystem-based
//! security: the runtime chmods the socket to `0o600` after binding.
//!
//! On **Windows**: `tokio::net::TcpStream` on `127.0.0.1:DEFAULT_TCP_PORT`.
//! The runtime always listens on TCP in parallel, and Windows has no native
//! support for Unix domain sockets in hyper 1.x.

use std::path::{Path, PathBuf};

use futures::channel::mpsc;
use futures::SinkExt;
use http_body_util::{BodyExt, Full};
use hyper::body::Bytes;
use hyper::client::conn::http1;
use hyper_util::rt::TokioIo;

#[cfg(windows)]
use tokio::net::TcpStream;
#[cfg(unix)]
use tokio::net::UnixStream;

/// Low-level I/O type used for the daemon to CLI connection.
#[cfg(unix)]
type RuntimeStream = UnixStream;
#[cfg(windows)]
type RuntimeStream = TcpStream;

/// Opens a connection to the runtime.
///
/// On Unix, uses `socket_path` (Unix domain socket).
/// On Windows, ignores the path and connects to `127.0.0.1:DEFAULT_TCP_PORT`.
#[cfg(unix)]
async fn connect_runtime(socket_path: &Path) -> std::io::Result<RuntimeStream> {
    UnixStream::connect(socket_path).await
}

#[cfg(windows)]
async fn connect_runtime(_socket_path: &Path) -> std::io::Result<RuntimeStream> {
    TcpStream::connect(format!("127.0.0.1:{}", DEFAULT_TCP_PORT)).await
}

/// Default Unix socket path for the Apollia runtime: `~/.apollia/runtime.sock`.
///
/// A function rather than a constant because the path depends on the user's
/// home directory. It used to be the literal `/tmp/apollia.sock`, a name any
/// account on the machine could take first.
pub fn default_socket_path() -> PathBuf {
    apollia_core::paths::socket_path_or_temp()
}

/// Default TCP port for the Apollia runtime.
pub const DEFAULT_TCP_PORT: u16 = 7771;

/// Parameters for [`RuntimeClient::authorize_tool`].
///
/// `decision` is `"accept"`, `"refuse"`, or `"always_accept"`. `reason` is only
/// honoured for `"refuse"`, `scope` only for `"always_accept"` (one of
/// `this_tool`, `this_session`, `this_agent`, `this_project`, `global`).
pub struct AuthorizeToolArgs<'a> {
    /// Chat session id.
    pub session_id: &'a str,
    /// Id of the message that triggered the tool call.
    pub message_id: &'a str,
    /// Name of the tool awaiting approval.
    pub tool_name: &'a str,
    /// Decision keyword sent to the runtime.
    pub decision: &'a str,
    /// Optional rejection reason (only for `"refuse"`).
    pub reason: Option<&'a str>,
    /// Optional always-accept scope (only for `"always_accept"`).
    pub scope: Option<&'a str>,
}

/// Client for communicating with the Apollia runtime via Unix socket.
///
/// Each method opens a new connection (HTTP/1.1 per-request). This is fine
/// for CLI usage where request frequency is low.
pub struct RuntimeClient {
    socket_path: PathBuf,
    /// Bearer token attached to every request. Loaded best-effort from
    /// `~/.apollia/api-token`. Ignored by the Unix socket (never token-gated),
    /// required by an authenticated TCP listener (Windows, or a remote host).
    api_token: Option<String>,
}

/// Result returned by `POST /api/v1/notifications/test` for one channel.
#[derive(Debug, serde::Deserialize)]
pub struct ChannelTestResult {
    /// Unique channel identifier.
    pub channel_id: String,
    /// Channel type: `"desktop"`, `"webhook"`, or `"sse"`.
    #[serde(rename = "type")]
    pub kind: String,
    /// Status: `"ok"`, `"error"`, or `"disabled"`.
    pub status: String,
    /// Error message when `status == "error"`.
    pub error: Option<String>,
    /// Measured latency in milliseconds (`None` if the channel is disabled).
    pub latency_ms: Option<u64>,
}

/// Errors that can occur when communicating with the runtime.
#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    /// Failed to connect to the Unix socket (runtime not started).
    #[error("runtime not started (connection refused)")]
    ConnectionRefused,

    /// IO error during communication.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// HTTP protocol error.
    #[error("http error: {0}")]
    Http(String),

    /// Failed to parse response body as JSON.
    #[error("invalid JSON response: {0}")]
    InvalidJson(#[from] serde_json::Error),

    /// Server returned a non-success status code.
    #[error("server error ({status}): {body}")]
    ServerError {
        /// HTTP status code.
        status: u16,
        /// Response body text.
        body: String,
    },
}

/// Raw HTTP response from the runtime.
#[derive(Debug)]
pub struct RawResponse {
    /// HTTP status code.
    pub status: u16,
    /// Response body as string.
    pub body: String,
}

/// Extract a human-readable error message from a response body.
///
/// Tries to parse the body as JSON and read the `"error"` field.
/// Falls back to the raw body, or a generic message if the body is empty.
/// Percent-encode a value for use as a single URL path segment.
///
/// MCP tool names look like `mcp:server/tool`; the `/` would otherwise split the
/// path and break `:tool` routing (the tool then reads as not-found). Encodes
/// everything outside the RFC 3986 unreserved set.
fn encode_path_segment(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(b as char);
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn extract_error(body: &str, status: u16) -> String {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|j| j.get("error").and_then(|e| e.as_str()).map(str::to_string))
        .unwrap_or_else(|| {
            if body.is_empty() {
                format!("server error (status {status}): no response body")
            } else {
                body.to_string()
            }
        })
}

/// Best-effort read of the local API bearer token from `~/.apollia/api-token`.
///
/// Returns `None` when the file is absent or empty. The token lets the CLI
/// drive an authenticated TCP listener; it is harmless on the Unix socket,
/// which is never token-gated.
fn load_default_api_token() -> Option<String> {
    let path = apollia_core::paths::data_dir()?.join("api-token");
    std::fs::read_to_string(path)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

impl RuntimeClient {
    /// Create a new client targeting the given Unix socket path.
    pub fn new(socket_path: PathBuf) -> Self {
        Self {
            socket_path,
            api_token: load_default_api_token(),
        }
    }

    /// Create a client with the default socket path (`~/.apollia/runtime.sock`).
    pub fn default_client() -> Self {
        Self::new(default_socket_path())
    }

    /// Override the bearer token used for TCP authentication.
    pub fn with_token(mut self, token: Option<String>) -> Self {
        self.api_token = token;
        self
    }

    /// The `Authorization` header value, when a token is configured.
    fn auth_header(&self) -> Option<String> {
        self.api_token.as_ref().map(|t| format!("Bearer {t}"))
    }

    /// Return the socket path this client connects to.
    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }

    /// Send a GET request and return the raw response.
    pub async fn get(&self, uri: &str) -> Result<RawResponse, ClientError> {
        self.request("GET", uri, None).await
    }

    /// Send a POST request with an optional JSON body and return the raw response.
    pub async fn post(
        &self,
        uri: &str,
        body: Option<&serde_json::Value>,
    ) -> Result<RawResponse, ClientError> {
        self.request("POST", uri, body).await
    }

    /// Send a DELETE request and return the raw response.
    pub async fn delete(&self, uri: &str) -> Result<RawResponse, ClientError> {
        self.request("DELETE", uri, None).await
    }

    /// Send a PUT request with an optional JSON body and return the raw response.
    pub async fn put(
        &self,
        uri: &str,
        body: Option<&serde_json::Value>,
    ) -> Result<RawResponse, ClientError> {
        self.request("PUT", uri, body).await
    }

    /// Send a POST request with a raw body and custom content-type.
    ///
    /// Used for multipart uploads where the body is pre-built by the caller.
    pub async fn post_multipart(
        &self,
        uri: &str,
        body: &[u8],
        content_type: &str,
    ) -> Result<RawResponse, ClientError> {
        self.request_raw("POST", uri, body, content_type).await
    }

    /// Check if the runtime is healthy by calling `GET /api/v1/health`.
    pub async fn health(&self) -> Result<serde_json::Value, ClientError> {
        let resp = self.get("/api/v1/health").await?;
        if resp.status != 200 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: resp.body,
            });
        }
        let json: serde_json::Value = serde_json::from_str(&resp.body)?;
        Ok(json)
    }

    /// Request shutdown via `POST /api/v1/shutdown`.
    pub async fn shutdown(&self) -> Result<serde_json::Value, ClientError> {
        let resp = self.post("/api/v1/shutdown", None).await?;
        let json: serde_json::Value = serde_json::from_str(&resp.body)?;
        Ok(json)
    }

    /// List all agents via `GET /api/v1/agents`.
    pub async fn list_agents(&self) -> Result<serde_json::Value, ClientError> {
        let resp = self.get("/api/v1/agents").await?;
        if resp.status != 200 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: resp.body,
            });
        }
        let json: serde_json::Value = serde_json::from_str(&resp.body)?;
        Ok(json)
    }

    /// List A2A-capable agents via `GET /api/v1/agents?supports_a2a=true`.
    ///
    /// Returns all agents that declare `supports_a2a = true` in their manifest,
    /// including their skill descriptors and version.
    pub async fn list_a2a_agents(&self) -> Result<serde_json::Value, ClientError> {
        let resp = self.get("/api/v1/agents?supports_a2a=true").await?;
        if resp.status != 200 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: resp.body,
            });
        }
        let json: serde_json::Value = serde_json::from_str(&resp.body)?;
        Ok(json)
    }

    /// Start (register) a new agent via `POST /api/v1/agents`.
    pub async fn start_agent(&self, agent_path: &str) -> Result<serde_json::Value, ClientError> {
        let body = serde_json::json!({ "agent_path": agent_path });
        let resp = self.post("/api/v1/agents", Some(&body)).await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    /// Get agent detail via `GET /api/v1/agents/{id}`.
    pub async fn get_agent(&self, agent_id: &str) -> Result<serde_json::Value, ClientError> {
        let resp = self.get(&format!("/api/v1/agents/{agent_id}")).await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    /// List in-memory A2A messages for an agent via
    /// `GET /api/v1/agents/{id}/messages?limit=N`.
    ///
    /// Returns the full JSON response body (`{ "messages": [...] }`) so the
    /// caller can render either tabular or JSON output without re-parsing.
    pub async fn list_agent_messages(
        &self,
        agent_id: &str,
        limit: Option<u32>,
    ) -> Result<serde_json::Value, ClientError> {
        let uri = match limit {
            Some(n) => format!("/api/v1/agents/{agent_id}/messages?limit={n}"),
            None => format!("/api/v1/agents/{agent_id}/messages"),
        };
        let resp = self.get(&uri).await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    /// Stop an agent via `DELETE /api/v1/agents/{id}`.
    pub async fn stop_agent(&self, agent_id: &str) -> Result<serde_json::Value, ClientError> {
        let resp = self.delete(&format!("/api/v1/agents/{agent_id}")).await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    /// Submit a task via `POST /api/v1/tasks`.
    pub async fn submit_task(
        &self,
        agent_id: &str,
        input: serde_json::Value,
    ) -> Result<serde_json::Value, ClientError> {
        self.submit_task_with_options(agent_id, input, serde_json::Value::Null)
            .await
    }

    /// Submit a task with per-run control options (`run_options`).
    ///
    /// `run_options` is forwarded verbatim under the `run_options` key so the
    /// runtime can honour CLI flags (`--plan`, `--autonomy`). Pass
    /// [`serde_json::Value::Null`] to send no options.
    pub async fn submit_task_with_options(
        &self,
        agent_id: &str,
        input: serde_json::Value,
        run_options: serde_json::Value,
    ) -> Result<serde_json::Value, ClientError> {
        let mut body = serde_json::json!({
            "agent_id": agent_id,
            "input": input,
        });
        if !run_options.is_null() {
            body["run_options"] = run_options;
        }
        let resp = self.post("/api/v1/tasks", Some(&body)).await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    /// List A2A skills via `GET /api/v1/a2a/skills`.
    ///
    /// Returns a flat list of every skill exposed by active Worker Agents,
    /// each carrying `skill_id`, `agent_name`, and the input schema.
    pub async fn list_a2a_skills(&self) -> Result<serde_json::Value, ClientError> {
        let resp = self.get("/api/v1/a2a/skills").await?;
        if resp.status != 200 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    /// Invoke an A2A Worker Agent skill via `POST /api/v1/a2a/invoke`.
    ///
    /// Routes by `skill_id` (not agent id) so the caller does not need to know
    /// which worker provides it. `timeout_secs = None` falls back to the
    /// runtime default (120 s).
    pub async fn invoke_a2a_skill(
        &self,
        skill_id: &str,
        input: serde_json::Value,
        timeout_secs: Option<u64>,
        caller: Option<&str>,
    ) -> Result<serde_json::Value, ClientError> {
        let mut body = serde_json::json!({
            "skill_id": skill_id,
            "input": input,
        });
        if let Some(t) = timeout_secs {
            body["timeout_secs"] = serde_json::Value::from(t);
        }
        if let Some(c) = caller {
            body["caller"] = serde_json::Value::String(c.to_string());
        }
        let resp = self.post("/api/v1/a2a/invoke", Some(&body)).await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    /// Get task status via `GET /api/v1/tasks/{id}`.
    pub async fn get_task(&self, task_id: &str) -> Result<serde_json::Value, ClientError> {
        let resp = self.get(&format!("/api/v1/tasks/{task_id}")).await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    /// List all triggers via `GET /api/v1/triggers`.
    pub async fn list_triggers(&self) -> Result<serde_json::Value, ClientError> {
        let resp = self.get("/api/v1/triggers").await?;
        if resp.status != 200 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: resp.body,
            });
        }
        let json: serde_json::Value = serde_json::from_str(&resp.body)?;
        Ok(json)
    }

    /// Get trigger detail via `GET /api/v1/triggers/{id}`.
    pub async fn get_trigger(&self, id: &str) -> Result<serde_json::Value, ClientError> {
        let resp = self.get(&format!("/api/v1/triggers/{id}")).await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    /// Fire a trigger immediately via `POST /api/v1/triggers/{id}/fire`.
    pub async fn fire_trigger(&self, id: &str) -> Result<serde_json::Value, ClientError> {
        let resp = self
            .post(&format!("/api/v1/triggers/{id}/fire"), None)
            .await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    /// Enable a trigger via `POST /api/v1/triggers/{id}/enable`.
    pub async fn enable_trigger(&self, id: &str) -> Result<serde_json::Value, ClientError> {
        let resp = self
            .post(&format!("/api/v1/triggers/{id}/enable"), None)
            .await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    /// Disable a trigger via `POST /api/v1/triggers/{id}/disable`.
    pub async fn disable_trigger(&self, id: &str) -> Result<serde_json::Value, ClientError> {
        let resp = self
            .post(&format!("/api/v1/triggers/{id}/disable"), None)
            .await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    /// Get trigger logs via `GET /api/v1/triggers/{id}/logs?last={last}`.
    pub async fn get_trigger_logs(
        &self,
        id: &str,
        last: usize,
    ) -> Result<serde_json::Value, ClientError> {
        let resp = self
            .get(&format!("/api/v1/triggers/{id}/logs?last={last}"))
            .await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    /// Hot-reload triggers via `POST /api/v1/triggers/reload`.
    ///
    /// Returns the JSON response on success (`{ "reloaded": <count> }`).
    pub async fn reload_triggers(&self) -> Result<serde_json::Value, ClientError> {
        let resp = self.post("/api/v1/triggers/reload", None).await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    /// Resume a HITL task via `POST /api/v1/tasks/{id}/resume`.
    ///
    /// Returns the raw response so the caller can handle HTTP 409
    /// (task not in `input_required` state) distinctly from other errors.
    pub async fn resume_task(
        &self,
        task_id: &str,
        approved: bool,
        reason: Option<String>,
    ) -> Result<RawResponse, ClientError> {
        let mut body = serde_json::json!({ "approved": approved });
        if let Some(r) = reason {
            body["reason"] = serde_json::Value::String(r);
        }
        self.post(&format!("/api/v1/tasks/{task_id}/resume"), Some(&body))
            .await
    }

    /// Send a test notification through all active channels via `POST /api/v1/notifications/test`.
    ///
    /// Returns the per-channel results (status, latency, error).
    pub async fn test_notifications(&self) -> Result<Vec<ChannelTestResult>, ClientError> {
        let resp = self.post("/api/v1/notifications/test", None).await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: resp.body,
            });
        }
        let json: serde_json::Value = serde_json::from_str(&resp.body)?;
        let results_val = json
            .get("results")
            .cloned()
            .unwrap_or(serde_json::Value::Array(vec![]));
        let results: Vec<ChannelTestResult> = serde_json::from_value(results_val)?;
        Ok(results)
    }

    /// List configured notification channels via `GET /api/v1/notifications/channels`.
    ///
    /// Returns the raw JSON value from the `"channels"` array.
    pub async fn list_notification_channels(&self) -> Result<serde_json::Value, ClientError> {
        let resp = self.get("/api/v1/notifications/channels").await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: resp.body,
            });
        }
        let json: serde_json::Value = serde_json::from_str(&resp.body)?;
        Ok(json)
    }

    /// Fetch notification log history via `GET /api/v1/notifications/logs?last={last}`.
    ///
    /// Returns the raw JSON value from the `"entries"` array.
    pub async fn notification_logs(&self, last: usize) -> Result<serde_json::Value, ClientError> {
        let resp = self
            .get(&format!("/api/v1/notifications/logs?last={last}"))
            .await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: resp.body,
            });
        }
        let json: serde_json::Value = serde_json::from_str(&resp.body)?;
        Ok(json)
    }

    /// List all registered pipelines via `GET /api/v1/pipelines`.
    ///
    /// Returns a JSON value with a `"pipelines"` array, each item having `"id"` and
    /// `"description"`.
    pub async fn list_pipelines(&self) -> Result<serde_json::Value, ClientError> {
        let resp = self.get("/api/v1/pipelines").await?;
        if resp.status != 200 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: resp.body,
            });
        }
        let json: serde_json::Value = serde_json::from_str(&resp.body)?;
        Ok(json)
    }

    /// Trigger a new pipeline run via `POST /api/v1/pipelines/{id}/run`.
    ///
    /// Returns the run summary including `run_id` on success.
    pub async fn run_pipeline(
        &self,
        id: &str,
        input: Option<&str>,
    ) -> Result<serde_json::Value, ClientError> {
        let body = match input {
            Some(i) => serde_json::json!({ "input": i }),
            None => serde_json::json!({}),
        };
        let resp = self
            .post(&format!("/api/v1/pipelines/{id}/run"), Some(&body))
            .await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    /// List recent runs for a pipeline via `GET /api/v1/pipelines/{id}/runs`.
    ///
    /// The server returns at most 20 runs. `limit` is a client-side cap applied
    /// on top of the server response.
    pub async fn list_pipeline_runs(
        &self,
        pipeline_id: &str,
        limit: u32,
    ) -> Result<serde_json::Value, ClientError> {
        let resp = self
            .get(&format!("/api/v1/pipelines/{pipeline_id}/runs"))
            .await?;
        if resp.status != 200 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: resp.body,
            });
        }
        let mut json: serde_json::Value = serde_json::from_str(&resp.body)?;
        // Apply client-side limit to the array.
        if let Some(arr) = json.as_array_mut() {
            arr.truncate(limit as usize);
        }
        Ok(json)
    }

    /// Get the detailed status of a pipeline run via `GET /api/v1/runs/{run_id}`.
    ///
    /// This endpoint does not require the pipeline id in the URL.
    pub async fn get_pipeline_run(&self, run_id: &str) -> Result<serde_json::Value, ClientError> {
        let resp = self.get(&format!("/api/v1/runs/{run_id}")).await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    /// Cancel a task via `DELETE /api/v1/tasks/{id}`.
    pub async fn cancel_task(&self, task_id: &str) -> Result<serde_json::Value, ClientError> {
        let resp = self.delete(&format!("/api/v1/tasks/{task_id}")).await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    /// Get STT engine status via `GET /api/v1/stt/status`.
    pub async fn stt_status(&self) -> Result<serde_json::Value, ClientError> {
        let resp = self.get("/api/v1/stt/status").await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    /// List STT transcriptions via `GET /api/v1/stt/transcriptions`.
    pub async fn stt_transcriptions(
        &self,
        limit: u32,
        offset: u32,
    ) -> Result<serde_json::Value, ClientError> {
        let resp = self
            .get(&format!(
                "/api/v1/stt/transcriptions?limit={limit}&offset={offset}"
            ))
            .await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    /// List STT models via `GET /api/v1/stt/models`.
    pub async fn stt_models(&self) -> Result<serde_json::Value, ClientError> {
        let resp = self.get("/api/v1/stt/models").await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    /// Create a new chat session via `POST /api/v1/sessions`.
    pub async fn create_chat_session(&self, mode: &str) -> Result<serde_json::Value, ClientError> {
        let body = serde_json::json!({ "mode": mode });
        let resp = self.post("/api/v1/sessions", Some(&body)).await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    /// Send a message in a chat session via `POST /api/v1/sessions/:id/messages`.
    pub async fn send_chat_message(
        &self,
        session_id: &str,
        content: &str,
    ) -> Result<serde_json::Value, ClientError> {
        let body = serde_json::json!({ "content": content });
        let resp = self
            .post(
                &format!("/api/v1/sessions/{session_id}/messages"),
                Some(&body),
            )
            .await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    /// One-shot completion via `POST /api/v1/llm/complete`.
    ///
    /// Optional `system` prompt + `user` message; `grammar` constrains local
    /// decoding with GBNF. Returns the parsed response (`content`, `usage`, ...).
    pub async fn llm_complete(
        &self,
        system: Option<&str>,
        user: &str,
        grammar: Option<&str>,
    ) -> Result<serde_json::Value, ClientError> {
        let mut messages = Vec::new();
        if let Some(s) = system {
            messages.push(serde_json::json!({ "role": "system", "content": s }));
        }
        messages.push(serde_json::json!({ "role": "user", "content": user }));
        let mut body = serde_json::json!({ "messages": messages });
        if let Some(g) = grammar {
            body["grammar"] = serde_json::Value::String(g.to_string());
        }
        let resp = self.post("/api/v1/llm/complete", Some(&body)).await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    /// Get session detail (with message history) via `GET /api/v1/sessions/:id`.
    pub async fn get_chat_session(
        &self,
        session_id: &str,
    ) -> Result<serde_json::Value, ClientError> {
        let resp = self.get(&format!("/api/v1/sessions/{session_id}")).await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    /// List recent sessions via `GET /api/v1/sessions/recent?limit=N`.
    pub async fn list_recent_chat_sessions(
        &self,
        limit: usize,
    ) -> Result<serde_json::Value, ClientError> {
        let resp = self
            .get(&format!("/api/v1/sessions/recent?limit={limit}"))
            .await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    /// Fork a session via `POST /api/v1/sessions/:id/fork`.
    ///
    /// When `up_to_index` is `None`, the full history is copied to the child.
    pub async fn fork_chat_session(
        &self,
        session_id: &str,
        up_to_index: Option<usize>,
    ) -> Result<serde_json::Value, ClientError> {
        let body = serde_json::json!({ "up_to_index": up_to_index });
        let resp = self
            .post(&format!("/api/v1/sessions/{session_id}/fork"), Some(&body))
            .await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    /// List child sessions (forks) of `session_id` via `GET /api/v1/sessions/:id/children`.
    pub async fn list_session_children(
        &self,
        session_id: &str,
    ) -> Result<serde_json::Value, ClientError> {
        let resp = self
            .get(&format!("/api/v1/sessions/{session_id}/children"))
            .await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    /// Resume an existing session via `POST /api/v1/sessions/:id/resume`.
    pub async fn resume_chat_session(
        &self,
        session_id: &str,
    ) -> Result<serde_json::Value, ClientError> {
        let resp = self
            .post(&format!("/api/v1/sessions/{session_id}/resume"), None)
            .await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    /// Resolve a pending tool approval via `POST /api/v1/sessions/:id/authorize`.
    pub async fn authorize_tool(
        &self,
        args: &AuthorizeToolArgs<'_>,
    ) -> Result<serde_json::Value, ClientError> {
        let mut body = serde_json::json!({
            "message_id": args.message_id,
            "tool_name": args.tool_name,
            "decision": args.decision,
        });
        if let Some(r) = args.reason {
            body["reason"] = serde_json::Value::String(r.to_string());
        }
        if let Some(s) = args.scope {
            body["scope"] = serde_json::Value::String(s.to_string());
        }
        let resp = self
            .post(
                &format!("/api/v1/sessions/{}/authorize", args.session_id),
                Some(&body),
            )
            .await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    /// Open a streaming GET connection for SSE endpoints.
    ///
    /// Unlike [`get`], this method does **not** buffer the entire response body.
    /// It reads HTTP body frames incrementally and yields complete text lines as they
    /// arrive from the server, enabling real-time display of Server-Sent Events.
    ///
    /// Endpoint-agnostic: used both by `GET /api/v1/tasks/{id}/stream` (task
    /// progress) and `GET /api/v1/sessions/{id}/stream` (chat tokens). Dropping
    /// the returned stream closes the connection automatically (the background
    /// reader task exits when the channel receiver is dropped).
    pub async fn stream_sse_lines(
        &self,
        uri: &str,
    ) -> Result<impl futures::Stream<Item = Result<String, ClientError>>, ClientError> {
        let stream = connect_runtime(&self.socket_path).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound
                || e.kind() == std::io::ErrorKind::ConnectionRefused
            {
                ClientError::ConnectionRefused
            } else {
                ClientError::Io(e)
            }
        })?;

        let io = TokioIo::new(stream);
        let (mut sender, conn) = http1::handshake(io)
            .await
            .map_err(|e| ClientError::Http(e.to_string()))?;

        tokio::spawn(async move {
            if let Err(e) = conn.await {
                tracing::debug!(error = %e, "SSE connection closed");
            }
        });

        let mut builder = hyper::Request::builder()
            .method("GET")
            .uri(uri)
            .header("host", "localhost")
            .header("accept", "text/event-stream");
        if let Some(auth) = self.auth_header() {
            builder = builder.header("authorization", auth);
        }
        let req = builder
            .body(Full::new(Bytes::new()))
            .map_err(|e| ClientError::Http(e.to_string()))?;

        let resp = sender
            .send_request(req)
            .await
            .map_err(|e| ClientError::Http(e.to_string()))?;

        // Buffer of 64 events, generous for any realistic task execution.
        let (tx, rx) = mpsc::channel::<Result<String, ClientError>>(64);

        // Background task: reads body frames, accumulates a line buffer, and
        // pushes complete lines (stripped of trailing '\r') to the channel.
        // Exits when the connection closes or when the receiver is dropped.
        tokio::spawn(pump_sse_body(resp.into_body(), tx));

        Ok(rx)
    }

    /// Internal: send an HTTP request with a raw byte body and explicit content-type.
    async fn request_raw(
        &self,
        method: &str,
        uri: &str,
        body: &[u8],
        content_type: &str,
    ) -> Result<RawResponse, ClientError> {
        let stream = connect_runtime(&self.socket_path).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound
                || e.kind() == std::io::ErrorKind::ConnectionRefused
            {
                ClientError::ConnectionRefused
            } else {
                ClientError::Io(e)
            }
        })?;

        let io = TokioIo::new(stream);
        let (mut sender, conn) = http1::handshake(io)
            .await
            .map_err(|e| ClientError::Http(e.to_string()))?;

        tokio::spawn(async move {
            if let Err(e) = conn.await {
                tracing::debug!(error = %e, "HTTP connection closed");
            }
        });

        let mut builder = hyper::Request::builder()
            .method(method)
            .uri(uri)
            .header("host", "localhost")
            .header("content-type", content_type);
        if let Some(auth) = self.auth_header() {
            builder = builder.header("authorization", auth);
        }
        let req = builder
            .body(Full::new(Bytes::from(body.to_vec())))
            .map_err(|e| ClientError::Http(e.to_string()))?;

        let resp = sender
            .send_request(req)
            .await
            .map_err(|e| ClientError::Http(e.to_string()))?;

        let status = resp.status().as_u16();

        if status >= 400 {
            let body_bytes = resp
                .into_body()
                .collect()
                .await
                .map_err(|e| ClientError::Http(e.to_string()))?
                .to_bytes();
            let body_str = String::from_utf8_lossy(&body_bytes).to_string();
            return Err(ClientError::ServerError {
                status,
                body: extract_error(&body_str, status),
            });
        }

        let body_bytes = resp
            .into_body()
            .collect()
            .await
            .map_err(|e| ClientError::Http(e.to_string()))?
            .to_bytes();
        let body_str = String::from_utf8_lossy(&body_bytes).to_string();

        Ok(RawResponse {
            status,
            body: body_str,
        })
    }

    /// Internal: send an HTTP request over Unix socket.
    async fn request(
        &self,
        method: &str,
        uri: &str,
        body: Option<&serde_json::Value>,
    ) -> Result<RawResponse, ClientError> {
        let stream = connect_runtime(&self.socket_path).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound
                || e.kind() == std::io::ErrorKind::ConnectionRefused
            {
                ClientError::ConnectionRefused
            } else {
                ClientError::Io(e)
            }
        })?;

        let io = TokioIo::new(stream);
        let (mut sender, conn) = http1::handshake(io)
            .await
            .map_err(|e| ClientError::Http(e.to_string()))?;

        tokio::spawn(async move {
            if let Err(e) = conn.await {
                tracing::debug!(error = %e, "HTTP connection closed");
            }
        });

        let req_body = match body {
            Some(json) => Full::new(Bytes::from(
                serde_json::to_vec(json).map_err(|e| ClientError::Http(e.to_string()))?,
            )),
            None => Full::new(Bytes::new()),
        };

        let mut builder = hyper::Request::builder()
            .method(method)
            .uri(uri)
            .header("host", "localhost");

        if body.is_some() {
            builder = builder.header("content-type", "application/json");
        }
        if let Some(auth) = self.auth_header() {
            builder = builder.header("authorization", auth);
        }

        let req = builder
            .body(req_body)
            .map_err(|e| ClientError::Http(e.to_string()))?;

        let resp = sender
            .send_request(req)
            .await
            .map_err(|e| ClientError::Http(e.to_string()))?;

        let status = resp.status().as_u16();
        let body_bytes = resp
            .into_body()
            .collect()
            .await
            .map_err(|e| ClientError::Http(e.to_string()))?
            .to_bytes();
        let body_str = String::from_utf8_lossy(&body_bytes).to_string();

        Ok(RawResponse {
            status,
            body: body_str,
        })
    }

    /// Request a code review for an existing task via `POST /api/v1/tasks/{id}/review`.
    ///
    /// Blocks until the `apollia-review` agent completes (up to 120 s on the server side).
    /// Returns the raw JSON `ReviewReport` value on success.
    pub async fn post_task_review(&self, task_id: &str) -> Result<serde_json::Value, ClientError> {
        let resp = self
            .post(&format!("/api/v1/tasks/{task_id}/review"), None)
            .await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    /// Submit a review directly to the `apollia-review` agent via `POST /api/v1/tasks`.
    ///
    /// Used when the caller supplies a PR number or diff file path rather than an
    /// existing task ID.  The `inputs` value is forwarded as a `data` part in the
    /// AIP input so the agent can extract `pr_number` / `diff_file`.
    pub async fn submit_review(
        &self,
        inputs: serde_json::Value,
    ) -> Result<serde_json::Value, ClientError> {
        let body = serde_json::json!({
            "agent_id": "apollia-review",
            "input": {
                "parts": [{ "type": "data", "data": inputs }]
            }
        });
        let resp = self.post("/api/v1/tasks", Some(&body)).await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    // ─── LLM Backends CRUD ────────────────────────────────────────────────────

    /// List all configured LLM backends via `GET /api/v1/llm/backends`.
    pub async fn list_llm_backends(&self) -> Result<serde_json::Value, ClientError> {
        let resp = self.get("/api/v1/llm/backends").await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    /// Fetch a single LLM backend via `GET /api/v1/llm/backends/{name}`.
    ///
    /// Returns the full configuration object including `provider`, `model`,
    /// `config_json`, `enabled`, and `is_default`. Used by `llm backends show`
    /// and by the CLI update path to merge partial flags with the existing
    /// state before re-submitting a full body to the route (`PUT` is replace,
    /// not patch).
    pub async fn get_llm_backend(&self, name: &str) -> Result<serde_json::Value, ClientError> {
        let resp = self.get(&format!("/api/v1/llm/backends/{name}")).await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    /// Create a new LLM backend via `POST /api/v1/llm/backends`.
    pub async fn create_llm_backend(
        &self,
        body: &serde_json::Value,
    ) -> Result<serde_json::Value, ClientError> {
        let resp = self.post("/api/v1/llm/backends", Some(body)).await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    /// Update an existing LLM backend via `PUT /api/v1/llm/backends/{name}`.
    pub async fn update_llm_backend(
        &self,
        name: &str,
        body: &serde_json::Value,
    ) -> Result<serde_json::Value, ClientError> {
        let uri = format!("/api/v1/llm/backends/{name}");
        let resp = self.put(&uri, Some(body)).await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    /// Delete an LLM backend via `DELETE /api/v1/llm/backends/{name}`.
    pub async fn delete_llm_backend(&self, name: &str) -> Result<serde_json::Value, ClientError> {
        let resp = self.delete(&format!("/api/v1/llm/backends/{name}")).await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    /// Rebuild the in-memory `LlmRouter` from `system.db` via
    /// `POST /api/v1/llm/reload`.
    ///
    /// Returns the list of backends that came up in the new router so the
    /// CLI can confirm the swap without a follow-up `status` call.
    pub async fn reload_llm_router(&self) -> Result<serde_json::Value, ClientError> {
        let resp = self.post("/api/v1/llm/reload", None).await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    /// Set a backend as the default LLM backend via `POST /api/v1/llm/backends/{name}/set-default`.
    pub async fn set_default_llm_backend(
        &self,
        name: &str,
    ) -> Result<serde_json::Value, ClientError> {
        let resp = self
            .post(&format!("/api/v1/llm/backends/{name}/set-default"), None)
            .await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    /// Get aggregated LLM usage and costs via `GET /api/v1/llm/costs`.
    pub async fn get_llm_costs(&self) -> Result<serde_json::Value, ClientError> {
        let resp = self.get("/api/v1/llm/costs").await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    // ─── Triggers CRUD ────────────────────────────────────────────────────────

    /// Create a new trigger via `POST /api/v1/triggers`.
    pub async fn create_trigger(
        &self,
        body: &serde_json::Value,
    ) -> Result<serde_json::Value, ClientError> {
        let resp = self.post("/api/v1/triggers", Some(body)).await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    /// Update an existing trigger via `PUT /api/v1/triggers/{id}`.
    pub async fn update_trigger(
        &self,
        id: &str,
        body: &serde_json::Value,
    ) -> Result<serde_json::Value, ClientError> {
        let uri = format!("/api/v1/triggers/{id}");
        let resp = self.put(&uri, Some(body)).await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    /// Delete a trigger via `DELETE /api/v1/triggers/{id}`.
    pub async fn delete_trigger(&self, id: &str) -> Result<serde_json::Value, ClientError> {
        let resp = self.delete(&format!("/api/v1/triggers/{id}")).await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    // ─── Notifications CRUD ───────────────────────────────────────────────────

    /// Create a notification channel via `POST /api/v1/notifications/channels`.
    pub async fn create_notification_channel(
        &self,
        body: &serde_json::Value,
    ) -> Result<serde_json::Value, ClientError> {
        let resp = self
            .post("/api/v1/notifications/channels", Some(body))
            .await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    /// Update a notification channel via `PUT /api/v1/notifications/channels/{id}`.
    pub async fn update_notification_channel(
        &self,
        id: &str,
        body: &serde_json::Value,
    ) -> Result<serde_json::Value, ClientError> {
        let uri = format!("/api/v1/notifications/channels/{id}");
        let resp = self.put(&uri, Some(body)).await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    /// Delete a notification channel via `DELETE /api/v1/notifications/channels/{id}`.
    pub async fn delete_notification_channel(
        &self,
        id: &str,
    ) -> Result<serde_json::Value, ClientError> {
        let resp = self
            .delete(&format!("/api/v1/notifications/channels/{id}"))
            .await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    /// Get notification event types configuration via `GET /api/v1/notifications/events`.
    pub async fn get_notification_events(&self) -> Result<serde_json::Value, ClientError> {
        let resp = self.get("/api/v1/notifications/events").await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    /// Update notification event types via `PUT /api/v1/notifications/events`.
    pub async fn set_notification_events(
        &self,
        body: &serde_json::Value,
    ) -> Result<serde_json::Value, ClientError> {
        let resp = self.put("/api/v1/notifications/events", Some(body)).await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    // ─── Pipelines CRUD ───────────────────────────────────────────────────────

    /// Create a new pipeline via `POST /api/v1/pipelines`.
    pub async fn create_pipeline(
        &self,
        body: &serde_json::Value,
    ) -> Result<serde_json::Value, ClientError> {
        let resp = self.post("/api/v1/pipelines", Some(body)).await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    /// Get pipeline details via `GET /api/v1/pipelines/{id}`.
    pub async fn get_pipeline(&self, id: &str) -> Result<serde_json::Value, ClientError> {
        let resp = self.get(&format!("/api/v1/pipelines/{id}")).await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    /// Update a pipeline via `PUT /api/v1/pipelines/{id}`.
    pub async fn update_pipeline(
        &self,
        id: &str,
        body: &serde_json::Value,
    ) -> Result<serde_json::Value, ClientError> {
        let uri = format!("/api/v1/pipelines/{id}");
        let resp = self.put(&uri, Some(body)).await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    /// Delete a pipeline via `DELETE /api/v1/pipelines/{id}`.
    pub async fn delete_pipeline(&self, id: &str) -> Result<serde_json::Value, ClientError> {
        let resp = self.delete(&format!("/api/v1/pipelines/{id}")).await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    // ─── MCP Servers CRUD ─────────────────────────────────────────────────────

    /// Add an MCP server to the runtime via `POST /api/v1/mcp/servers`.
    pub async fn add_mcp_server(
        &self,
        body: &serde_json::Value,
    ) -> Result<serde_json::Value, ClientError> {
        let resp = self.post("/api/v1/mcp/servers", Some(body)).await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    /// Get MCP server details via `GET /api/v1/mcp/servers/{name}`.
    pub async fn get_mcp_server_detail(
        &self,
        name: &str,
    ) -> Result<serde_json::Value, ClientError> {
        let resp = self.get(&format!("/api/v1/mcp/servers/{name}")).await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    /// Remove an MCP server from the runtime via `DELETE /api/v1/mcp/servers/{name}`.
    pub async fn remove_mcp_server(&self, name: &str) -> Result<serde_json::Value, ClientError> {
        let resp = self.delete(&format!("/api/v1/mcp/servers/{name}")).await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    /// Test an MCP server connection via `POST /api/v1/mcp/servers/test`.
    pub async fn test_mcp_connection(
        &self,
        body: &serde_json::Value,
    ) -> Result<serde_json::Value, ClientError> {
        let resp = self.post("/api/v1/mcp/servers/test", Some(body)).await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    /// Test an already-installed MCP server by name via
    /// `POST /api/v1/mcp/servers/{name}/test`. Returns `404` for an unknown
    /// server (the name-based route is existence-aware).
    pub async fn test_live_mcp_server(&self, name: &str) -> Result<serde_json::Value, ClientError> {
        let body = serde_json::json!({});
        let resp = self
            .post(&format!("/api/v1/mcp/servers/{name}/test"), Some(&body))
            .await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    /// Restart an MCP server via `POST /api/v1/mcp/servers/{name}/restart`.
    pub async fn restart_mcp_server(&self, name: &str) -> Result<serde_json::Value, ClientError> {
        let resp = self
            .post(&format!("/api/v1/mcp/servers/{name}/restart"), None)
            .await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    /// Update an MCP server configuration via `PUT /api/v1/mcp/servers/{name}/config`.
    pub async fn update_mcp_server_config(
        &self,
        name: &str,
        body: &serde_json::Value,
    ) -> Result<serde_json::Value, ClientError> {
        let uri = format!("/api/v1/mcp/servers/{name}/config");
        let resp = self.put(&uri, Some(body)).await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    // ─── STT Config ───────────────────────────────────────────────────────────

    /// Get the STT configuration via `GET /api/v1/stt/config`.
    pub async fn get_stt_config(&self) -> Result<serde_json::Value, ClientError> {
        let resp = self.get("/api/v1/stt/config").await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    /// Update the STT configuration via `PUT /api/v1/stt/config`.
    pub async fn update_stt_config(
        &self,
        body: &serde_json::Value,
    ) -> Result<serde_json::Value, ClientError> {
        let resp = self.put("/api/v1/stt/config", Some(body)).await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    /// Delete a transcription by ID via `DELETE /api/v1/stt/transcriptions/{id}`.
    pub async fn delete_transcription(&self, id: &str) -> Result<serde_json::Value, ClientError> {
        let resp = self
            .delete(&format!("/api/v1/stt/transcriptions/{id}"))
            .await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        // The runtime returns 204 No Content (empty body) on success (delete is
        // idempotent), so an empty body is success, not malformed JSON.
        if resp.body.trim().is_empty() {
            return Ok(serde_json::json!({ "deleted": id }));
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    // ─── Audit ────────────────────────────────────────────────────────────────

    /// Get audit statistics via `GET /api/v1/audit/stats`.
    pub async fn get_audit_stats(&self) -> Result<serde_json::Value, ClientError> {
        let resp = self.get("/api/v1/audit/stats").await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    // ─── Approvals ────────────────────────────────────────────────────────────

    /// List resolved HITL approvals via `GET /api/v1/approvals/resolved`.
    pub async fn list_resolved_approvals(&self) -> Result<serde_json::Value, ClientError> {
        let resp = self.get("/api/v1/approvals/resolved").await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    // ─── Agent Logs ───────────────────────────────────────────────────────────

    /// Fetch recent log lines for an agent via `GET /api/v1/agents/{id}/logs?last={n}`.
    ///
    /// Returns a JSON value with a `"logs"` array of log line strings.
    pub async fn get_agent_logs(
        &self,
        agent_id: &str,
        last: u32,
    ) -> Result<serde_json::Value, ClientError> {
        let resp = self
            .get(&format!("/api/v1/agents/{agent_id}/logs?last={last}"))
            .await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    // ─── Resilience ───────────────────────────────────────────────────────────

    /// List all circuit breakers via `GET /api/v1/resilience/status`.
    ///
    /// Returns `{ "circuit_breakers": [ { tool_name, state, failure_count,
    /// cooldown_remaining_secs } ] }`.
    pub async fn resilience_list(&self) -> Result<serde_json::Value, ClientError> {
        let resp = self.get("/api/v1/resilience/status").await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    /// Show the circuit breaker state for one tool via `GET /api/v1/resilience/status/{tool}`.
    ///
    /// Returns 404 when the tool has no recorded circuit breaker yet.
    pub async fn resilience_show(&self, tool_name: &str) -> Result<serde_json::Value, ClientError> {
        let resp = self
            .get(&format!(
                "/api/v1/resilience/status/{}",
                encode_path_segment(tool_name)
            ))
            .await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    /// Reset a circuit breaker to CLOSED via `POST /api/v1/resilience/reset/{tool}`.
    ///
    /// Returns 404 when the tool has no recorded circuit breaker yet.
    pub async fn resilience_reset(
        &self,
        tool_name: &str,
    ) -> Result<serde_json::Value, ClientError> {
        let resp = self
            .post(
                &format!(
                    "/api/v1/resilience/reset/{}",
                    encode_path_segment(tool_name)
                ),
                None,
            )
            .await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    /// Add a permission prefix rule via `POST /api/v1/permissions/prefix`.
    pub async fn add_permission_prefix_rule(
        &self,
        prefix: &str,
        action: &str,
    ) -> Result<serde_json::Value, ClientError> {
        let body = serde_json::json!({ "prefix": prefix, "action": action });
        let resp = self.post("/api/v1/permissions/prefix", Some(&body)).await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }
}

/// Reads HTTP body frames from an SSE response, accumulates a line buffer, and
/// pushes complete lines (stripped of trailing '\r') to `tx`. Exits when the
/// connection closes or when the receiver is dropped.
async fn pump_sse_body(
    body: hyper::body::Incoming,
    mut tx: mpsc::Sender<Result<String, ClientError>>,
) {
    let mut buffer = String::new();
    let mut pinned_body = Box::pin(body);

    loop {
        let frame_opt =
            std::future::poll_fn(|cx| hyper::body::Body::poll_frame(pinned_body.as_mut(), cx))
                .await;

        match frame_opt {
            None => break, // Server closed the connection
            Some(Err(e)) => {
                let _ = tx.send(Err(ClientError::Http(e.to_string()))).await;
                return;
            }
            Some(Ok(frame)) => {
                let Ok(data) = frame.into_data() else {
                    continue; // Skip HTTP trailers
                };
                buffer.push_str(&String::from_utf8_lossy(&data));
                if drain_sse_lines(&mut buffer, &mut tx).await.is_err() {
                    return; // Receiver dropped, stop reading
                }
            }
        }
    }

    // Flush any remaining content that arrived without a trailing newline.
    if !buffer.is_empty() {
        let _ = tx.send(Ok(std::mem::take(&mut buffer))).await;
    }
}

/// Drains all complete ('\n'-terminated) lines from `buffer`, sending each to
/// `tx`. Returns `Err(())` if the receiver has been dropped.
async fn drain_sse_lines(
    buffer: &mut String,
    tx: &mut mpsc::Sender<Result<String, ClientError>>,
) -> Result<(), ()> {
    while let Some(pos) = buffer.find('\n') {
        let line = buffer[..pos].trim_end_matches('\r').to_string();
        buffer.drain(..=pos);
        if tx.send(Ok(line)).await.is_err() {
            return Err(());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_runtime_client_connection_refused() {
        // GIVEN a nonexistent socket
        let client = RuntimeClient::new(PathBuf::from("/tmp/apollia-test-nonexistent.sock"));

        // WHEN health() is called
        let result = client.health().await;

        // THEN connection refused error
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, ClientError::ConnectionRefused),
            "expected ConnectionRefused, got: {err}"
        );
    }

    #[test]
    fn test_default_socket_path() {
        let client = RuntimeClient::default_client();
        assert_eq!(client.socket_path(), default_socket_path().as_path());
    }

    #[test]
    fn test_custom_socket_path() {
        let client = RuntimeClient::new(PathBuf::from("/custom/path.sock"));
        assert_eq!(client.socket_path(), Path::new("/custom/path.sock"));
    }
}
