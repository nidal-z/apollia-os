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

use crate::router::config::{BackendConfig, BackendKind, LlmConfig};
use crate::types::{CompletionModel, CompletionRequest, CompletionResponse, LlmError, StreamChunk};

#[cfg(feature = "cloud")]
use crate::backends::anthropic::AnthropicClient;
#[cfg(feature = "cloud")]
use crate::backends::ollama::{OllamaClient, OllamaConfig};
#[cfg(feature = "cloud")]
use crate::backends::openai::{ApiBackendConfig, OpenAICompatibleClient};
#[cfg(feature = "cloud")]
use crate::backends::vertex::VertexClient;

/// Instantiate an `Arc<dyn CompletionModel>` backend from its config.
///
/// Heuristic: Anthropic API maps to `AnthropicClient`, any other provider to
/// `OpenAICompatibleClient`. Returns `Err` if the API key cannot be resolved.
#[cfg(feature = "cloud")]
pub(super) fn build_backend(
    backend_cfg: &BackendConfig,
    config: &LlmConfig,
    cancellation_token: &CancellationToken,
) -> Result<Arc<dyn CompletionModel>, LlmError> {
    match &backend_cfg.kind {
        BackendKind::Api(cfg) => {
            let key = cfg
                .resolve_api_key()
                .map_err(|e| LlmError::BackendUnavailable {
                    backend: cfg.name.clone(),
                    reason: e.to_string(),
                })?;
            let backend: Arc<dyn CompletionModel> = if cfg.api_url.contains("anthropic.com") {
                Arc::new(AnthropicClient::new(
                    cfg,
                    key,
                    config.pricing_overrides.clone(),
                    cancellation_token.clone(),
                ))
            } else {
                Arc::new(OpenAICompatibleClient::new(
                    cfg,
                    key,
                    cancellation_token.clone(),
                ))
            };
            Ok(backend)
        }
    }
}
/// Instantiate the Vertex AI backend from `[llm.vertex]` when `enabled = true`.
///
/// Fails silently (logs a warning, skips the backend) and does not propagate
/// an error.
#[cfg(feature = "cloud")]
pub(super) fn insert_vertex_backend(
    backends: &mut HashMap<String, Arc<dyn CompletionModel>>,
    config: &LlmConfig,
    cancellation_token: &CancellationToken,
) {
    let Some(vertex_cfg) = &config.vertex else {
        return;
    };
    if !vertex_cfg.enabled {
        return;
    }
    match VertexClient::new(vertex_cfg, cancellation_token.clone()) {
        Ok(client) => {
            backends.insert(
                "vertex".to_owned(),
                Arc::new(client) as Arc<dyn CompletionModel>,
            );
        }
        Err(e) => {
            tracing::warn!(
                error = %e,
                reason = "Vertex AI credentials missing or invalid",
                "llm.backend.skipped"
            );
        }
    }
}
/// Load a backend, emitting `LlmModelLoading` / `LlmModelReady` /
/// `LlmModelFailed` on the bus (when present). Fails silently: the backend is
/// skipped, no crash.
pub(super) fn load_backend_with_bus(
    backends: &mut HashMap<String, Arc<dyn CompletionModel>>,
    backend_cfg: &BackendConfig,
    config: &LlmConfig,
    cancellation_token: &CancellationToken,
    bus: &Option<EventBusSender>,
) {
    let name = backend_cfg.name().to_owned();
    let model_path = backend_cfg.model_path_hint();

    // Emit LlmModelLoading before each load attempt.
    if let Some(b) = bus {
        let _ = b.send(RuntimeEvent::LlmModelLoading {
            backend: name.clone(),
            model_path,
        });
    }

    #[cfg(feature = "cloud")]
    let result = build_backend(backend_cfg, config, cancellation_token);
    #[cfg(not(feature = "cloud"))]
    let result: Result<Arc<dyn CompletionModel>, LlmError> = {
        let _ = (config, cancellation_token);
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
/// Variant of [`insert_vertex_backend`] that emits events on the bus.
#[cfg(feature = "cloud")]
pub(super) fn insert_vertex_backend_with_bus(
    backends: &mut HashMap<String, Arc<dyn CompletionModel>>,
    config: &LlmConfig,
    cancellation_token: &CancellationToken,
    bus: &Option<EventBusSender>,
) {
    let Some(vertex_cfg) = &config.vertex else {
        return;
    };
    if !vertex_cfg.enabled {
        return;
    }
    let vertex_name = "vertex".to_owned();
    if let Some(b) = bus {
        let _ = b.send(RuntimeEvent::LlmModelLoading {
            backend: vertex_name.clone(),
            model_path: vertex_cfg.model_id.clone(),
        });
    }
    match VertexClient::new(vertex_cfg, cancellation_token.clone()) {
        Ok(client) => {
            let model_id = client.model_id().to_owned();
            if let Some(b) = bus {
                let _ = b.send(RuntimeEvent::LlmModelReady {
                    backend: vertex_name.clone(),
                    model_id,
                });
            }
            backends.insert(vertex_name, Arc::new(client) as Arc<dyn CompletionModel>);
        }
        Err(e) => {
            tracing::warn!(
                error = %e,
                reason = "Vertex AI credentials missing or invalid",
                "llm.backend.skipped"
            );
            if let Some(b) = bus {
                let _ = b.send(RuntimeEvent::LlmModelFailed {
                    backend: vertex_name,
                    reason: e.to_string(),
                });
            }
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
        provider => instantiate_cloud_backend(cfg, provider, cancel).await,
    }
}
/// Instantiate a cloud backend (OpenAI-compatible or Anthropic) from the SQLite config.
///
/// Resolves the API key from `config_json["api_key"]` when present.
#[cfg(feature = "cloud")]
pub(super) async fn instantiate_cloud_backend(
    cfg: &LlmBackendConfig,
    provider: &LlmProvider,
    cancel: CancellationToken,
) -> Result<Arc<dyn CompletionModel>, LlmError> {
    let api_key = extract_api_key_value(cfg)?;

    let default_url = match provider {
        LlmProvider::OpenAi => "https://api.openai.com/v1",
        LlmProvider::Mistral => "https://api.mistral.ai/v1",
        LlmProvider::Ollama => "http://127.0.0.1:11434/v1",
        LlmProvider::Anthropic => "https://api.anthropic.com",
        LlmProvider::LlamaCpp => {
            unreachable!("LlamaCpp is handled before reaching instantiate_cloud_backend (sidecar runner path)")
        }
    };

    let base_url = extract_base_url(cfg, default_url);
    let idle_timeout = extract_idle_timeout(cfg);

    // Ollama is reached through its native API, the only one that lets the
    // client choose the context window. See `backends::ollama`.
    if matches!(provider, LlmProvider::Ollama) {
        let root = OllamaConfig::root_of(&base_url);
        let num_ctx =
            OllamaClient::num_ctx(&root, &cfg.model, configured_context_window(cfg)).await;
        tracing::info!(
            backend = %cfg.name,
            model = %cfg.model,
            num_ctx = num_ctx,
            "llm.ollama.num_ctx"
        );
        return Ok(Arc::new(OllamaClient::new(
            OllamaConfig {
                name: cfg.name.clone(),
                root,
                model: cfg.model.clone(),
                num_ctx,
            },
            cancel,
            idle_timeout,
        )) as Arc<dyn CompletionModel>);
    }

    let context_window = resolve_context_window(cfg);

    let api_cfg = ApiBackendConfig {
        name: cfg.name.clone(),
        api_url: base_url,
        api_key_env: String::new(), // key already resolved
        model: cfg.model.clone(),
        context_window,
        // A configured provider is reached over the plain OpenAI protocol. The
        // embedded llama-server never comes through here: `LlamaCpp` is routed
        // to its own backend above, and that is where the extension is declared.
        llama_cpp_extensions: false,
    };

    if matches!(provider, LlmProvider::Anthropic) {
        return Ok(Arc::new(AnthropicClient::with_idle_timeout(
            &api_cfg,
            api_key,
            HashMap::new(),
            cancel,
            idle_timeout,
        )) as Arc<dyn CompletionModel>);
    }

    Ok(Arc::new(OpenAICompatibleClient::with_idle_timeout(
        &api_cfg,
        api_key,
        cancel,
        idle_timeout,
    )) as Arc<dyn CompletionModel>)
}
/// The context window an operator pinned in `config_json["context_window"]`.
#[cfg(feature = "cloud")]
pub(super) fn configured_context_window(cfg: &LlmBackendConfig) -> Option<u32> {
    crate::context_window::configured(&cfg.config_json)
}

/// Establish the usable context window of an OpenAI-compatible backend, so the
/// router sizes compaction against the real window.
///
/// The OpenAI protocol has no way to ask, so this is the operator's configured
/// figure or `None`, which the router reads as unknown. Ollama used to be asked
/// here through `/api/ps`, which reported the window a model happened to be
/// loaded with: 4096 tokens under Ollama's memory-based default, and nothing at
/// all when the model was not loaded, so the same backend flipped between an
/// assumed 200000 and a real 4096 depending on timing. Ollama now goes through
/// its native client, which chooses the window instead of discovering it.
#[cfg(feature = "cloud")]
pub(super) fn resolve_context_window(cfg: &LlmBackendConfig) -> Option<usize> {
    configured_context_window(cfg).map(|v| v as usize)
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
pub(super) async fn instantiate_cloud_backend(
    cfg: &LlmBackendConfig,
    provider: &LlmProvider,
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
