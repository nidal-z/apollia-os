//! Backend instantiation, from a config entry to a live client.
//!
//! Split out of `router.rs`: the router stays in the parent, every path that
//! turns a declared backend into an `Arc<dyn CompletionModel>` lives here,
//! along with the no-backend placeholder.

use std::collections::HashMap;
use std::pin::Pin;
use std::sync::{Arc, OnceLock};

use futures::Stream;
use tokio_util::sync::CancellationToken;

use apollia_core::events::{EventBusSender, RuntimeEvent};
use apollia_core::{LlmBackendConfig, LlmProvider};

use crate::router::config::{BackendConfig, BackendKind};
use crate::types::{CompletionModel, CompletionRequest, CompletionResponse, LlmError, StreamChunk};

#[cfg(feature = "cloud")]
use apollia_core::inference_server::{openai_api_base, DEFAULT_OLLAMA_ADDRESS};

#[cfg(feature = "cloud")]
use crate::backends::openai::{ApiBackendConfig, OpenAICompatibleClient};
#[cfg(feature = "cloud")]
use crate::server_window;

/// Instantiate an `Arc<dyn CompletionModel>` backend from its config.
///
/// Every API backend is served by `OpenAICompatibleClient`. Returns `Err` if
/// the API key cannot be resolved.
#[cfg(feature = "cloud")]
pub(super) fn build_backend(
    backend_cfg: &BackendConfig,
    cancellation_token: &CancellationToken,
) -> Result<Arc<dyn CompletionModel>, LlmError> {
    match &backend_cfg.kind {
        BackendKind::Api(entry) => {
            let cfg = ApiBackendConfig::from(entry);
            let key = cfg
                .resolve_api_key()
                .map_err(|e| LlmError::BackendUnavailable {
                    backend: cfg.name.clone(),
                    reason: e.to_string(),
                })?;
            Ok(Arc::new(OpenAICompatibleClient::new(
                &cfg,
                key,
                cancellation_token.clone(),
            )) as Arc<dyn CompletionModel>)
        }
    }
}
/// Load a backend, emitting `LlmModelLoading` / `LlmModelReady` /
/// `LlmModelFailed` on the bus (when present). Fails silently: the backend is
/// skipped, no crash.
pub(super) fn load_backend_with_bus(
    backends: &mut HashMap<String, Arc<dyn CompletionModel>>,
    backend_cfg: &BackendConfig,
    cancellation_token: &CancellationToken,
    bus: &Option<EventBusSender>,
) {
    let name = backend_cfg.name().to_owned();
    let model_path = crate::router::config::model_path_hint(backend_cfg);

    // Emit LlmModelLoading before each load attempt.
    if let Some(b) = bus {
        let _ = b.send(RuntimeEvent::LlmModelLoading {
            backend: name.clone(),
            model_path,
        });
    }

    #[cfg(feature = "cloud")]
    let result = build_backend(backend_cfg, cancellation_token);
    #[cfg(not(feature = "cloud"))]
    let result: Result<Arc<dyn CompletionModel>, LlmError> = {
        let _ = cancellation_token;
        match &backend_cfg.kind {}
    };

    match result {
        Ok(backend) => {
            // Emit LlmModelReady on success.
            if let Some(b) = bus {
                let _ = b.send(RuntimeEvent::LlmModelReady {
                    backend: name.clone(),
                    model_id: backend.model_id().to_owned(),
                });
            }
            backends.insert(name, backend);
        }
        Err(e) => {
            tracing::warn!(
                backend = %name,
                error = %e,
                reason = "loading failed",
                "llm.backend.skipped"
            );
            // Emit LlmModelFailed: backend skipped, no crash.
            if let Some(b) = bus {
                let _ = b.send(RuntimeEvent::LlmModelFailed {
                    backend: name.clone(),
                    reason: e.to_string(),
                });
            }
            // Keep going and try the remaining backends.
        }
    }
}
/// The backend a router with no backend at all routes to.
///
/// `LlmRouter::empty()` builds such a router, and two production paths fall
/// back to it when no backend could be instantiated. Every call reports
/// [`LlmError::BackendUnavailable`], which is what the callers of `complete`
/// and `stream` already handle, and what the degradation paths this
/// constructor exists to exercise expect to see.
pub(super) struct NoBackend;
impl NoBackend {
    const NAME: &'static str = "none";

    fn unavailable() -> LlmError {
        LlmError::BackendUnavailable {
            backend: Self::NAME.to_string(),
            reason: "the router holds no backend".to_string(),
        }
    }
}
#[async_trait::async_trait]
impl CompletionModel for NoBackend {
    async fn complete(&self, _req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
        Err(Self::unavailable())
    }

    async fn stream(
        &self,
        _req: CompletionRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamChunk, LlmError>> + Send>>, LlmError> {
        Err(Self::unavailable())
    }

    fn is_available(&self) -> bool {
        false
    }

    fn backend_name(&self) -> &str {
        Self::NAME
    }

    fn model_id(&self) -> &str {
        Self::NAME
    }
}
/// The shared [`NoBackend`] instance, allocated once.
pub(super) fn no_backend() -> Arc<dyn CompletionModel> {
    static INSTANCE: OnceLock<Arc<dyn CompletionModel>> = OnceLock::new();
    INSTANCE.get_or_init(|| Arc::new(NoBackend)).clone()
}
/// Resolve the configured default backend name, failing fast when it is absent.
///
/// The configured default must be present in the successfully-instantiated
/// `backends` map. When it is missing (e.g. the local feature isn't compiled or
/// the API key is missing for a cloud backend) this returns
/// [`LlmError::BackendUnavailable`] rather than silently substituting another
/// backend: routing every request to an unrequested model is a silent failure,
/// and principle 4 (fail fast) requires a startup-detectable misconfiguration
/// to surface at startup.
///
/// # Errors
///
/// - [`LlmError::BackendUnavailable`] if `configured_default` is not among the
///   instantiated backends.
pub(super) fn resolve_default_backend(
    configured_default: String,
    backends: &HashMap<String, Arc<dyn CompletionModel>>,
) -> Result<String, LlmError> {
    if backends.contains_key(&configured_default) {
        return Ok(configured_default);
    }
    let mut available: Vec<&String> = backends.keys().collect();
    available.sort();
    Err(LlmError::BackendUnavailable {
        backend: configured_default,
        reason: format!("configured default backend not instantiated (available: {available:?})"),
    })
}
/// Instantiate a [`CompletionModel`] from a SQLite [`LlmBackendConfig`].
pub(super) async fn instantiate_from_config(
    cfg: &LlmBackendConfig,
    cancel: CancellationToken,
) -> Result<Arc<dyn CompletionModel>, LlmError> {
    match &cfg.provider {
        // Local `.gguf` backends are served by the embedded `llama-server`,
        // injected by the caller's override factory. Reaching here means the
        // supervisor could not be built, and the only reason it fails to build
        // is a missing binary. The message names that, because the previous
        // wording blamed the `apollia-runner` sidecar, which stopped carrying
        // local LLM inference and sends the reader looking in the wrong place.
        LlmProvider::LlamaCpp => Err(LlmError::BackendUnavailable {
            backend: cfg.name.clone(),
            reason: "local model backend needs the embedded llama-server, which was not \
                     found. Place the `llama-server` binary next to the apollia-os \
                     executable or in a `runners/` directory beside it, or point \
                     APOLLIA_LLAMA_SERVER_BIN at it. A `cargo build -p apollia-cli` \
                     does not stage it: only the packaged build does"
                .to_string(),
        }),
        LlmProvider::OpenAi => {
            instantiate_remote_backend(cfg, RemoteKind::OpenAiCompatible, cancel).await
        }
        LlmProvider::Ollama => instantiate_remote_backend(cfg, RemoteKind::Ollama, cancel).await,
    }
}
/// The providers reached over HTTP, the local engine excluded by construction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RemoteKind {
    /// An OpenAI-compatible server at the address the backend designates.
    OpenAiCompatible,
    /// An Ollama server, local by default.
    Ollama,
}
impl std::fmt::Display for RemoteKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RemoteKind::OpenAiCompatible => f.write_str("openai"),
            RemoteKind::Ollama => f.write_str("ollama"),
        }
    }
}
/// Instantiate an HTTP backend (OpenAI-compatible server or Ollama) from the
/// SQLite config.
///
/// Both are reached through the OpenAI-compatible client: an Ollama row is an
/// OpenAI-compatible server on port 11434 by default, and nothing in its
/// requests chooses how the server loads the model. Resolves the API key from
/// `config_json["api_key"]` when present. An OpenAI-compatible backend must
/// designate its server: there is no cloud address to fall back on.
#[cfg(feature = "cloud")]
pub(super) async fn instantiate_remote_backend(
    cfg: &LlmBackendConfig,
    kind: RemoteKind,
    cancel: CancellationToken,
) -> Result<Arc<dyn CompletionModel>, LlmError> {
    let api_key = extract_api_key_value(cfg)?;

    let base_url = match kind {
        RemoteKind::Ollama => extract_base_url(cfg, DEFAULT_OLLAMA_ADDRESS),
        RemoteKind::OpenAiCompatible => {
            let url = extract_base_url(cfg, "");
            if url.is_empty() {
                return Err(LlmError::BackendUnavailable {
                    backend: cfg.name.clone(),
                    reason: "an OpenAI-compatible backend needs the address of its server \
                             (base_url)"
                        .to_string(),
                });
            }
            url
        }
    };
    let idle_timeout = extract_idle_timeout(cfg);
    let context_window = discover_context_window(cfg, kind, &base_url, &api_key).await;

    let api_cfg = ApiBackendConfig {
        name: cfg.name.clone(),
        api_url: openai_api_base(&base_url),
        api_key_env: String::new(), // key already resolved
        model: cfg.model.clone(),
        context_window,
        // A configured provider is reached over the plain OpenAI protocol. The
        // embedded llama-server never comes through here: `LlamaCpp` is routed
        // to its own backend above, and that is where the extension is declared.
        llama_cpp_extensions: false,
    };

    Ok(Arc::new(OpenAICompatibleClient::with_idle_timeout(
        &api_cfg,
        api_key,
        cancel,
        idle_timeout,
    )) as Arc<dyn CompletionModel>)
}
/// The context window a remote backend declares, read on its server when the
/// server exposes it, else the operator's configured figure.
///
/// Ollama is asked by `POST /api/show`; any other server is read in
/// `GET /v1/models` (`max_model_len` on vLLM). A window read shorter than
/// [`server_window::EXPECTED_CONTEXT_WINDOW`] is reported by
/// `llm.server.context_short` and declared as it is: the server's
/// configuration stays its administrator's.
#[cfg(feature = "cloud")]
pub(super) async fn discover_context_window(
    cfg: &LlmBackendConfig,
    kind: RemoteKind,
    base_url: &str,
    api_key: &str,
) -> Option<usize> {
    let key = Some(api_key).filter(|k| !k.is_empty());
    let read = match kind {
        RemoteKind::Ollama => server_window::ollama_window(base_url, &cfg.model, key).await,
        RemoteKind::OpenAiCompatible => {
            server_window::listed_window(base_url, &cfg.model, key).await
        }
    };
    match read {
        Some(window) => {
            server_window::context_short(&cfg.name, &cfg.model, window);
            tracing::event!(
                tracing::Level::INFO,
                backend = %cfg.name,
                model = %cfg.model,
                context_window = window,
                "llm.server.context_read"
            );
            Some(window as usize)
        }
        None => resolve_context_window(cfg),
    }
}
/// The context window the operator configured for a remote backend, the
/// figure that stands when its server exposes none (see
/// [`discover_context_window`]). `None` reads as unknown to the router.
#[cfg(feature = "cloud")]
pub(super) fn resolve_context_window(cfg: &LlmBackendConfig) -> Option<usize> {
    crate::context_window::configured(&cfg.config_json).map(|v| v as usize)
}
/// Reads how long a backend may stay silent before the call is abandoned.
///
/// Persisted as `config_json["timeout_sec"]`, which is what
/// `apollia-os llm backends create --timeout-sec` writes. Absent or zero falls
/// back to the shared default rather than to an unbounded wait: a backend that
/// accepts a connection and never answers must not pin the caller forever.
#[cfg(feature = "cloud")]
pub(super) fn extract_idle_timeout(cfg: &LlmBackendConfig) -> std::time::Duration {
    crate::http_client::idle_timeout_from_secs(
        cfg.config_json.get("timeout_sec").and_then(|v| v.as_u64()),
    )
}
/// Return `BackendUnavailable` when the `"cloud"` feature is not compiled.
#[cfg(not(feature = "cloud"))]
pub(super) async fn instantiate_remote_backend(
    cfg: &LlmBackendConfig,
    provider: RemoteKind,
    _cancel: CancellationToken,
) -> Result<Arc<dyn CompletionModel>, LlmError> {
    Err(LlmError::BackendUnavailable {
        backend: cfg.name.clone(),
        reason: format!("provider '{}' requires feature 'cloud'", provider),
    })
}
/// Extract and resolve the API key from `config_json["api_key"]`.
///
/// - Absent: `Ok("")` (Ollama-style, no authentication)
/// - `"${VAR}"`: resolved via `std::env::var(VAR)`; errors if the variable is absent
/// - Literal value: returned as-is
#[cfg(feature = "cloud")]
pub(super) fn extract_api_key_value(cfg: &LlmBackendConfig) -> Result<String, LlmError> {
    let raw = match cfg.config_json.get("api_key").and_then(|v| v.as_str()) {
        None => return Ok(String::new()),
        Some(s) => s.to_string(),
    };

    if let Some(var_name) = raw.strip_prefix("${").and_then(|s| s.strip_suffix('}')) {
        std::env::var(var_name).map_err(|_| LlmError::ApiKeyMissing {
            var: var_name.to_string(),
        })
    } else {
        Ok(raw)
    }
}
/// Extract the base URL from the backend `config_json`, or return `default`.
///
/// The URL has historically been persisted under three different keys: the
/// desktop settings write `endpoint`, the CLI writes `base_url`, and the
/// TOML-to-DB seed writes `api_url`. This reads all three (canonical first) so a
/// backend created by any path resolves to its real URL instead of silently
/// falling back to the provider default.
#[cfg(feature = "cloud")]
pub(super) fn extract_base_url(cfg: &LlmBackendConfig, default: &str) -> String {
    ["base_url", "endpoint", "api_url"]
        .iter()
        .find_map(|key| {
            cfg.config_json
                .get(*key)
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
        })
        .unwrap_or(default)
        .to_string()
}

#[cfg(all(test, feature = "cloud"))]
mod tests;
