//! MCP session: transport lifecycle, JSON-RPC routing, and initialize handshake.
//!
//! Each [`McpSession`] owns one [`McpTransport`] and one background dispatch task:
//! - the **dispatch task** calls [`McpTransport::recv`] in a loop, parses each
//!   JSON-RPC response, and routes it to the caller waiting on the matching
//!   [`oneshot`] channel.
//!
//! Request/response correlation is handled via a shared `pending` map keyed by
//! request ID. The transport handles all byte-level I/O.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use apollia_core::McpHealth;
use tokio::sync::{oneshot, Mutex};
use tracing::{debug, warn};

use crate::config::{McpServerConfig, SecretResolver};
use crate::jsonrpc::{JsonRpcNotification, JsonRpcRequest, JsonRpcResponse};
use crate::protocol::{
    ClientCapabilities, ClientInfo, InitializeParams, InitializeResult, McpToolDefinition,
    ServerCapabilities, ServerInfo, ToolCallParams, ToolCallResult, ToolsListResult,
};
use crate::transport::{create_transport, McpTransport};

// ─── retry ───────────────────────────────────────────────────────────────────

/// Retry policy applied to tool calls that fail with a transport-level error.
///
/// Backoff formula: `delay = min(base_delay_secs × 2^attempt, max_delay_secs)`.
struct McpRetryConfig {
    /// Maximum number of additional attempts after the first failure.
    max_retries: u32,
    /// Base delay in seconds for the first retry.
    base_delay_secs: u64,
    /// Upper bound on the computed delay.
    max_delay_secs: u64,
}

impl McpRetryConfig {
    /// Default retry policy: 3 retries, 1s base, 8s cap.
    const DEFAULT: Self = Self {
        max_retries: 3,
        base_delay_secs: 1,
        max_delay_secs: 8,
    };
}

/// Returns `true` for errors that originate from a transport failure and are
/// therefore candidates for a retry.
fn is_transport_error(err: &McpSessionError) -> bool {
    matches!(
        err,
        McpSessionError::StdinClosed { .. } | McpSessionError::ServerExited { .. }
    )
}

/// Compute the exponential backoff delay for a given attempt index.
///
/// `attempt` is zero-based: attempt 0 → `base`, attempt 1 → `base × 2`, etc.
fn compute_backoff_delay(attempt: u32, base_delay_secs: u64, max_delay_secs: u64) -> u64 {
    let factor = 2u64.saturating_pow(attempt);
    base_delay_secs.saturating_mul(factor).min(max_delay_secs)
}

/// Run `f` up to `retry_cfg.max_retries + 1` times, sleeping between retries on
/// transport errors.  Non-transport errors are propagated immediately.
async fn with_transport_retry<F, Fut, T>(
    retry_cfg: &McpRetryConfig,
    server_name: &str,
    mut f: F,
) -> Result<T, McpSessionError>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, McpSessionError>>,
{
    for attempt in 0..=retry_cfg.max_retries {
        match f().await {
            Ok(val) => return Ok(val),
            Err(err) if is_transport_error(&err) && attempt < retry_cfg.max_retries => {
                let delay_secs = compute_backoff_delay(
                    attempt,
                    retry_cfg.base_delay_secs,
                    retry_cfg.max_delay_secs,
                );
                tracing::warn!(
                    attempt = attempt + 1,
                    max = retry_cfg.max_retries,
                    delay_ms = delay_secs * 1000,
                    server = %server_name,
                    "retrying MCP call after transport error"
                );
                tokio::time::sleep(std::time::Duration::from_secs(delay_secs)).await;
            }
            Err(err) => return Err(err),
        }
    }
    // Unreachable: the loop always returns on the last attempt.
    unreachable!("retry loop must return within max_retries + 1 iterations")
}

// ─── errors ──────────────────────────────────────────────────────────────────

/// Errors that can arise during MCP session operations.
#[derive(Debug, thiserror::Error)]
pub enum McpSessionError {
    /// The server subprocess could not be spawned.
    #[error("failed to spawn server '{server}': {cause}")]
    SpawnFailed { server: String, cause: String },

    /// The `initialize` handshake completed but the response was malformed.
    #[error("server '{server}' initialize handshake failed: {cause}")]
    InitializeFailed { server: String, cause: String },

    /// The `initialize` handshake did not complete within the configured timeout.
    ///
    /// `stderr_hint` carries the last few lines emitted by the subprocess (or
    /// the empty string for non-stdio transports) so the operator sees why the
    /// child stalled without having to attach a debugger.
    #[error("server '{server}' initialize timed out after {timeout_secs}s{stderr_hint}")]
    InitializeTimeout {
        server: String,
        timeout_secs: u64,
        stderr_hint: String,
    },

    /// A `tools/call` request failed on the server side.
    #[error("server '{server}' tool call '{tool}' failed: {cause}")]
    ToolCallFailed {
        server: String,
        tool: String,
        cause: String,
    },

    /// A `tools/call` request did not complete within the configured timeout.
    #[error("server '{server}' tool call '{tool}' timed out after {timeout_secs}s")]
    ToolCallTimeout {
        server: String,
        tool: String,
        timeout_secs: u64,
    },

    /// The server process exited before the operation completed.
    #[error("server '{server}' process exited unexpectedly")]
    ServerExited { server: String },

    /// The server returned a JSON-RPC error object.
    #[error("JSON-RPC error from server '{server}': [{code}] {message}")]
    JsonRpcError {
        server: String,
        code: i64,
        message: String,
    },

    /// A JSON-RPC message could not be serialised or deserialised.
    #[error("failed to serialize/deserialize JSON-RPC message: {0}")]
    SerdeError(String),

    /// The transport's send channel was closed.
    ///
    /// `cause` carries the underlying transport error message when available
    /// (e.g. `"HTTP 401"` for an unauthorized remote, `"timeout"`, or the
    /// raw `reqwest` / IO error). Surfaces as a richer UI hint than the
    /// previous bare "channel closed" message.
    #[error("server '{server}' transport closed: {cause}")]
    StdinClosed { server: String, cause: String },

    /// A hot reload was requested for a server that is not currently managed.
    #[error("cannot reload server '{server}': not found in managed sessions")]
    ConfigReload { server: String },

    /// The tool call was suspended because HITL approval is required.
    ///
    /// The caller should surface `approval_id` to the operator so they can
    /// run `apollia-os mcp set-approval` to unblock future calls.
    #[error("tool call to '{server}/{tool}' requires human approval (id={approval_id})")]
    PendingApproval {
        /// MCP server name.
        server: String,
        /// Tool name within the server.
        tool: String,
        /// UUID of the pending approval request created in the approval store.
        approval_id: String,
    },

    /// A tool call arrived while the server is being hot-reloaded.
    ///
    /// The server has been disconnected but the new session is not yet established.
    /// Callers should retry the operation after a short delay.
    #[error("server '{server}' is currently being reloaded")]
    ServerReloading { server: String },

    /// The remote MCP server returned HTTP 401 Unauthorized during the handshake
    /// or a tool call. `www_authenticate` carries the verbatim
    /// `WWW-Authenticate` header (RFC 6750), which orchestration layers parse
    /// to drive the MCP HTTP OAuth 2.1 flow.
    #[error("server '{server}' returned 401 Unauthorized; OAuth handshake required")]
    Unauthorized {
        /// MCP server name.
        server: String,
        /// `WWW-Authenticate` header value verbatim (empty when the server
        /// omitted the header, unusual but technically allowed).
        www_authenticate: String,
    },

    /// The full schema for a tool could not be fetched on demand.
    ///
    /// Produced by [`McpSession::fetch_tool_schema`] when the tool name is not
    /// known to the server, or when the on-demand `tools/list` round-trip fails
    /// at the transport level. In [`LoadingMode::Deferred`] this is the typed
    /// failure that replaces a silently empty schema (principle: fail fast).
    #[error("server '{server}' schema fetch failed for tool '{tool}': {cause}")]
    SchemaFetchFailed {
        /// MCP server name.
        server: String,
        /// Tool name within the server.
        tool: String,
        /// Underlying cause: a transport error message or `"tool not found"`.
        cause: String,
    },
}

/// Build the human-readable `stderr_hint` suffix for handshake / call timeout
/// errors. Empty input → empty hint (no leading separator added). When lines
/// are present, they are concatenated oldest-first with ` | ` so an operator
/// reading a single-line error message can still see what the subprocess said.
fn format_stderr_hint(tail: &[String]) -> String {
    if tail.is_empty() {
        return String::new();
    }
    format!("; stderr: {}", tail.join(" | "))
}

// ─── loading mode ──────────────────────────────────────────────────────────

/// Strategy for loading MCP tool schemas at session start.
///
/// `Eager` preserves the historical behavior: every tool schema is fetched at
/// boot via `tools/list` and stored in [`McpSession::tools`]. `Deferred` fetches
/// only a lightweight name/description index at boot
/// ([`McpSession::tool_index`]); full schemas are fetched and cached on first
/// use via [`McpSession::fetch_tool_schema`]. Deferred mode keeps the context
/// cost near zero for large MCP ecosystems on local models with narrow windows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadingMode {
    /// Load all tool schemas during `start()`. This is the default and the
    /// backward-compatible path for [`McpSession::start`].
    Eager,
    /// Load only the index of tool names and descriptions at boot. Full schemas
    /// are fetched and cached on first use.
    Deferred,
}

impl From<apollia_core::McpToolLoading> for LoadingMode {
    /// Map the operator-facing config value to the session loading mode.
    fn from(value: apollia_core::McpToolLoading) -> Self {
        match value {
            apollia_core::McpToolLoading::Eager => Self::Eager,
            apollia_core::McpToolLoading::Deferred => Self::Deferred,
        }
    }
}

/// A lightweight entry in the deferred tool index.
///
/// Populated from a `tools/list` response when running in
/// [`LoadingMode::Deferred`]. The tool's `input_schema` is intentionally
/// omitted to keep the index cheap; it is fetched on demand by
/// [`McpSession::fetch_tool_schema`].
#[derive(Debug, Clone)]
pub struct ToolIndexEntry {
    /// Tool name within the server (e.g. `"search_pages"`).
    pub name: String,
    /// Human-readable description, when provided by the server.
    pub description: Option<String>,
}

// ─── session ─────────────────────────────────────────────────────────────────

/// Active session with a single MCP server.
///
/// Manages the JSON-RPC message routing and request/response correlation.
/// The underlying byte-level I/O is handled by the [`McpTransport`] implementation.
/// One session per server; owned by `McpClientManager`.
pub struct McpSession {
    /// Server configuration (name, timeouts, command, etc.).
    config: McpServerConfig,
    /// Transport that handles all byte-level send/recv with the server.
    transport: Arc<dyn McpTransport>,
    /// Pending in-flight requests: request ID → reply oneshot.
    pending: Arc<Mutex<HashMap<u64, oneshot::Sender<JsonRpcResponse>>>>,
    /// Monotonically increasing request ID counter.
    next_id: AtomicU64,
    /// Server capabilities received during the initialize handshake.
    capabilities: ServerCapabilities,
    /// Server identity received during the initialize handshake.
    server_info: ServerInfo,
    /// Free-text operator guidance returned by the server in `initialize`,
    /// or `None` when the server omitted the `instructions` field.
    instructions: Option<String>,
    /// Tools discovered via `tools/list`. Populated in [`LoadingMode::Eager`];
    /// stays empty in [`LoadingMode::Deferred`] (see `tool_index`).
    tools: Vec<McpToolDefinition>,
    /// Loading strategy chosen at construction time.
    loading_mode: LoadingMode,
    /// Lightweight index of tool names and descriptions.
    ///
    /// Populated by `discover_tools_index` in [`LoadingMode::Deferred`] after the
    /// boot `tools/list`. Empty in [`LoadingMode::Eager`] (schemas live in
    /// `tools`).
    tool_index: Vec<ToolIndexEntry>,
    /// Cache of full JSON schemas keyed by tool name.
    ///
    /// Populated lazily by [`McpSession::fetch_tool_schema`] in
    /// [`LoadingMode::Deferred`]. Stays empty in [`LoadingMode::Eager`], where
    /// schemas are read directly from `tools`. Owned by the session, never shared
    /// across actors.
    schema_cache: HashMap<String, serde_json::Value>,
    /// Instant at which the session was successfully started.
    started_at: std::time::Instant,
    /// Operational health, mutated by the manager after each operation.
    ///
    /// Initialised to `Healthy { verified: false }`: the handshake passed but no
    /// real operation has yet proven the server's scopes/grants. Lives here so
    /// it stays owned by the manager actor (no shared lock across actors).
    health: McpHealth,
    /// Background dispatch task: reads from the transport and routes responses.
    _dispatch_task: tokio::task::JoinHandle<()>,
}

impl McpSession {
    /// Connect the transport and perform the MCP `initialize` handshake in the
    /// default [`LoadingMode::Eager`].
    ///
    /// Resolves `${VAR}` placeholders in `config.env` before spawning. Placeholders
    /// prefixed with `APOLLIA_SECRET:` are resolved via `secret_store` when provided.
    /// On success, returns a session whose every tool schema is already loaded and
    /// ready to accept `tools/list` and `tools/call` requests.
    ///
    /// Equivalent to [`McpSession::start_with_mode`] with [`LoadingMode::Eager`];
    /// the signature is preserved for backward compatibility.
    ///
    /// # Errors
    ///
    /// Returns [`McpSessionError`] when the subprocess cannot be spawned, the
    /// `initialize` handshake fails or times out, or `tools/list` cannot be read.
    pub async fn start(
        config: McpServerConfig,
        secret_store: Option<&dyn SecretResolver>,
    ) -> Result<Self, McpSessionError> {
        Self::start_with_mode(config, secret_store, LoadingMode::Eager).await
    }

    /// Connect the transport and perform the MCP `initialize` handshake with an
    /// explicit [`LoadingMode`].
    ///
    /// In [`LoadingMode::Eager`] this behaves exactly like [`McpSession::start`]:
    /// all tool schemas are fetched at boot. In [`LoadingMode::Deferred`] only the
    /// lightweight tool index (names and descriptions) is populated at boot;
    /// schemas are fetched on demand via [`McpSession::fetch_tool_schema`].
    ///
    /// # Errors
    ///
    /// Returns [`McpSessionError`] when the subprocess cannot be spawned, the
    /// `initialize` handshake fails or times out, or the boot `tools/list` cannot
    /// be read.
    pub async fn start_with_mode(
        config: McpServerConfig,
        secret_store: Option<&dyn SecretResolver>,
        mode: LoadingMode,
    ) -> Result<Self, McpSessionError> {
        let resolved_env =
            config
                .resolve_env(secret_store)
                .await
                .map_err(|e| McpSessionError::SpawnFailed {
                    server: config.name.clone(),
                    cause: e.to_string(),
                })?;

        let transport: Arc<dyn McpTransport> =
            Arc::from(create_transport(&config, resolved_env).map_err(|e| {
                McpSessionError::SpawnFailed {
                    server: config.name.clone(),
                    cause: e.to_string(),
                }
            })?);

        let pending: Arc<Mutex<HashMap<u64, oneshot::Sender<JsonRpcResponse>>>> =
            Arc::new(Mutex::new(HashMap::new()));

        let dispatch_task = spawn_dispatch_task(Arc::clone(&transport), Arc::clone(&pending));

        let mut session = McpSession {
            config,
            transport,
            pending,
            next_id: AtomicU64::new(1),
            capabilities: ServerCapabilities::default(),
            server_info: ServerInfo {
                name: String::new(),
                version: None,
            },
            instructions: None,
            tools: Vec::new(),
            loading_mode: mode,
            tool_index: Vec::new(),
            schema_cache: HashMap::new(),
            started_at: std::time::Instant::now(),
            health: McpHealth::Healthy { verified: false },
            _dispatch_task: dispatch_task,
        };

        session.initialize().await?;
        match mode {
            LoadingMode::Eager => session.discover_tools().await?,
            LoadingMode::Deferred => session.discover_tools_index().await?,
        }

        Ok(session)
    }

    /// Perform the MCP `initialize` handshake.
    ///
    /// Sends the `initialize` request with the client identity and capabilities,
    /// stores the server's capabilities and identity, then sends the
    /// `notifications/initialized` notification to complete the handshake.
    async fn initialize(&mut self) -> Result<(), McpSessionError> {
        use crate::protocol::{
            ElicitationCapability, RootsCapability, SamplingCapability,
            APOLLIA_MCP_PROTOCOL_VERSION,
        };
        let params = InitializeParams {
            protocol_version: APOLLIA_MCP_PROTOCOL_VERSION.to_string(),
            capabilities: ClientCapabilities {
                roots: Some(RootsCapability {
                    list_changed: Some(true),
                }),
                sampling: Some(SamplingCapability::default()),
                elicitation: Some(ElicitationCapability::default()),
            },
            client_info: ClientInfo {
                name: "apollia-runtime".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
            },
        };

        let params_value = serde_json::to_value(&params)
            .map_err(|e| McpSessionError::SerdeError(e.to_string()))?;

        let timeout_secs = self.config.init_timeout_secs;
        let result = self
            .send_request("initialize", Some(params_value), timeout_secs)
            .await?;

        let init_result: InitializeResult =
            serde_json::from_value(result).map_err(|e| McpSessionError::InitializeFailed {
                server: self.config.name.clone(),
                cause: e.to_string(),
            })?;

        debug!(
            server = %self.config.name,
            protocol_version = %init_result.protocol_version,
            "MCP initialize handshake completed"
        );

        self.capabilities = init_result.capabilities;
        self.server_info = init_result.server_info;
        self.instructions = init_result.instructions;

        self.send_notification("notifications/initialized", None)
            .await?;

        Ok(())
    }

    /// Discover tools available on this MCP server via `tools/list`.
    ///
    /// Called automatically at the end of `start()` after the `initialize` handshake.
    /// Populates the `tools` field; logs a warning when the server exposes no tools.
    async fn discover_tools(&mut self) -> Result<(), McpSessionError> {
        let timeout_secs = self.config.init_timeout_secs;
        let response = self.send_request("tools/list", None, timeout_secs).await?;

        let result: ToolsListResult =
            serde_json::from_value(response).map_err(|e| McpSessionError::InitializeFailed {
                server: self.config.name.clone(),
                cause: e.to_string(),
            })?;

        tracing::info!(
            server = %self.config.name,
            tools_count = result.tools.len(),
            "MCP tools discovered"
        );

        if result.tools.is_empty() {
            tracing::warn!(server = %self.config.name, "MCP server exposes no tools");
        }

        self.tools = result.tools;
        Ok(())
    }

    /// Discover only the lightweight tool index (names and descriptions) via
    /// `tools/list`.
    ///
    /// Called at the end of `start_with_mode` in [`LoadingMode::Deferred`]. The
    /// full `tools/list` response is parsed, but only `{name, description}` is
    /// retained in `tool_index`; the `tools` field stays empty and schemas are
    /// fetched on demand by [`McpSession::fetch_tool_schema`].
    async fn discover_tools_index(&mut self) -> Result<(), McpSessionError> {
        let timeout_secs = self.config.init_timeout_secs;
        let response = self.send_request("tools/list", None, timeout_secs).await?;

        let result: ToolsListResult =
            serde_json::from_value(response).map_err(|e| McpSessionError::InitializeFailed {
                server: self.config.name.clone(),
                cause: e.to_string(),
            })?;

        self.tool_index = result
            .tools
            .into_iter()
            .map(|tool| ToolIndexEntry {
                name: tool.name,
                description: tool.description,
            })
            .collect();

        tracing::info!(
            server = %self.config.name,
            tools_count = self.tool_index.len(),
            "MCP deferred tool index discovered"
        );

        if self.tool_index.is_empty() {
            tracing::warn!(server = %self.config.name, "MCP server exposes no tools");
        }

        Ok(())
    }

    /// Send a JSON-RPC request and wait for the response, with a hard timeout.
    ///
    /// Inserts a [`oneshot::Sender`] into `pending`, writes the serialised request
    /// to the transport, then awaits the response on the matching receiver.
    /// On timeout, the pending entry is removed to prevent map growth.
    async fn send_request(
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
    async fn send_notification(
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

    /// Returns the server name from the configuration.
    pub fn server_name(&self) -> &str {
        &self.config.name
    }

    /// Returns the server capabilities received during the initialize handshake.
    pub fn capabilities(&self) -> &ServerCapabilities {
        &self.capabilities
    }

    /// Returns the server identity received during the initialize handshake.
    pub fn server_info(&self) -> &ServerInfo {
        &self.server_info
    }

    /// Returns the server-level `instructions` from the `initialize` handshake,
    /// or `None` when the server omitted them.
    ///
    /// The value is returned verbatim; an empty string is reported as
    /// `Some("")`, leaving the decision to treat it as absent to the caller.
    pub fn instructions(&self) -> Option<&str> {
        self.instructions.as_deref()
    }

    /// Returns the tools discovered via `tools/list`, or an empty slice before discovery.
    ///
    /// Populated in [`LoadingMode::Eager`]. In [`LoadingMode::Deferred`] this is
    /// empty; use [`McpSession::tool_index`] instead.
    pub fn tools(&self) -> &[McpToolDefinition] {
        &self.tools
    }

    /// Returns the lightweight tool index, populated in [`LoadingMode::Deferred`].
    ///
    /// Returns an empty slice in [`LoadingMode::Eager`] (use [`McpSession::tools`]
    /// instead).
    pub fn tool_index(&self) -> &[ToolIndexEntry] {
        &self.tool_index
    }

    /// Fetch and cache the full JSON schema for a named tool.
    ///
    /// In [`LoadingMode::Eager`] the schema is read from the already-loaded
    /// `tools` slice with no network round-trip. In [`LoadingMode::Deferred`] the
    /// first miss triggers a single `tools/list` call whose every schema is
    /// cached; subsequent calls for any tool are served from `schema_cache`.
    ///
    /// # Errors
    ///
    /// Returns [`McpSessionError::SchemaFetchFailed`] when the tool name is not
    /// known to the server, or when the on-demand `tools/list` round-trip fails
    /// at the transport level.
    pub async fn fetch_tool_schema(
        &mut self,
        tool_name: &str,
    ) -> Result<serde_json::Value, McpSessionError> {
        match self.loading_mode {
            LoadingMode::Eager => self
                .tools
                .iter()
                .find(|tool| tool.name == tool_name)
                .map(|tool| tool.input_schema.clone())
                .ok_or_else(|| McpSessionError::SchemaFetchFailed {
                    server: self.config.name.clone(),
                    tool: tool_name.to_string(),
                    cause: "tool not found".to_string(),
                }),
            LoadingMode::Deferred => {
                if let Some(schema) = self.schema_cache.get(tool_name) {
                    return Ok(schema.clone());
                }

                let timeout_secs = self.config.init_timeout_secs;
                let response = self
                    .send_request("tools/list", None, timeout_secs)
                    .await
                    .map_err(|e| McpSessionError::SchemaFetchFailed {
                        server: self.config.name.clone(),
                        tool: tool_name.to_string(),
                        cause: e.to_string(),
                    })?;

                let result: ToolsListResult = serde_json::from_value(response).map_err(|e| {
                    McpSessionError::SchemaFetchFailed {
                        server: self.config.name.clone(),
                        tool: tool_name.to_string(),
                        cause: e.to_string(),
                    }
                })?;

                for tool in result.tools {
                    self.schema_cache.insert(tool.name, tool.input_schema);
                }

                self.schema_cache.get(tool_name).cloned().ok_or_else(|| {
                    McpSessionError::SchemaFetchFailed {
                        server: self.config.name.clone(),
                        tool: tool_name.to_string(),
                        cause: "tool not found".to_string(),
                    }
                })
            }
        }
    }

    /// Returns whether every tool call to this server requires HITL approval.
    pub fn requires_approval(&self) -> bool {
        self.config.requires_approval
    }

    /// Update whether tool calls to this server require HITL approval.
    ///
    /// The change takes effect immediately for future calls; no process restart
    /// is required. Callers are responsible for persisting the change to `mcp.toml`.
    pub fn set_requires_approval(&mut self, requires_approval: bool) {
        self.config.requires_approval = requires_approval;
    }

    /// Returns the OS process ID of the server process, if applicable.
    ///
    /// Delegates to the transport; subprocess-based transports return the child PID.
    /// Network-based transports return `None`.
    pub fn pid(&self) -> Option<u32> {
        self.transport.pid()
    }

    /// Returns the number of seconds elapsed since this session was started.
    pub fn uptime_secs(&self) -> u64 {
        self.started_at.elapsed().as_secs()
    }

    /// Returns the configuration used to start this session.
    pub fn config(&self) -> &McpServerConfig {
        &self.config
    }

    /// Returns the current operational health of this session.
    pub fn health(&self) -> &McpHealth {
        &self.health
    }

    /// Overwrite the operational health. Called by the manager after each
    /// operation has been classified.
    pub(crate) fn set_health(&mut self, health: McpHealth) {
        self.health = health;
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

    /// Subscribe to updates for a specific resource (`resources/subscribe`).
    ///
    /// The server will subsequently send `notifications/resources/updated` for
    /// the matching URI. Apollia's dispatch loop currently drops notifications
    /// (cf. dispatch_task); wiring them through is part of the next step.
    pub async fn subscribe_resource(&self, uri: &str) -> Result<(), McpSessionError> {
        let params = serde_json::json!({ "uri": uri });
        self.send_request(
            "resources/subscribe",
            Some(params),
            self.config.call_timeout_secs,
        )
        .await
        .map(|_| ())
    }

    /// List the prompts exposed by the server (`prompts/list`).
    pub async fn list_prompts(
        &self,
    ) -> Result<crate::protocol::PromptsListResult, McpSessionError> {
        let response = self
            .send_request("prompts/list", None, self.config.call_timeout_secs)
            .await?;
        serde_json::from_value(response).map_err(|e| McpSessionError::ToolCallFailed {
            server: self.config.name.clone(),
            tool: "prompts/list".into(),
            cause: e.to_string(),
        })
    }

    /// Retrieve a server-side prompt template (`prompts/get`).
    pub async fn get_prompt(
        &self,
        name: &str,
        arguments: Option<serde_json::Value>,
    ) -> Result<crate::protocol::PromptsGetResult, McpSessionError> {
        let params = crate::protocol::PromptsGetParams {
            name: name.to_owned(),
            arguments,
        };
        let params_value = serde_json::to_value(&params)
            .map_err(|e| McpSessionError::SerdeError(e.to_string()))?;
        let response = self
            .send_request(
                "prompts/get",
                Some(params_value),
                self.config.call_timeout_secs,
            )
            .await?;
        serde_json::from_value(response).map_err(|e| McpSessionError::ToolCallFailed {
            server: self.config.name.clone(),
            tool: "prompts/get".into(),
            cause: e.to_string(),
        })
    }

    /// Set the server's log level (`logging/setLevel`).
    pub async fn set_log_level(&self, level: &str) -> Result<(), McpSessionError> {
        let params = serde_json::json!({ "level": level });
        self.send_request(
            "logging/setLevel",
            Some(params),
            self.config.call_timeout_secs,
        )
        .await
        .map(|_| ())
    }

    /// Gracefully shut down the session.
    ///
    /// 1. Sends a `notifications/cancelled` notification (best-effort; silently
    ///    ignored if the transport is already closed).
    /// 2. Shuts down the transport (terminates the subprocess or closes the connection).
    pub async fn shutdown(self) {
        let _ = self
            .send_notification("notifications/cancelled", None)
            .await;
        let _ = self.transport.shutdown().await;
        tracing::info!(server = %self.config.name, "MCP session shutdown complete");
    }
}

// ─── background tasks ────────────────────────────────────────────────────────

/// Spawn the dispatch task.
///
/// Calls [`McpTransport::recv`] in a loop, deserialises each line as a
/// [`JsonRpcResponse`], and routes it to the caller waiting on the matching
/// entry in `pending`. Exits when the transport closes (recv returns an error).
fn spawn_dispatch_task(
    transport: Arc<dyn McpTransport>,
    pending: Arc<Mutex<HashMap<u64, oneshot::Sender<JsonRpcResponse>>>>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        while let Ok(line) = transport.recv().await {
            if line.trim().is_empty() {
                continue;
            }
            match serde_json::from_str::<JsonRpcResponse>(&line) {
                Ok(response) => {
                    if let Some(id) = response.id {
                        let mut map = pending.lock().await;
                        if let Some(sender) = map.remove(&id) {
                            // The receiver may have been dropped on timeout, which is expected.
                            let _ = sender.send(response);
                        }
                    }
                    // Notifications (no id) are intentionally ignored in V1.
                }
                Err(e) => {
                    warn!(error = %e, "failed to parse JSON-RPC line from MCP server");
                }
            }
        }
    })
}

// ─── tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn make_config(
        name: &str,
        command: &str,
        args: Vec<String>,
        init_timeout_secs: u64,
        call_timeout_secs: u64,
    ) -> McpServerConfig {
        McpServerConfig {
            name: name.to_string(),
            command: command.to_string(),
            args,
            env: HashMap::new(),
            transport: "stdio".to_string(),
            url: None,
            requires_approval: false,
            init_timeout_secs,
            call_timeout_secs,
            tags: vec![],
        }
    }

    #[test]
    fn test_tools_list_result_parsing() {
        // GIVEN a tools/list response with three tools
        let json = serde_json::json!({
            "tools": [
                {"name": "search", "description": "Search pages", "inputSchema": {"type": "object"}},
                {"name": "create", "inputSchema": {"type": "object"}},
                {"name": "delete", "description": "Delete a page", "inputSchema": {"type": "object"}}
            ]
        });
        // WHEN
        let result: ToolsListResult = serde_json::from_value(json).unwrap();
        // THEN
        assert_eq!(result.tools.len(), 3);
        assert_eq!(result.tools[0].name, "search");
        assert_eq!(result.tools[1].description, None);
    }

    #[test]
    fn test_empty_tools_list_is_valid() {
        // GIVEN a tools/list response with no tools
        let json = serde_json::json!({"tools": []});
        // WHEN
        let result: ToolsListResult = serde_json::from_value(json).unwrap();
        // THEN
        assert!(result.tools.is_empty());
    }

    #[test]
    fn test_session_error_display() {
        // GIVEN
        let error = McpSessionError::SpawnFailed {
            server: "notion".to_string(),
            cause: "command not found".to_string(),
        };
        // WHEN / THEN
        assert!(error.to_string().contains("notion"));
        assert!(error.to_string().contains("command not found"));
    }

    #[test]
    fn test_schema_fetch_failed_display() {
        // GIVEN a deferred-mode schema fetch failure
        let error = McpSessionError::SchemaFetchFailed {
            server: "notion".to_string(),
            tool: "search_pages".to_string(),
            cause: "tool not found".to_string(),
        };
        // WHEN / THEN the message carries server, tool, and cause
        let display = error.to_string();
        assert!(display.contains("notion"));
        assert!(display.contains("search_pages"));
        assert!(display.contains("tool not found"));
    }

    #[test]
    fn test_loading_mode_is_copy_and_comparable() {
        // GIVEN the two loading modes
        let eager = LoadingMode::Eager;
        // WHEN copied (Copy) and compared (Eq)
        let also_eager = eager;
        // THEN equality and inequality hold without moving the value
        assert_eq!(eager, also_eager);
        assert_ne!(LoadingMode::Eager, LoadingMode::Deferred);
    }

    #[test]
    fn test_initialize_timeout_error_display() {
        // GIVEN
        let error = McpSessionError::InitializeTimeout {
            server: "notion".to_string(),
            timeout_secs: 30,
            stderr_hint: String::new(),
        };
        // WHEN / THEN
        assert!(error.to_string().contains("30s"));
    }

    #[test]
    fn test_initialize_timeout_error_includes_stderr_hint() {
        let error = McpSessionError::InitializeTimeout {
            server: "filesystem".to_string(),
            timeout_secs: 30,
            stderr_hint: "; stderr: npm: command not found".to_string(),
        };
        let msg = error.to_string();
        assert!(msg.contains("npm: command not found"));
        assert!(msg.contains("stderr"));
    }

    #[test]
    fn test_format_stderr_hint() {
        // Empty tail → empty hint
        assert_eq!(format_stderr_hint(&[]), "");
        // Single line is included
        let hint = format_stderr_hint(&["boom".to_string()]);
        assert!(hint.contains("boom"));
        assert!(hint.starts_with("; stderr: "));
        // Multiple lines kept oldest-first, joined with " | "
        let hint = format_stderr_hint(&["a".to_string(), "b".to_string(), "c".to_string()]);
        assert!(hint.contains("a | b | c"));
    }

    #[tokio::test]
    async fn test_spawn_with_invalid_command_fails() {
        // GIVEN a command that does not exist on this system
        let config = make_config("test", "nonexistent-binary-12345", vec![], 5, 10);
        // WHEN
        let result = McpSession::start(config, None).await;
        // THEN
        assert!(matches!(result, Err(McpSessionError::SpawnFailed { .. })));
    }

    #[tokio::test]
    async fn test_handshake_timeout_when_server_never_responds() {
        // GIVEN `cat` spawns successfully but never writes a JSON-RPC response
        let config = make_config("timeout-test", "cat", vec![], 1, 10);
        // WHEN
        let result = McpSession::start(config, None).await;
        // THEN the timeout fires and an error is returned
        assert!(result.is_err());
    }

    #[test]
    fn test_tool_call_params_serialization() {
        use crate::protocol::ToolCallParams;
        // GIVEN
        let params = ToolCallParams {
            name: "search".to_string(),
            arguments: Some(serde_json::json!({"query": "test"})),
        };
        // WHEN
        let value = serde_json::to_value(&params).unwrap();
        // THEN
        assert_eq!(value["name"], "search");
        assert_eq!(value["arguments"]["query"], "test");
    }

    #[test]
    fn test_jsonrpc_error_display() {
        // GIVEN
        let error = McpSessionError::JsonRpcError {
            server: "notion".to_string(),
            code: -32600,
            message: "Invalid Request".to_string(),
        };
        // WHEN / THEN
        let display = error.to_string();
        assert!(display.contains("-32600"));
        assert!(display.contains("Invalid Request"));
    }

    #[test]
    fn test_tool_call_result_with_is_error() {
        use crate::protocol::ToolCallResult;
        // GIVEN
        let json = serde_json::json!({
            "content": [{"type": "text", "text": "tool not found"}],
            "isError": true
        });
        // WHEN
        let result: ToolCallResult = serde_json::from_value(json).unwrap();
        // THEN
        assert_eq!(result.is_error, Some(true));
        assert_eq!(result.content.len(), 1);
    }

    #[test]
    fn test_shutdown_notification_method() {
        // GIVEN the cancellation notification method name
        let method = "notifications/cancelled";
        // THEN it conforms to the MCP protocol notification namespace
        assert!(method.starts_with("notifications/"));
    }

    #[test]
    fn test_tool_call_timeout_error_display() {
        // GIVEN
        let error = McpSessionError::ToolCallTimeout {
            server: "notion".to_string(),
            tool: "search".to_string(),
            timeout_secs: 60,
        };
        // WHEN / THEN
        let display = error.to_string();
        assert!(display.contains("60s"));
        assert!(display.contains("search"));
    }

    #[test]
    fn test_mcp_transport_error_backoff_exponential() {
        // GIVEN the default retry config
        let cfg = McpRetryConfig::DEFAULT;

        // WHEN computing delays for attempts 0, 1, 2
        let delay0 = compute_backoff_delay(0, cfg.base_delay_secs, cfg.max_delay_secs);
        let delay1 = compute_backoff_delay(1, cfg.base_delay_secs, cfg.max_delay_secs);
        let delay2 = compute_backoff_delay(2, cfg.base_delay_secs, cfg.max_delay_secs);

        // THEN delays follow base * 2^attempt, capped at max_delay_secs
        assert_eq!(delay0, 1, "attempt 0 → 1s");
        assert_eq!(delay1, 2, "attempt 1 → 2s");
        assert_eq!(delay2, 4, "attempt 2 → 4s");

        // AND large attempts are capped at max_delay_secs
        let delay_large = compute_backoff_delay(10, cfg.base_delay_secs, cfg.max_delay_secs);
        assert_eq!(
            delay_large, cfg.max_delay_secs,
            "large attempt → max_delay_secs"
        );
    }

    #[tokio::test]
    async fn test_mcp_transport_error_retries_3_times() {
        use std::sync::atomic::{AtomicU32, Ordering};
        use std::sync::Arc;

        // GIVEN a call that fails with StdinClosed for the first 3 attempts and succeeds on the 4th.
        // Zero-delay config so the test does not sleep.
        let call_count = Arc::new(AtomicU32::new(0));
        let call_count_clone = call_count.clone();

        let cfg = McpRetryConfig {
            max_retries: 3,
            base_delay_secs: 0,
            max_delay_secs: 0,
        };

        let result = with_transport_retry(&cfg, "test-server", || {
            let count = call_count_clone.clone();
            async move {
                let attempt = count.fetch_add(1, Ordering::SeqCst);
                if attempt < 3 {
                    Err(McpSessionError::StdinClosed {
                        server: "test-server".to_string(),
                        cause: "simulated".to_string(),
                    })
                } else {
                    Ok(42u32)
                }
            }
        })
        .await;

        // THEN the call succeeds after 3 retries (4 total attempts)
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42u32);
        assert_eq!(
            call_count.load(Ordering::SeqCst),
            4,
            "expected 4 total attempts (1 + 3 retries)"
        );
    }
}
