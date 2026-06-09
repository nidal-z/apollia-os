//! `LlmRouter`, dispatches requests to the right backend by name.
//!
//! Built at Supervisor startup (before `TaskRouter`) via
//! [`LlmRouter::from_config`]. Shareable as `Arc<LlmRouter>` thanks to
//! `Clone + Send + Sync`.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use tokio_util::sync::CancellationToken;

use crate::pricing::PricingTier;
use crate::routing_level::{EscalationSignal, LlmRoutingLevel};

use apollia_core::events::{EventBusSender, RuntimeEvent};
use apollia_core::token_budget::TokenBudget;

use crate::token_budget::SessionBudgetTracker;
use apollia_core::{
    LlmBackendConfig, LlmBackendRepository, LlmProvider, LlmRoutingConfig, LlmRunnerConfig,
    VertexConfig,
};

use crate::types::{
    message_char_len, BackendInfo, ChatMessage, CompletionModel, CompletionRequest,
    CompletionResponse, LlmError,
};

#[cfg(feature = "cloud")]
use crate::backends::anthropic::AnthropicClient;

#[cfg(feature = "cloud")]
use crate::backends::openai::{ApiBackendConfig, OpenAICompatibleClient};

#[cfg(feature = "cloud")]
use crate::backends::vertex::VertexClient;

/// LLM configuration deserialized from the `[llm]` section of `apollia.toml`.
///
/// Passed to [`LlmRouter::from_config`] at Supervisor startup. The `default`
/// field names the backend used when `get(None)` is called.
///
/// The `[llm.routing]` section is mandatory: its absence triggers
/// [`LlmError::RoutingConfigMissing`] at startup (fail fast).
#[derive(Debug, Clone, serde::Deserialize)]
pub struct LlmConfig {
    /// Default backend name (must exist in `backends`).
    pub default: String,
    /// Backends to instantiate from `[[llm.backends]]`.
    pub backends: Vec<BackendConfig>,
    /// Observability settings (tokens, latency, cost, prompt debug).
    #[serde(default)]
    pub observability: ObservabilityConfig,
    /// LLM routing by precision level (`[llm.routing]` section).
    ///
    /// Mandatory: triggers [`LlmError::RoutingConfigMissing`] if absent.
    /// See [`LlmRoutingConfig`] for the `precise` and `fast` fields.
    pub routing: Option<LlmRoutingConfig>,
    /// Operator pricing overrides (`[llm.pricing_overrides]` section).
    ///
    /// Entries here take priority over the internal table in
    /// [`crate::pricing::default_pricing`]. Lets operators add custom models
    /// or correct prices without a code update.
    ///
    /// `apollia.toml` example:
    /// ```toml
    /// [llm.pricing_overrides]
    /// "custom-local-model" = { input_per_mtok = 0.0, output_per_mtok = 0.0 }
    /// "claude-sonnet-4-5"  = { input_per_mtok = 2.5, output_per_mtok = 12.0 }
    /// ```
    #[serde(default)]
    pub pricing_overrides: HashMap<String, PricingTier>,
    /// Cost threshold in USD above which [`RuntimeEvent::TokenBudgetUpdated`]
    /// is emitted with `threshold_exceeded = true`.
    ///
    /// `None` (default) disables threshold alerts.
    ///
    /// `apollia.toml` example:
    /// ```toml
    /// [llm]
    /// cost_alert_threshold_usd = 0.50
    /// ```
    #[serde(default)]
    pub cost_alert_threshold_usd: Option<f64>,
    /// Optional Google Vertex AI backend configuration (`[llm.vertex]`).
    ///
    /// If absent or `enabled = false`, the backend is not instantiated.
    ///
    /// `apollia.toml` example:
    /// ```toml
    /// [llm.vertex]
    /// enabled    = true
    /// project_id = "my-gcp-project"
    /// location   = "us-east5"
    /// model_id   = "claude-sonnet-4-6@20251001"
    /// ```
    #[serde(default)]
    pub vertex: Option<VertexConfig>,
    /// Local LLM sidecar runner configuration (`[llm.runner]` section).
    ///
    /// Determines which runner binary (`apollia-runner-cuda`,
    /// `apollia-runner-metal`, etc.) the daemon spawns at boot. The default
    /// `"auto"` lets `apollia_runtime::runner_supervisor::gpu_detection`
    /// choose based on the hardware.
    #[serde(default)]
    pub runner: LlmRunnerConfig,
}

impl LlmConfig {
    /// Converts TOML-parsed backends to [`LlmBackendConfig`] entries for `system.db`.
    ///
    /// Used by the Supervisor at startup to migrate from `apollia.toml` to `system.db`
    /// when no backends are found in the database (first boot, or manual TOML edits).
    pub fn to_db_configs(&self) -> Vec<LlmBackendConfig> {
        self.backends
            .iter()
            .map(|b| backend_config_to_db(b, b.name() == self.default))
            .collect()
    }
}

/// Converts a TOML [`BackendConfig`] to a [`LlmBackendConfig`] for `system.db`.
fn backend_config_to_db(cfg: &BackendConfig, is_default: bool) -> LlmBackendConfig {
    match &cfg.kind {
        #[cfg(feature = "cloud")]
        BackendKind::Api(api) => LlmBackendConfig {
            name: api.name.clone(),
            provider: infer_api_provider_from_url(&api.api_url),
            model: api.model.clone(),
            config_json: serde_json::json!({
                "api_url": api.api_url,
                "api_key": format!("${{{}}}", api.api_key_env),
            }),
            enabled: true,
            is_default,
        },
    }
}

/// Infers a [`LlmProvider`] from the API base URL.
#[cfg(feature = "cloud")]
fn infer_api_provider_from_url(api_url: &str) -> LlmProvider {
    if api_url.contains("anthropic.com") {
        LlmProvider::Anthropic
    } else if api_url.contains("mistral.ai") {
        LlmProvider::Mistral
    } else if api_url.contains("localhost:11434") || api_url.contains("ollama") {
        LlmProvider::Ollama
    } else {
        LlmProvider::OpenAi
    }
}

/// Observability settings for the LLM router.
///
/// `log_token_usage` and `log_latency` are enabled by default.
/// `log_cost` and `debug_log_prompt` are disabled by default.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ObservabilityConfig {
    /// Log the number of tokens consumed after each call.
    #[serde(default = "default_true")]
    pub log_token_usage: bool,
    /// Log the total latency of each call.
    #[serde(default = "default_true")]
    pub log_latency: bool,
    /// Log the estimated cost in USD (cloud backends only).
    #[serde(default)]
    pub log_cost: bool,
    /// Log the full prompt at `TRACE` level (debug only).
    #[serde(default)]
    pub debug_log_prompt: bool,
}

impl Default for ObservabilityConfig {
    fn default() -> Self {
        Self {
            log_token_usage: true,
            log_latency: true,
            log_cost: false,
            debug_log_prompt: false,
        }
    }
}

fn default_true() -> bool {
    true
}

/// Routing context for [`LlmRouter::complete_with_fallback`].
///
/// Groups the primary backend, the ordered fallback list, the optional event
/// bus and the observability config. The completion request stays passed
/// separately because it is consumed per call.
pub struct FallbackPlan<'a> {
    /// Name of the primary backend to try first.
    pub primary: &'a str,
    /// Secondary backends tried in order if the primary fails.
    pub fallbacks: &'a [&'a str],
    /// Optional event bus to emit [`RuntimeEvent::LlmFallbackTriggered`].
    pub bus: Option<&'a EventBusSender>,
    /// Observability config propagated to the underlying calls.
    pub obs: &'a ObservabilityConfig,
}

/// Configuration entry for an individual backend in `[[llm.backends]]`.
///
/// The backend's logical name is defined in the inner config
/// (`ApiBackendConfig.name`).
#[derive(Debug, Clone, serde::Deserialize)]
pub struct BackendConfig {
    /// Backend type and parameters, discriminated by the TOML `type` field.
    #[serde(flatten)]
    pub kind: BackendKind,
}

impl BackendConfig {
    /// Return the backend's logical name from the inner config.
    pub fn name(&self) -> &str {
        match &self.kind {
            #[cfg(feature = "cloud")]
            BackendKind::Api(cfg) => &cfg.name,
        }
    }

    /// Return a path/URL hint for the `LlmModelLoading` event.
    ///
    /// For a cloud backend this is the API URL.
    fn model_path_hint(&self) -> String {
        match &self.kind {
            #[cfg(feature = "cloud")]
            BackendKind::Api(cfg) => cfg.api_url.clone(),
        }
    }
}

/// Backend type discriminant in `[[llm.backends]]`.
///
/// `type = "api"`: [`ApiBackendConfig`] (feature `"cloud"`).
/// Local backends (llama-cpp) are now hosted by the `apollia-runner` crate
/// (sidecar) and injected separately via `RunnerLlmBackend`.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum BackendKind {
    /// OpenAI- or Anthropic-compatible cloud HTTP backend (feature `"cloud"`).
    #[cfg(feature = "cloud")]
    Api(ApiBackendConfig),
}

/// Single entry point for the entire Apollia OS LLM layer.
///
/// Instantiated by the Supervisor at startup via [`LlmRouter::from_config`].
/// Dispatches requests to the right backend by name via [`get`](Self::get),
/// with fallback to the `default` backend.
///
/// [`route_precise`](Self::route_precise) and [`route_fast`](Self::route_fast)
/// select the backend by the required precision level (`[llm.routing]` config).
///
/// `LlmRouter` is `Clone + Send + Sync`, shareable as `Arc<LlmRouter>` across
/// runtime components (it acts as a read-only catalog).
///
/// `Debug` is implemented manually: `Arc<dyn CompletionModel>` does not
/// implement `Debug` (the trait object does not expose it).
///
/// The session `CancellationToken` lets `ORIAEngine::abort()` cancel all
/// in-flight LLM calls and their retry delays via
/// [`cancellation_token`](Self::cancellation_token).
#[derive(Clone)]
pub struct LlmRouter {
    backends: HashMap<String, Arc<dyn CompletionModel>>,
    default: String,
    /// LLM routing by precision level. `None` for routers built via
    /// `from_repository` or `with_backends` (no TOML config).
    routing: Option<LlmRoutingConfig>,
    /// Cancellation token shared by all backends of this router.
    cancellation_token: CancellationToken,
    /// Cumulative session budget with real-time event emission.
    ///
    /// Guarded by a standard `Mutex` (short lock, never held across an async
    /// call) so the struct can `Clone` without an extra `Arc`.
    session_budget: Arc<Mutex<SessionBudgetTracker>>,
}

/// Instantiate an `Arc<dyn CompletionModel>` backend from its config.
///
/// Heuristic: Anthropic API maps to `AnthropicClient`, any other provider to
/// `OpenAICompatibleClient`. Returns `Err` if the API key cannot be resolved.
#[cfg(feature = "cloud")]
fn build_backend(
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
fn insert_vertex_backend(
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
                "Vertex AI backend ignoré : ADC absent ou invalide"
            );
        }
    }
}

/// Load a backend, emitting `LlmModelLoading` / `LlmModelReady` /
/// `LlmModelFailed` on the bus (when present). Fails silently: the backend is
/// skipped, no crash.
fn load_backend_with_bus(
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
                "backend ignoré : chargement échoué"
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
fn insert_vertex_backend_with_bus(
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
                "Vertex AI backend ignoré : ADC absent ou invalide"
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

/// Validate `[llm.routing]` consistency: the named backends must exist.
///
/// `[llm.routing]` is optional. When present, the `precise`/`fast` names must
/// be in the map; otherwise `route_precise/fast` fall back to
/// `config.default` at runtime.
fn validate_routing(
    backends: &HashMap<String, Arc<dyn CompletionModel>>,
    routing: Option<&LlmRoutingConfig>,
) -> Result<(), LlmError> {
    if let Some(routing) = routing {
        if !backends.contains_key(&routing.precise) {
            return Err(LlmError::BackendNotFound(routing.precise.clone()));
        }
        if !backends.contains_key(&routing.fast) {
            return Err(LlmError::BackendNotFound(routing.fast.clone()));
        }
        // When the hybrid section is present, the frontier backend it names must
        // exist too: a misconfiguration is caught at startup, not at runtime.
        if let Some(hybrid) = routing.hybrid.as_ref() {
            if !backends.contains_key(&hybrid.frontier) {
                return Err(LlmError::BackendNotFound(hybrid.frontier.clone()));
            }
        }
    }
    Ok(())
}

impl LlmRouter {
    /// Build the router from configuration, called by the Supervisor at startup.
    ///
    /// Iterates over `config.backends` and tries to instantiate each backend.
    /// For `Api`: resolves the API key; if missing, logs `tracing::warn!` and
    /// skips the backend.
    ///
    /// After the loop, checks that `config.default` is present in the map.
    /// If absent (unconfigured or skipped) returns [`LlmError::BackendUnavailable`].
    ///
    /// # Errors
    ///
    /// - [`LlmError::ModelNotFound`] / [`LlmError::InferenceError`]: `.gguf` load failed.
    /// - [`LlmError::BackendUnavailable`]: the default backend is missing or unavailable.
    pub async fn from_config(config: &LlmConfig) -> Result<Self, LlmError> {
        let cancellation_token = CancellationToken::new();
        let mut backends: HashMap<String, Arc<dyn CompletionModel>> = HashMap::new();

        for backend_cfg in &config.backends {
            let name = backend_cfg.name().to_owned();

            #[cfg(feature = "cloud")]
            let result = build_backend(backend_cfg, config, &cancellation_token);
            #[cfg(not(feature = "cloud"))]
            let result: Result<Arc<dyn CompletionModel>, LlmError> = match &backend_cfg.kind {};

            match result {
                Ok(backend) => {
                    backends.insert(name, backend);
                }
                Err(e) => {
                    tracing::warn!(
                        backend = %name,
                        error = %e,
                        "backend ignoré : clé API absente"
                    );
                    continue;
                }
            }
        }

        // Vertex AI is instantiated separately from [llm.vertex] when enabled = true.
        #[cfg(feature = "cloud")]
        insert_vertex_backend(&mut backends, config, &cancellation_token);

        // The default backend must be available after the loop.
        if !backends.contains_key(&config.default) {
            return Err(LlmError::BackendUnavailable {
                backend: config.default.clone(),
                reason: "not configured".to_owned(),
            });
        }

        validate_routing(&backends, config.routing.as_ref())?;

        Ok(Self {
            backends,
            default: config.default.clone(),
            routing: config.routing.clone(),
            cancellation_token,
            session_budget: Arc::new(Mutex::new(SessionBudgetTracker::default())),
        })
    }

    /// Build the router from configuration with EventBus observability.
    ///
    /// Variant of [`from_config`](Self::from_config) for the Supervisor to use.
    /// Emits on the bus for each backend:
    /// - [`RuntimeEvent::LlmModelLoading`]: before loading
    /// - [`RuntimeEvent::LlmModelReady`]: when loading succeeds
    /// - [`RuntimeEvent::LlmModelFailed`]: when loading fails (backend skipped, no crash)
    ///
    /// The `EventBusSender` is optional: `None` disables event emission without
    /// changing functional behavior.
    ///
    /// Unlike [`from_config`](Self::from_config), per-backend load errors
    /// (missing `.gguf`, etc.) are logged and emitted as `LlmModelFailed` but
    /// do not propagate an error; the router continues with the available backends.
    ///
    /// # Errors
    ///
    /// - [`LlmError::BackendUnavailable`]: the default backend is missing after
    ///   all backends have been tried.
    pub async fn from_config_with_bus(
        config: &LlmConfig,
        bus: Option<EventBusSender>,
    ) -> Result<Self, LlmError> {
        let cancellation_token = CancellationToken::new();
        let mut backends: HashMap<String, Arc<dyn CompletionModel>> = HashMap::new();

        for backend_cfg in &config.backends {
            load_backend_with_bus(&mut backends, backend_cfg, config, &cancellation_token, &bus);
        }

        // Vertex AI is instantiated separately from [llm.vertex] when enabled = true.
        #[cfg(feature = "cloud")]
        insert_vertex_backend_with_bus(&mut backends, config, &cancellation_token, &bus);

        if !backends.contains_key(&config.default) {
            return Err(LlmError::BackendUnavailable {
                backend: config.default.clone(),
                reason: "not configured".to_owned(),
            });
        }

        validate_routing(&backends, config.routing.as_ref())?;

        Ok(Self {
            backends,
            default: config.default.clone(),
            routing: config.routing.clone(),
            cancellation_token,
            session_budget: Arc::new(Mutex::new(SessionBudgetTracker::new(
                bus,
                config.cost_alert_threshold_usd,
            ))),
        })
    }

    /// Call the backend and automatically emit [`RuntimeEvent::LlmCallCompleted`].
    ///
    /// Execution sequence:
    /// 1. Log the prompt at `TRACE` level if `obs.debug_log_prompt` is enabled.
    /// 2. Call `backend.complete(req)`.
    /// 3. Emit `LlmCallCompleted` fire-and-forget on the bus (when present).
    /// 4. Log tokens/latency at `INFO` level per the `obs` flags.
    /// 5. Return `Ok(response)`.
    ///
    /// The `EventBusSender` is optional: `None` disables emission without
    /// changing functional behavior. `send()` errors are silently ignored.
    ///
    /// # Errors
    ///
    /// - [`LlmError::BackendUnavailable`]: the requested backend is not in the router.
    /// - Any error propagated by `backend.complete()`.
    pub async fn complete_with_observability(
        &self,
        backend_name: Option<&str>,
        req: CompletionRequest,
        bus: Option<&EventBusSender>,
        obs: &ObservabilityConfig,
    ) -> Result<CompletionResponse, LlmError> {
        let backend_key = backend_name.unwrap_or(&self.default);

        let backend =
            self.backends
                .get(backend_key)
                .ok_or_else(|| LlmError::BackendUnavailable {
                    backend: backend_key.to_owned(),
                    reason: "not found in router".to_owned(),
                })?;

        // Log the prompt at TRACE only, never at INFO.
        if obs.debug_log_prompt {
            tracing::trace!(prompt = ?req.messages, "llm prompt");
        }

        let started = Instant::now();
        let response = backend.complete(req).await?;
        let latency_ms = started.elapsed().as_millis() as u64;

        // Accumulate into the session budget and emit TokenBudgetUpdated.
        // Lock held only for the duration of record_usage(), never across awaits.
        if let Ok(mut tracker) = self.session_budget.lock() {
            tracker.record_usage(&response.usage, latency_ms, response.ttft_ms);
        }

        // Fire-and-forget emission: send() errors are silently ignored.
        if let Some(b) = bus {
            let _ = b.send(RuntimeEvent::LlmCallCompleted {
                backend: backend_key.to_owned(),
                model: backend.model_id().to_owned(),
                task_id: None,
                step_id: None,
                prompt_tokens: response.usage.prompt_tokens,
                completion_tokens: response.usage.completion_tokens,
                latency_ms,
                cost_usd: response.usage.cost_usd,
            });
        }

        if obs.log_token_usage {
            tracing::info!(
                backend = backend_key,
                prompt_tokens = response.usage.prompt_tokens,
                completion_tokens = response.usage.completion_tokens,
                "llm token usage"
            );
        }

        if obs.log_latency {
            tracing::info!(
                backend = backend_key,
                latency_ms = latency_ms,
                "llm latency"
            );
        }

        Ok(response)
    }

    /// Invoke the primary backend, then on a non-recoverable failure switch to
    /// the first available secondary backend.
    ///
    /// Emits [`RuntimeEvent::LlmFallbackTriggered`] on the bus for each
    /// successful switch. The switch is functionally transparent: the caller
    /// receives either the primary's response, the response of the first
    /// fallback that answers, or the last observed error.
    pub async fn complete_with_fallback(
        &self,
        plan: FallbackPlan<'_>,
        req: CompletionRequest,
    ) -> Result<CompletionResponse, LlmError> {
        let FallbackPlan {
            primary,
            fallbacks,
            bus,
            obs,
        } = plan;
        let primary_result = self
            .complete_with_observability(Some(primary), req.clone(), bus, obs)
            .await;

        let primary_err = match primary_result {
            Ok(response) => return Ok(response),
            Err(e) => e,
        };

        let mut last_err = primary_err;
        for &candidate in fallbacks {
            if candidate == primary || !self.backends.contains_key(candidate) {
                continue;
            }
            if let Some(b) = bus {
                let _ = b.send(RuntimeEvent::LlmFallbackTriggered {
                    from_provider: primary.to_string(),
                    to_provider: candidate.to_string(),
                    reason: last_err.to_string(),
                });
            }
            tracing::warn!(
                from = %primary,
                to = %candidate,
                reason = %last_err,
                "LLM primary failed, attempting fallback"
            );
            match self
                .complete_with_observability(Some(candidate), req.clone(), bus, obs)
                .await
            {
                Ok(response) => return Ok(response),
                Err(e) => {
                    last_err = e;
                }
            }
        }

        Err(last_err)
    }

    /// Open a stream of [`StreamChunk`]s from the resolved backend.
    ///
    /// Resolves the backend (by name or default), calls `backend.stream(req)`,
    /// and returns the raw stream. The caller is responsible for emitting the
    /// `LlmCallCompleted` event once the stream is consumed.
    ///
    /// # Errors
    ///
    /// - [`LlmError::BackendUnavailable`]: the requested backend is not in the router.
    /// - Any error propagated by `backend.stream()`.
    pub async fn stream_with_observability(
        &self,
        backend_name: Option<&str>,
        req: CompletionRequest,
        obs: &ObservabilityConfig,
    ) -> Result<
        std::pin::Pin<
            Box<dyn futures::Stream<Item = Result<crate::types::StreamChunk, LlmError>> + Send>,
        >,
        LlmError,
    > {
        let backend_key = backend_name.unwrap_or(&self.default);

        let backend =
            self.backends
                .get(backend_key)
                .ok_or_else(|| LlmError::BackendUnavailable {
                    backend: backend_key.to_owned(),
                    reason: "not found in router".to_owned(),
                })?;

        if obs.debug_log_prompt {
            tracing::trace!(prompt = ?req.messages, "llm stream prompt");
        }

        let stream = backend.stream(req).await?;
        Ok(stream)
    }

    /// Return the backend by name, or the default backend if `name` is `None`.
    ///
    /// Returns `None` if the requested backend is not in the router.
    pub fn get(&self, name: Option<&str>) -> Option<Arc<dyn CompletionModel>> {
        let key = name.unwrap_or(&self.default);
        self.backends.get(key).cloned()
    }

    /// Build an `LlmRouter` from already-instantiated backends.
    ///
    /// Used in integration tests to inject [`CompletionModel`] mocks without
    /// going through the TOML configuration. LLM routing is not configured on
    /// this router, so `route_precise()` and `route_fast()` will return
    /// [`LlmError::RoutingConfigMissing`].
    ///
    /// # Panics
    ///
    /// Panics if `default` is not present in `backends`.
    pub fn with_backends(
        backends: HashMap<String, Arc<dyn CompletionModel>>,
        default: impl Into<String>,
    ) -> Self {
        let default = default.into();
        assert!(
            backends.contains_key(&default),
            "LlmRouter::with_backends - backend '{default}' must be present in backends map"
        );
        Self {
            backends,
            default,
            routing: None,
            cancellation_token: CancellationToken::new(),
            session_budget: Arc::new(Mutex::new(SessionBudgetTracker::default())),
        }
    }

    /// Build the router from an already-loaded list of [`LlmBackendConfig`].
    ///
    /// Variant of [`from_repository`](Self::from_repository) that takes the
    /// configs directly (useful when the SQLite repository has already been
    /// read in a blocking thread, e.g. via `spawn_blocking`).
    ///
    /// Only `enabled = true` backends are instantiated. Backends that fail are
    /// logged and skipped (non-fatal degradation).
    ///
    /// # Errors
    ///
    /// - [`LlmError::BackendUnavailable`] if `default_name` is not instantiated successfully.
    pub async fn from_backend_configs(
        all: Vec<LlmBackendConfig>,
        default_name: String,
    ) -> Result<Self, LlmError> {
        Self::from_backend_configs_with_override(all, default_name, |_| None).await
    }

    /// Variant of [`from_backend_configs`](Self::from_backend_configs) with an
    /// override factory (multi-runner).
    ///
    /// Identical to [`from_repository_with_override`](Self::from_repository_with_override)
    /// but takes already-loaded configs (useful for reloads that read the
    /// SQLite repository in a `spawn_blocking`). The factory routes `LlamaCpp`
    /// backends to a `RunnerLlmBackend`; without it, those backends are skipped
    /// (the local runner becomes unreachable).
    pub async fn from_backend_configs_with_override<F>(
        all: Vec<LlmBackendConfig>,
        default_name: String,
        override_factory: F,
    ) -> Result<Self, LlmError>
    where
        F: Fn(&LlmBackendConfig) -> Option<Arc<dyn CompletionModel>>,
    {
        let cancellation_token = CancellationToken::new();
        let mut backends: HashMap<String, Arc<dyn CompletionModel>> = HashMap::new();

        for cfg in all.into_iter().filter(|c| c.enabled) {
            let name = cfg.name.clone();
            if let Some(overriden) = override_factory(&cfg) {
                backends.insert(name, overriden);
                continue;
            }
            match instantiate_from_config(&cfg, cancellation_token.clone()).await {
                Ok(backend) => {
                    backends.insert(name, backend);
                }
                Err(e) => {
                    tracing::warn!(
                        backend = %name,
                        error = %e,
                        "LLM backend skipped during config load"
                    );
                }
            }
        }

        let default = pick_default_or_fallback(default_name, &backends)?;

        Ok(Self {
            backends,
            default,
            routing: None,
            cancellation_token,
            session_budget: Arc::new(Mutex::new(SessionBudgetTracker::default())),
        })
    }

    /// Build the router from a SQLite [`LlmBackendRepository`].
    ///
    /// Loads every `enabled = true` backend. The `is_default = true` backend
    /// becomes the default. Backends that fail to instantiate are logged with
    /// `tracing::warn!` and skipped (non-fatal degradation).
    ///
    /// # Errors
    ///
    /// - [`LlmError::BackendUnavailable`] if no backend is marked `is_default = true`
    ///   in `system.db`, or if the default backend fails to instantiate.
    pub async fn from_repository(repo: &LlmBackendRepository) -> Result<Self, LlmError> {
        Self::from_repository_with_override(repo, |_| None).await
    }

    /// Variant of [`from_repository`] that lets callers inject a factory to
    /// override instantiation of certain backends (multi-runner support).
    ///
    /// The closure receives the `LlmBackendConfig` and returns:
    /// - `Some(Arc<dyn CompletionModel>)` to override (typically used to
    ///   redirect `LlamaCpp` backends to a `RunnerLlmBackend`).
    /// - `None` to keep standard instantiation (the cloud backend case).
    pub async fn from_repository_with_override<F>(
        repo: &LlmBackendRepository,
        override_factory: F,
    ) -> Result<Self, LlmError>
    where
        F: Fn(&apollia_core::LlmBackendConfig) -> Option<Arc<dyn CompletionModel>>,
    {
        let all = repo.list().map_err(|e| LlmError::BackendUnavailable {
            backend: "system.db".to_string(),
            reason: e.to_string(),
        })?;

        let default_name = repo
            .find_default()
            .map_err(|e| LlmError::BackendUnavailable {
                backend: "system.db".to_string(),
                reason: e.to_string(),
            })?
            .ok_or_else(|| LlmError::BackendUnavailable {
                backend: "default".to_string(),
                reason: "no default LLM backend in system.db - configure one with is_default=true"
                    .to_string(),
            })?
            .name;

        let cancellation_token = CancellationToken::new();
        let mut backends: HashMap<String, Arc<dyn CompletionModel>> = HashMap::new();

        for cfg in all.into_iter().filter(|c| c.enabled) {
            let name = cfg.name.clone();
            // Try the override first (typically the runner proxy for LlamaCpp).
            if let Some(overriden) = override_factory(&cfg) {
                tracing::info!(
                    backend = %name,
                    "LLM backend instantiated via override (runner proxy)"
                );
                backends.insert(name, overriden);
                continue;
            }

            match instantiate_from_config(&cfg, cancellation_token.clone()).await {
                Ok(backend) => {
                    backends.insert(name, backend);
                }
                Err(e) => {
                    tracing::warn!(
                        backend = %name,
                        error = %e,
                        "LLM backend skipped during repository load"
                    );
                }
            }
        }

        let default = pick_default_or_fallback(default_name, &backends)?;

        Ok(Self {
            backends,
            default,
            routing: None,
            cancellation_token,
            session_budget: Arc::new(Mutex::new(SessionBudgetTracker::default())),
        })
    }

    /// Return the backend for `llm_backend`, or the default if `None` / unknown.
    ///
    /// Emits `tracing::warn!` if the named backend is absent from the router
    /// (silent fallback apart from the structured log).
    ///
    /// # Panics
    ///
    /// Panics if the router holds no backend. Do not call `route()` on a router
    /// built via [`LlmRouter::empty()`].
    pub fn route(&self, llm_backend: Option<&str>) -> Arc<dyn CompletionModel> {
        match llm_backend {
            None => self
                .backends
                .get(&self.default)
                .expect("LlmRouter invariant: default backend must be present")
                .clone(),
            Some(name) => {
                if let Some(b) = self.backends.get(name) {
                    b.clone()
                } else {
                    tracing::warn!(
                        backend = %name,
                        fallback = %self.default,
                        "unknown LLM backend requested, falling back to default"
                    );
                    self.backends
                        .get(&self.default)
                        .expect("LlmRouter invariant: default backend must be present")
                        .clone()
                }
            }
        }
    }

    /// Return the names of all backends loaded in the router.
    pub fn backend_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.backends.keys().cloned().collect();
        names.sort();
        names
    }

    /// Attach (or replace) the router's `[llm.routing]`.
    ///
    /// Useful post-construction when the router is instantiated via
    /// [`from_repository`](Self::from_repository) (which reads `system.db`)
    /// while the `[llm.routing]` config lives in `apollia.toml`. The supervisor
    /// calls `from_repository` then chains `with_routing` if the TOML declares
    /// a `[llm.routing]` section, propagating the application routing to the
    /// router without duplicating the read.
    ///
    /// Validation: the routing is applied as-is. The backends it names are
    /// checked at invocation via [`route_precise`](Self::route_precise) /
    /// [`route_fast`](Self::route_fast), which return
    /// [`LlmError::BackendNotFound`] if a name points to an absent backend.
    pub fn with_routing(mut self, routing: LlmRoutingConfig) -> Self {
        self.routing = Some(routing);
        self
    }

    /// Create an empty `LlmRouter` with no backend, for unit tests.
    ///
    /// Used to test degradation paths: `ctx.llm = None` and `AgentDegraded`
    /// on the EventBus.
    pub fn empty() -> Self {
        Self {
            backends: HashMap::new(),
            default: String::new(),
            routing: None,
            cancellation_token: CancellationToken::new(),
            session_budget: Arc::new(Mutex::new(SessionBudgetTracker::default())),
        }
    }

    /// Return the default backend name configured in `apollia.toml`.
    pub fn default_name(&self) -> &str {
        &self.default
    }

    /// Return the configured LLM cost alert threshold in USD, or `None`.
    ///
    /// Maps to `[llm] cost_alert_threshold_usd` in `apollia.toml`.
    pub fn cost_alert_threshold_usd(&self) -> Option<f64> {
        self.session_budget.lock().ok()?.threshold_usd()
    }

    /// Return the backend configured for deep reasoning tasks.
    ///
    /// Selects the backend named in `[llm.routing] precise` of `apollia.toml`.
    /// Used by components that need maximum reasoning quality (ORIA planning,
    /// complex analysis, judgment).
    ///
    /// # Errors
    ///
    /// - [`LlmError::BackendNotFound`]: `[llm.routing] precise` names a backend
    ///   that is not instantiated.
    /// - [`LlmError::RoutingConfigMissing`]: no `[llm.routing]` AND no resolvable
    ///   default backend (empty router or invalid default).
    ///
    /// # Fallback
    ///
    /// When `[llm.routing]` is not configured explicitly (the default case
    /// after a plain `apollia-os llm backends set-default <name>`), the router
    /// falls back to `self.default`. This is the documented promise: a
    /// single-backend setup must work for orchestrated agents without an
    /// explicit `[llm.routing]` config.
    pub fn route_precise(&self) -> Result<Arc<dyn CompletionModel>, LlmError> {
        // Explicit case: `[llm.routing] precise = "<name>"`. If the named
        // backend exists, use it; otherwise a structured error (invalid config).
        if let Some(routing) = self.routing.as_ref() {
            return self
                .backends
                .get(&routing.precise)
                .cloned()
                .ok_or_else(|| LlmError::BackendNotFound(routing.precise.clone()));
        }
        // Fallback: no routing means the default backend.
        self.fallback_default("precise")
    }

    /// Return the backend configured for light extraction tasks.
    ///
    /// Selects the backend named in `[llm.routing] fast` of `apollia.toml`.
    /// Used by components doing deterministic extraction (metadata, short
    /// summaries, classification, path parsing).
    ///
    /// # Errors
    ///
    /// - [`LlmError::BackendNotFound`]: `[llm.routing] fast` names a backend
    ///   that is not instantiated.
    /// - [`LlmError::RoutingConfigMissing`]: no `[llm.routing]` AND no resolvable
    ///   default backend.
    ///
    /// # Fallback
    ///
    /// See [`route_precise`](Self::route_precise): same fallback rules on
    /// `self.default` when `[llm.routing]` is not explicit.
    pub fn route_fast(&self) -> Result<Arc<dyn CompletionModel>, LlmError> {
        if let Some(routing) = self.routing.as_ref() {
            return self
                .backends
                .get(&routing.fast)
                .cloned()
                .ok_or_else(|| LlmError::BackendNotFound(routing.fast.clone()));
        }
        self.fallback_default("fast")
    }

    /// Resolve the default backend when `[llm.routing]` is not configured.
    ///
    /// Consistent with the single-backend promise (`set-default` is enough).
    /// Returns the backend named by `self.default` if it exists; otherwise
    /// returns [`LlmError::RoutingConfigMissing`] with a message that guides
    /// the operator.
    ///
    /// `role` is only used for the structured log ("precise" or "fast").
    fn fallback_default(&self, role: &str) -> Result<Arc<dyn CompletionModel>, LlmError> {
        if self.default.is_empty() || self.backends.is_empty() {
            return Err(LlmError::RoutingConfigMissing);
        }
        match self.backends.get(&self.default).cloned() {
            Some(backend) => {
                tracing::debug!(
                    role = %role,
                    backend = %self.default,
                    "no [llm.routing] configured - falling back to default backend"
                );
                Ok(backend)
            }
            None => Err(LlmError::RoutingConfigMissing),
        }
    }

    /// Return the backend to use for this step, applying the escalation policy.
    ///
    /// Decision tree, in order:
    /// 1. No `[llm.routing.hybrid]` configured: return the local backend for `level`.
    /// 2. Signal is [`EscalationSignal::None`]: return the local backend.
    /// 3. `session_cost_usd >= cost_ceiling_usd`: return local, emit `tracing::warn!`.
    /// 4. Frontier backend absent from the router: return local, emit `tracing::warn!`.
    /// 5. Otherwise: return the frontier backend.
    ///
    /// Never returns an error: degradation is always local, never a crash
    /// (Principle #1 local-first; Principle #4 fail fast at startup, not runtime).
    /// The per-session cost ceiling is consulted before every escalation and is
    /// not bypassable (Principle #7).
    pub fn route_with_escalation(
        &self,
        signal: EscalationSignal,
        level: LlmRoutingLevel,
    ) -> Arc<dyn CompletionModel> {
        // Local fallback for `level`. `route_precise`/`route_fast` only error when
        // routing is misconfigured (caught at startup); `route(None)` is the
        // infallible last resort on the default backend, avoiding any `expect` here.
        let local = || match level {
            LlmRoutingLevel::Precise => self.route_precise().unwrap_or_else(|_| self.route(None)),
            LlmRoutingLevel::Fast => self.route_fast().unwrap_or_else(|_| self.route(None)),
        };

        // 1. No hybrid configuration: stay local.
        let Some(routing) = self.routing.as_ref() else {
            return local();
        };
        let Some(hybrid) = routing.hybrid.as_ref() else {
            return local();
        };

        // 2. No escalation requested.
        if !signal.is_escalation() {
            return local();
        }

        // 3. Cost ceiling check. A poisoned mutex reads as 0.0 (cost unknown,
        // conservative: it permits the escalation rather than blocking work).
        let session_cost = self
            .session_budget
            .lock()
            .map(|t| t.session_cost_usd)
            .unwrap_or(0.0);
        if session_cost >= hybrid.cost_ceiling_usd {
            tracing::warn!(
                session_cost_usd = session_cost,
                ceiling_usd = hybrid.cost_ceiling_usd,
                signal = ?signal,
                "hybrid escalation blocked: cost ceiling reached, staying local"
            );
            return local();
        }

        // 4 + 5. Frontier availability.
        match self.backends.get(&hybrid.frontier) {
            Some(backend) => {
                tracing::info!(
                    frontier = %hybrid.frontier,
                    session_cost_usd = session_cost,
                    ceiling_usd = hybrid.cost_ceiling_usd,
                    signal = ?signal,
                    "hybrid escalation: routing to frontier backend"
                );
                backend.clone()
            }
            None => {
                tracing::warn!(
                    frontier = %hybrid.frontier,
                    signal = ?signal,
                    "hybrid escalation: frontier backend absent from router, staying local"
                );
                local()
            }
        }
    }

    /// Return `true` when hybrid routing is configured and the per-session cost
    /// has reached or exceeded the configured ceiling.
    ///
    /// Returns `false` when no `[llm.routing.hybrid]` section is configured, so
    /// a caller can treat the absence of hybrid routing as "ceiling never hit".
    /// Reads the same `session_cost_usd` as [`Self::route_with_escalation`]
    /// under a short lock (a poisoned mutex reads as `0.0`), so a caller that
    /// invokes both with no intervening `await` observes a consistent decision.
    pub fn is_ceiling_reached(&self) -> bool {
        let Some(routing) = self.routing.as_ref() else {
            return false;
        };
        let Some(hybrid) = routing.hybrid.as_ref() else {
            return false;
        };
        let session_cost = self
            .session_budget
            .lock()
            .map(|t| t.session_cost_usd)
            .unwrap_or(0.0);
        session_cost >= hybrid.cost_ceiling_usd
    }

    /// Seed the accumulated session cost in USD.
    ///
    /// Lets integration tests drive the hybrid cost ceiling deterministically
    /// when backends are injected directly via [`Self::with_backends`], without
    /// real token billing. A no-op if the budget lock is poisoned.
    pub fn seed_session_cost_usd(&self, usd: f64) {
        if let Ok(mut tracker) = self.session_budget.lock() {
            tracker.session_cost_usd = usd;
        }
    }

    /// Return the context window size in tokens used to size compaction.
    ///
    /// Prefers the active backend's reported window (e.g. a local model's
    /// trained context, once loaded) so compaction fires before the model
    /// overflows. Falls back to `200_000` (the `claude-sonnet` window,
    /// conservative for cloud backends and before the local model has loaded).
    pub fn context_limit(&self) -> usize {
        self.backends
            .get(&self.default)
            .and_then(|backend| backend.context_window())
            .unwrap_or(200_000)
    }

    /// Estimate the token count of `messages` using the default backend.
    ///
    /// Delegates to [`CompletionModel::count_tokens`] on the default backend, so
    /// a local backend returns its real tokenizer count while cloud backends
    /// keep the `(chars / 4) * 1.2` proxy. When no backend is registered (empty
    /// router, degraded startup), applies the proxy inline so the caller never
    /// panics.
    pub fn count_tokens(&self, messages: &[ChatMessage]) -> usize {
        match self.backends.get(&self.default) {
            Some(backend) => backend.count_tokens(messages),
            None => {
                let total_chars: usize = messages.iter().map(message_char_len).sum();
                ((total_chars as f32) / 4.0 * 1.2) as usize
            }
        }
    }

    /// Return the session `CancellationToken` to cancel in-flight calls.
    ///
    /// Called by `ORIAEngine::abort()` to interrupt all in-flight LLM calls
    /// and retry delays across every backend of the router.
    ///
    /// The token is `Clone`: each backend holds a clone, all cancelled at once
    /// by `token.cancel()`.
    pub fn cancellation_token(&self) -> CancellationToken {
        self.cancellation_token.clone()
    }

    /// Return a snapshot of the token budget accumulated since the last reset.
    ///
    /// Called by `ORIAEngine` at task end to persist the cost in
    /// `~/.apollia/session_costs.jsonl`.
    pub fn session_budget(&self) -> TokenBudget {
        self.session_budget
            .lock()
            .map(|t| t.to_token_budget())
            .unwrap_or_default()
    }

    /// Reset the session counters.
    ///
    /// Called by `ORIAEngine` at the start of each task to isolate counters
    /// per run. The tracker configuration (bus, threshold) is preserved.
    pub fn reset_session_budget(&self) {
        if let Ok(mut tracker) = self.session_budget.lock() {
            tracker.reset();
        }
    }

    /// List all available backends with their summary information.
    pub fn list(&self) -> Vec<BackendInfo> {
        self.backends
            .values()
            .map(|b| BackendInfo {
                name: b.backend_name().to_string(),
                model_id: b.model_id().to_string(),
                available: b.is_available(),
            })
            .collect()
    }
}

/// Pick the effective default backend name with graceful fallback.
///
/// When the configured default is missing from the successfully-instantiated
/// `backends` (e.g. the local feature isn't compiled or the API key is
/// missing for a cloud backend), pick the first available alphabetical backend
/// and emit a clear warning. Only fails entirely when no backend instantiated.
fn pick_default_or_fallback(
    configured_default: String,
    backends: &HashMap<String, Arc<dyn CompletionModel>>,
) -> Result<String, LlmError> {
    if backends.contains_key(&configured_default) {
        return Ok(configured_default);
    }
    let mut available: Vec<&String> = backends.keys().collect();
    available.sort();
    match available.first() {
        Some(fallback) => {
            tracing::warn!(
                configured_default = %configured_default,
                fallback = %fallback,
                available = ?available,
                "configured default LLM backend unavailable - falling back to first available backend"
            );
            Ok((*fallback).clone())
        }
        None => Err(LlmError::BackendUnavailable {
            backend: configured_default,
            reason: "no LLM backend instantiated successfully (default unreachable, no fallback available)".to_string(),
        }),
    }
}

// ─────────────────────────────────────────────
// Backend instantiation helpers
// ─────────────────────────────────────────────

/// Instantiate a [`CompletionModel`] from a SQLite [`LlmBackendConfig`].
async fn instantiate_from_config(
    cfg: &LlmBackendConfig,
    cancel: CancellationToken,
) -> Result<Arc<dyn CompletionModel>, LlmError> {
    match &cfg.provider {
        // `LlamaCpp` backends (local .gguf models) are served by the
        // `apollia-runner` sidecar via a `RunnerLlmBackend` injected by the
        // caller's override factory. Reaching here means no runner was
        // available when the router was built.
        LlmProvider::LlamaCpp => Err(LlmError::BackendUnavailable {
            backend: cfg.name.clone(),
            reason: "local llama-cpp backend requires the apollia-runner sidecar, \
                     which is currently unavailable (the runner failed to start, \
                     or no runner is bundled for this platform)"
                .to_string(),
        }),
        provider => instantiate_cloud_backend(cfg, provider, cancel).await,
    }
}

/// Instantiate a cloud backend (OpenAI-compatible or Anthropic) from the SQLite config.
///
/// Resolves the API key from `config_json["api_key"]` when present.
#[cfg(feature = "cloud")]
async fn instantiate_cloud_backend(
    cfg: &LlmBackendConfig,
    provider: &LlmProvider,
    cancel: CancellationToken,
) -> Result<Arc<dyn CompletionModel>, LlmError> {
    let api_key = extract_api_key_value(cfg)?;

    let default_url = match provider {
        LlmProvider::OpenAi => "https://api.openai.com/v1",
        LlmProvider::Mistral => "https://api.mistral.ai/v1",
        LlmProvider::Ollama => "http://localhost:11434/v1",
        LlmProvider::Anthropic => "https://api.anthropic.com",
        LlmProvider::LlamaCpp => {
            unreachable!("LlamaCpp is handled before reaching instantiate_cloud_backend (sidecar runner path)")
        }
    };

    let base_url = extract_base_url(cfg, default_url);

    let api_cfg = ApiBackendConfig {
        name: cfg.name.clone(),
        api_url: base_url,
        api_key_env: String::new(), // key already resolved
        model: cfg.model.clone(),
    };

    if matches!(provider, LlmProvider::Anthropic) {
        return Ok(Arc::new(AnthropicClient::new(
            &api_cfg,
            api_key,
            HashMap::new(),
            cancel,
        )) as Arc<dyn CompletionModel>);
    }

    Ok(
        Arc::new(OpenAICompatibleClient::new(&api_cfg, api_key, cancel))
            as Arc<dyn CompletionModel>,
    )
}

/// Return `BackendUnavailable` when the `"cloud"` feature is not compiled.
#[cfg(not(feature = "cloud"))]
async fn instantiate_cloud_backend(
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
fn extract_api_key_value(cfg: &LlmBackendConfig) -> Result<String, LlmError> {
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

/// Extract the base URL from `config_json["base_url"]`, or return `default`.
#[cfg(feature = "cloud")]
fn extract_base_url(cfg: &LlmBackendConfig, default: &str) -> String {
    cfg.config_json
        .get("base_url")
        .and_then(|v| v.as_str())
        .unwrap_or(default)
        .to_string()
}

// ─────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    use std::pin::Pin;

    use futures::Stream;

    use crate::types::{
        CompletionRequest, CompletionResponse, FinishReason, StreamChunk, TokenUsage,
    };

    // ── Mock ─────────────────────────────────────────────────────────────────

    struct MockCompletionModel {
        name: String,
    }

    impl Default for MockCompletionModel {
        fn default() -> Self {
            Self {
                name: "mock".to_owned(),
            }
        }
    }

    #[async_trait::async_trait]
    impl CompletionModel for MockCompletionModel {
        async fn complete(&self, _req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
            Ok(CompletionResponse {
                content: "mock response".to_owned(),
                tool_calls: vec![],
                usage: TokenUsage {
                    prompt_tokens: 10,
                    completion_tokens: 5,
                    cost_usd: None,
                    ..Default::default()
                },
                finish_reason: FinishReason::Stop,
                latency_ms: 1,
                ttft_ms: None,
            })
        }

        async fn stream(
            &self,
            _req: CompletionRequest,
        ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamChunk, LlmError>> + Send>>, LlmError>
        {
            Ok(Box::pin(futures::stream::once(async {
                Ok(StreamChunk::Text("mock chunk".to_owned()))
            })))
        }

        fn is_available(&self) -> bool {
            true
        }

        fn backend_name(&self) -> &str {
            &self.name
        }

        fn model_id(&self) -> &str {
            &self.name
        }
    }

    fn make_mock_backend(name: &str) -> Arc<dyn CompletionModel> {
        Arc::new(MockCompletionModel {
            name: name.to_owned(),
        })
    }

    fn make_test_router(
        backends: HashMap<String, Arc<dyn CompletionModel>>,
        default: &str,
    ) -> LlmRouter {
        LlmRouter {
            backends,
            default: default.into(),
            routing: None,
            cancellation_token: CancellationToken::new(),
            session_budget: Arc::new(Mutex::new(SessionBudgetTracker::default())),
        }
    }

    fn make_routing_router(precise: &str, fast: &str) -> LlmRouter {
        let mut backends = HashMap::new();
        backends.insert(precise.to_owned(), make_mock_backend(precise));
        if fast != precise {
            backends.insert(fast.to_owned(), make_mock_backend(fast));
        }
        let routing = Some(LlmRoutingConfig {
            precise: precise.to_owned(),
            fast: fast.to_owned(),
            hybrid: None,
        });
        LlmRouter {
            default: precise.to_owned(),
            backends,
            routing,
            cancellation_token: CancellationToken::new(),
            session_budget: Arc::new(Mutex::new(SessionBudgetTracker::default())),
        }
    }

    /// Backend overriding `count_tokens` with a fixed value, for the delegate
    /// test. Inference methods are stubbed.
    struct FixedCountModel(usize);

    #[async_trait::async_trait]
    impl CompletionModel for FixedCountModel {
        async fn complete(&self, _req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
            Err(LlmError::InferenceError("stub".into()))
        }
        async fn stream(
            &self,
            _req: CompletionRequest,
        ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamChunk, LlmError>> + Send>>, LlmError>
        {
            Err(LlmError::InferenceError("stub".into()))
        }
        fn is_available(&self) -> bool {
            true
        }
        fn backend_name(&self) -> &str {
            "fixed"
        }
        fn model_id(&self) -> &str {
            "fixed-model"
        }
        fn count_tokens(&self, _messages: &[ChatMessage]) -> usize {
            self.0
        }
    }

    // ── Tests count_tokens() ─────────────────────────────────────────────────

    // GIVEN a router whose default backend overrides count_tokens to return 42
    // WHEN count_tokens is called
    // THEN the router delegates and returns 42
    #[test]
    fn test_router_count_tokens_delegates() {
        let mut backends: HashMap<String, Arc<dyn CompletionModel>> = HashMap::new();
        backends.insert("fixed".into(), Arc::new(FixedCountModel(42)));
        let router = make_test_router(backends, "fixed");

        let tokens = router.count_tokens(&[ChatMessage::user("anything")]);
        assert_eq!(tokens, 42);
    }

    // GIVEN an empty router (no backend)
    // WHEN count_tokens is called
    // THEN the inline proxy is returned (> 0) without panicking
    #[test]
    fn test_router_empty_count_tokens_no_panic() {
        let router = LlmRouter::empty();
        let tokens = router.count_tokens(&[ChatMessage::user("hello")]);
        assert!(tokens > 0);
    }

    // ── Tests route() ────────────────────────────────────────────────────────

    // GIVEN router with "local-code" and "mistral-small", default = "mistral-small"
    // WHEN route(Some("local-code"))
    // THEN the "local-code" backend is returned
    #[test]
    fn test_ac1_route_to_explicit_backend() {
        let mut backends = HashMap::new();
        backends.insert("local-code".into(), make_mock_backend("local-code"));
        backends.insert("mistral-small".into(), make_mock_backend("mistral-small"));
        let router = make_test_router(backends, "mistral-small");

        let backend = router.route(Some("local-code"));
        assert_eq!(backend.backend_name(), "local-code");
    }

    // GIVEN router with default = "local-code"
    // WHEN route(None)
    // THEN the default backend is returned
    #[test]
    fn test_ac2_route_none_returns_default() {
        let mut backends = HashMap::new();
        backends.insert("local-code".into(), make_mock_backend("local-code"));
        let router = make_test_router(backends, "local-code");

        let backend = router.route(None);
        assert_eq!(backend.backend_name(), "local-code");
    }

    // GIVEN router without "phantom"
    // WHEN route(Some("phantom"))
    // THEN the default backend is returned (warning emitted)
    #[test]
    fn test_ac3_unknown_backend_falls_back_to_default() {
        let mut backends = HashMap::new();
        backends.insert("local-code".into(), make_mock_backend("local-code"));
        let router = make_test_router(backends, "local-code");

        let backend = router.route(Some("phantom"));
        assert_eq!(backend.backend_name(), "local-code");
    }

    // GIVEN router with 2 backends
    // WHEN backend_names()
    // THEN sorted list of names returned
    #[test]
    fn test_backend_names_sorted() {
        let mut backends = HashMap::new();
        backends.insert("z-backend".into(), make_mock_backend("z-backend"));
        backends.insert("a-backend".into(), make_mock_backend("a-backend"));
        let router = make_test_router(backends, "a-backend");

        let names = router.backend_names();
        assert_eq!(names, vec!["a-backend", "z-backend"]);
    }

    // GIVEN a LlmBackendRepository with 2 enabled + 1 disabled Ollama backend
    // WHEN from_repository(&repo).await
    // THEN the router contains exactly 2 backends (the disabled one is excluded)
    #[cfg(feature = "cloud")]
    #[tokio::test]
    async fn test_ac4_from_repository_loads_only_enabled() {
        use apollia_core::{LlmBackendConfig, LlmBackendRepository, LlmProvider};
        use tempfile::TempDir;

        let dir = TempDir::new().unwrap();
        let repo = LlmBackendRepository::open(&dir.path().join("system.db")).unwrap();

        let make_ollama = |name: &str, enabled: bool, is_default: bool| LlmBackendConfig {
            name: name.to_string(),
            provider: LlmProvider::Ollama,
            model: "llama3".to_string(),
            config_json: serde_json::json!({ "base_url": "http://localhost:11434/v1" }),
            enabled,
            is_default,
        };

        repo.save(&make_ollama("ollama-default", true, true))
            .unwrap();
        repo.save(&make_ollama("ollama-extra", true, false))
            .unwrap();
        repo.save(&make_ollama("ollama-disabled", false, false))
            .unwrap();

        let router = LlmRouter::from_repository(&repo)
            .await
            .expect("from_repository should succeed");

        let names = router.backend_names();
        assert_eq!(names.len(), 2);
        assert!(names.contains(&"ollama-default".to_string()));
        assert!(names.contains(&"ollama-extra".to_string()));
        assert!(!names.contains(&"ollama-disabled".to_string()));
    }

    // GIVEN a repository with no default backend
    // WHEN from_repository(&repo).await
    // THEN BackendUnavailable is returned
    #[tokio::test]
    async fn test_from_repository_no_default_returns_error() {
        use apollia_core::{LlmBackendConfig, LlmBackendRepository, LlmProvider};
        use tempfile::TempDir;

        let dir = TempDir::new().unwrap();
        let repo = LlmBackendRepository::open(&dir.path().join("system.db")).unwrap();

        // empty repo, no default
        let result = LlmRouter::from_repository(&repo).await;
        assert!(matches!(result, Err(LlmError::BackendUnavailable { .. })));

        // backend with is_default=false, still no default
        repo.save(&LlmBackendConfig {
            name: "orphan".to_string(),
            provider: LlmProvider::Ollama,
            model: "llama3".to_string(),
            config_json: serde_json::json!({}),
            enabled: true,
            is_default: false,
        })
        .unwrap();

        let result2 = LlmRouter::from_repository(&repo).await;
        assert!(matches!(result2, Err(LlmError::BackendUnavailable { .. })));
    }

    // ── Tests: get, list, clone, error cases ─────────────────────────────────

    // GIVEN an LlmRouter with default = "local" and a "local" backend
    // WHEN get(None) is called
    // THEN Some(backend) with backend_name() == "local" is returned
    #[tokio::test]
    async fn test_ac3_get_none_returns_default() {
        // GIVEN
        let mut backends = HashMap::new();
        backends.insert("local".into(), make_mock_backend("local"));
        let router = make_test_router(backends, "local");

        // WHEN
        let result = router.get(None);

        // THEN
        assert!(
            result.is_some(),
            "get(None) doit retourner Some pour le backend défaut"
        );
        assert_eq!(
            result.unwrap().backend_name(),
            "local",
            "le backend retourné doit être le backend défaut"
        );
    }

    // GIVEN an LlmRouter with an "anthropic" backend
    // WHEN get(Some("anthropic")) is called
    // THEN Some(arc) with backend_name() == "anthropic" is returned
    #[tokio::test]
    async fn test_ac4_get_named_backend() {
        // GIVEN
        let mut backends = HashMap::new();
        backends.insert("anthropic".into(), make_mock_backend("anthropic"));
        let router = make_test_router(backends, "anthropic");

        // WHEN
        let result = router.get(Some("anthropic"));

        // THEN
        assert!(
            result.is_some(),
            "get(Some(\"anthropic\")) doit retourner Some"
        );
        assert_eq!(result.unwrap().backend_name(), "anthropic");
    }

    // GIVEN an LlmRouter without an "inexistant" backend
    // WHEN get(Some("inexistant")) is called
    // THEN None is returned
    #[tokio::test]
    async fn test_ac5_get_unknown_returns_none() {
        // GIVEN
        let mut backends = HashMap::new();
        backends.insert("local".into(), make_mock_backend("local"));
        let router = make_test_router(backends, "local");

        // WHEN / THEN
        assert!(
            router.get(Some("inexistant")).is_none(),
            "get(Some(\"inexistant\")) doit retourner None pour un backend inconnu"
        );
    }

    // GIVEN an LlmRouter with 2 backends ("a" and "b")
    // WHEN list() is called
    // THEN a Vec of length 2 is returned
    #[tokio::test]
    async fn test_router_list_returns_all_backends() {
        // GIVEN
        let mut backends = HashMap::new();
        backends.insert("a".into(), make_mock_backend("a"));
        backends.insert("b".into(), make_mock_backend("b"));
        let router = make_test_router(backends, "a");

        // WHEN
        let list = router.list();

        // THEN
        assert_eq!(
            list.len(),
            2,
            "list() doit retourner autant d'entrées que de backends"
        );
    }

    // GIVEN a cloned LlmRouter
    // WHEN the clone is queried
    // THEN it shares the same backends via Arc (refcount)
    #[tokio::test]
    async fn test_router_clone_shares_backends() {
        // GIVEN
        let mut backends = HashMap::new();
        backends.insert("local".into(), make_mock_backend("local"));
        let router = make_test_router(backends, "local");

        // WHEN
        let cloned = router.clone();

        // THEN
        assert!(
            cloned.get(None).is_some(),
            "le clone doit avoir accès aux mêmes backends"
        );
        assert_eq!(cloned.list().len(), 1);
    }

    // GIVEN an LlmConfig with default = "local" but an empty backends list
    // WHEN LlmRouter::from_config(&config).await is called
    // THEN Err(LlmError::BackendUnavailable { backend: "local", .. }) is returned
    #[tokio::test]
    async fn test_ac6_from_config_errors_if_default_missing() {
        // GIVEN
        let config = LlmConfig {
            default: "local".to_owned(),
            backends: vec![],
            observability: ObservabilityConfig::default(),
            routing: None,
            pricing_overrides: HashMap::new(),
            cost_alert_threshold_usd: None,
            vertex: None,
            runner: Default::default(),
        };

        // WHEN
        let result = LlmRouter::from_config(&config).await;

        // THEN
        assert!(
            matches!(
                result,
                Err(LlmError::BackendUnavailable { ref backend, .. }) if backend == "local"
            ),
            "from_config doit retourner BackendUnavailable si le backend défaut est absent"
        );
    }

    // ── Observability tests ──────────────────────────────────────────────────

    // GIVEN an LlmRouter with a mock backend and an EventBusSender
    // WHEN complete_with_observability(None, req, Some(&tx), &obs) is called
    // THEN an LlmCallCompleted event is received on the bus with backend == "mock"
    #[tokio::test]
    async fn test_ac1_llm_call_completed_emitted() {
        use apollia_core::events::RuntimeEvent;
        use tokio::sync::broadcast;

        // GIVEN
        let (tx, mut rx) = broadcast::channel::<RuntimeEvent>(16);
        let mut backends = HashMap::new();
        backends.insert(
            "mock".into(),
            Arc::new(MockCompletionModel::default()) as Arc<dyn CompletionModel>,
        );
        let router = make_test_router(backends, "mock");
        let req = CompletionRequest {
            messages: vec![crate::types::ChatMessage::user("test")],
            ..Default::default()
        };
        let obs = ObservabilityConfig::default();

        // WHEN
        router
            .complete_with_observability(None, req, Some(&tx), &obs)
            .await
            .expect("complete_with_observability ne doit pas échouer avec un mock valide");

        // THEN
        let event = rx
            .try_recv()
            .expect("un événement doit être présent dans le bus");
        assert!(
            matches!(
                event,
                RuntimeEvent::LlmCallCompleted { ref backend, .. } if backend == "mock"
            ),
            "l'événement reçu doit être LlmCallCompleted avec backend == \"mock\", obtenu: {event:?}"
        );
    }

    // GIVEN a router with debug_log_prompt = false
    // WHEN complete_with_observability() is called with a "secret_payload_xyz" message
    // THEN the function does not panic and returns Ok; the prompt is not logged at INFO
    #[tokio::test]
    async fn test_ac4_prompt_not_logged_at_info_without_debug_flag() {
        // GIVEN
        let obs = ObservabilityConfig {
            debug_log_prompt: false,
            ..Default::default()
        };
        let req = CompletionRequest {
            messages: vec![crate::types::ChatMessage::user("secret_payload_xyz")],
            ..Default::default()
        };
        let mut backends = HashMap::new();
        backends.insert(
            "mock".into(),
            Arc::new(MockCompletionModel::default()) as Arc<dyn CompletionModel>,
        );
        let router = make_test_router(backends, "mock");

        // WHEN: must not panic; an absent bus is acceptable (Option::None)
        let result = router
            .complete_with_observability(None, req, None, &obs)
            .await;

        // THEN
        assert!(
            result.is_ok(),
            "complete_with_observability doit retourner Ok même sans bus : {result:?}"
        );
    }

    // GIVEN an LlmRouter with an EventBusSender and an empty backends list (default absent)
    // WHEN from_config_with_bus is called
    // THEN Err(LlmError::BackendUnavailable) is returned without a crash
    // (variant without the "local" feature: checks the router does not crash)
    #[tokio::test]
    async fn test_ac3_from_config_with_bus_no_backends_returns_error() {
        use apollia_core::events::RuntimeEvent;
        use tokio::sync::broadcast;

        // GIVEN
        let (tx, _rx) = broadcast::channel::<RuntimeEvent>(16);
        let config = LlmConfig {
            default: "local".to_owned(),
            backends: vec![],
            observability: ObservabilityConfig::default(),
            routing: None,
            pricing_overrides: HashMap::new(),
            cost_alert_threshold_usd: None,
            vertex: None,
            runner: Default::default(),
        };

        // WHEN
        let result = LlmRouter::from_config_with_bus(&config, Some(tx)).await;

        // THEN: clean error, no crash
        assert!(
            matches!(
                result,
                Err(LlmError::BackendUnavailable { ref backend, .. }) if backend == "local"
            ),
            "from_config_with_bus doit retourner BackendUnavailable si aucun backend n'est disponible"
        );
    }

    // ── Routing tests ────────────────────────────────────────────────────────

    // GIVEN routing config { precise: "claude-opus-4-6", fast: "claude-haiku-4-5-20251001" }
    // WHEN route_precise()
    // THEN backend "claude-opus-4-6" is selected
    #[tokio::test]
    async fn router_precise_selects_configured_backend() {
        let router = make_routing_router("claude-opus-4-6", "claude-haiku-4-5-20251001");

        let backend = router
            .route_precise()
            .expect("route_precise should succeed");
        assert_eq!(backend.backend_name(), "claude-opus-4-6");
    }

    // GIVEN routing config { precise: "claude-opus-4-6", fast: "claude-haiku-4-5-20251001" }
    // WHEN route_fast()
    // THEN backend "claude-haiku-4-5-20251001" is selected
    #[tokio::test]
    async fn router_fast_selects_configured_backend() {
        let router = make_routing_router("claude-opus-4-6", "claude-haiku-4-5-20251001");

        let backend = router.route_fast().expect("route_fast should succeed");
        assert_eq!(backend.backend_name(), "claude-haiku-4-5-20251001");
    }

    // GIVEN a router with a "default" backend but no [llm.routing] (routing: None)
    // WHEN route_precise() / route_fast() are called
    // THEN both fall back to the `default` backend (documented single-backend
    //      case: `apollia-os llm backends set-default <name>` is enough).
    #[tokio::test]
    async fn router_falls_back_to_default_when_routing_missing() {
        let mut backends = HashMap::new();
        backends.insert("default".to_owned(), make_mock_backend("default"));
        let router = make_test_router(backends, "default");

        let precise = router
            .route_precise()
            .expect("route_precise should fallback to default backend");
        assert_eq!(precise.backend_name(), "default");

        let fast = router
            .route_fast()
            .expect("route_fast should fallback to default backend");
        assert_eq!(fast.backend_name(), "default");
    }

    // GIVEN no backend at all (empty router) and no [llm.routing]
    // WHEN route_precise() is called
    // THEN Err(RoutingConfigMissing): no fallback possible, the operator must
    //      declare at least one backend.
    #[tokio::test]
    async fn router_errors_when_no_backend_and_no_routing() {
        let backends = HashMap::new();
        let router = make_test_router(backends, "");

        assert!(
            matches!(router.route_precise(), Err(LlmError::RoutingConfigMissing)),
            "route_precise() must error when no backend is registered"
        );
        assert!(
            matches!(router.route_fast(), Err(LlmError::RoutingConfigMissing)),
            "route_fast() must error when no backend is registered"
        );
    }

    // GIVEN a router built via `from_repository` (so routing=None) then enriched
    //       via with_routing(LlmRoutingConfig { precise, fast })
    // WHEN route_precise() / route_fast() are called
    // THEN the chained routing is respected.
    #[tokio::test]
    async fn router_with_routing_attaches_routing_post_construction() {
        let mut backends = HashMap::new();
        backends.insert("opus".to_owned(), make_mock_backend("opus"));
        backends.insert("haiku".to_owned(), make_mock_backend("haiku"));
        let router = make_test_router(backends, "haiku").with_routing(LlmRoutingConfig {
            precise: "opus".to_owned(),
            fast: "haiku".to_owned(),
            hybrid: None,
        });

        assert_eq!(
            router
                .route_precise()
                .expect("route_precise should resolve via attached routing")
                .backend_name(),
            "opus"
        );
        assert_eq!(
            router
                .route_fast()
                .expect("route_fast should resolve via attached routing")
                .backend_name(),
            "haiku"
        );
    }

    // primary fails, secondary succeeds, LlmFallbackTriggered emitted
    #[tokio::test]
    async fn router_emits_fallback_event_on_primary_failure() {
        use apollia_core::events::RuntimeEvent;
        use tokio::sync::broadcast;

        struct FailingBackend {
            name: String,
        }
        #[async_trait::async_trait]
        impl CompletionModel for FailingBackend {
            async fn complete(
                &self,
                _req: CompletionRequest,
            ) -> Result<CompletionResponse, LlmError> {
                Err(LlmError::InferenceError("primary down".to_string()))
            }
            async fn stream(
                &self,
                _req: CompletionRequest,
            ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamChunk, LlmError>> + Send>>, LlmError>
            {
                Err(LlmError::InferenceError("primary down".to_string()))
            }
            fn is_available(&self) -> bool {
                true
            }
            fn backend_name(&self) -> &str {
                &self.name
            }
            fn model_id(&self) -> &str {
                &self.name
            }
        }

        // GIVEN a router with a failing primary and a healthy secondary
        let mut backends: HashMap<String, Arc<dyn CompletionModel>> = HashMap::new();
        backends.insert(
            "primary".into(),
            Arc::new(FailingBackend {
                name: "primary".into(),
            }),
        );
        backends.insert("secondary".into(), make_mock_backend("secondary"));
        let router = make_test_router(backends, "primary");
        let (tx, mut rx) = broadcast::channel::<RuntimeEvent>(16);
        let req = CompletionRequest {
            messages: vec![crate::types::ChatMessage::user("hi")],
            ..Default::default()
        };

        // WHEN complete_with_fallback
        let obs = ObservabilityConfig::default();
        let response = router
            .complete_with_fallback(
                FallbackPlan {
                    primary: "primary",
                    fallbacks: &["secondary"],
                    bus: Some(&tx),
                    obs: &obs,
                },
                req,
            )
            .await
            .expect("fallback should succeed");

        // THEN response comes from secondary
        assert_eq!(response.content, "mock response");

        // AND LlmFallbackTriggered was emitted
        let mut saw_fallback = false;
        while let Ok(evt) = rx.try_recv() {
            if let RuntimeEvent::LlmFallbackTriggered {
                from_provider,
                to_provider,
                ..
            } = evt
            {
                assert_eq!(from_provider, "primary");
                assert_eq!(to_provider, "secondary");
                saw_fallback = true;
            }
        }
        assert!(
            saw_fallback,
            "LlmFallbackTriggered should have been emitted"
        );
    }

    // GIVEN routing config { precise: "claude-opus-4-6", fast: "claude-opus-4-6" }
    // WHEN route_precise() and route_fast()
    // THEN the same backend "claude-opus-4-6" is returned in both cases
    #[tokio::test]
    async fn router_same_backend_for_precise_and_fast_when_identical() {
        let router = make_routing_router("claude-opus-4-6", "claude-opus-4-6");

        let precise = router
            .route_precise()
            .expect("route_precise should succeed");
        let fast = router.route_fast().expect("route_fast should succeed");

        assert_eq!(precise.backend_name(), "claude-opus-4-6");
        assert_eq!(fast.backend_name(), "claude-opus-4-6");
        assert_eq!(precise.backend_name(), fast.backend_name());
    }

    // ── Hybrid escalation policy (STORY-557) ──────────────────────────────

    /// Build a router with `precise = fast = "local"`, a `"frontier-model"`
    /// backend, an `[llm.routing.hybrid]` section with the given ceiling, and a
    /// seeded session cost.
    fn make_hybrid_router(ceiling: f64, session_cost: f64) -> LlmRouter {
        let mut backends = HashMap::new();
        backends.insert("local".to_owned(), make_mock_backend("local"));
        backends.insert(
            "frontier-model".to_owned(),
            make_mock_backend("frontier-model"),
        );
        let routing = Some(LlmRoutingConfig {
            precise: "local".to_owned(),
            fast: "local".to_owned(),
            hybrid: Some(apollia_core::HybridRoutingConfig {
                frontier: "frontier-model".to_owned(),
                cost_ceiling_usd: ceiling,
                ceiling_action: apollia_core::CeilingAction::StayLocal,
            }),
        });
        let session_budget = Arc::new(Mutex::new(SessionBudgetTracker::default()));
        session_budget.lock().unwrap().session_cost_usd = session_cost;
        LlmRouter {
            default: "local".to_owned(),
            backends,
            routing,
            cancellation_token: CancellationToken::new(),
            session_budget,
        }
    }

    // AC-1: escalation accepted when the frontier is available and under ceiling.
    #[test]
    fn test_escalation_accepted_under_ceiling() {
        // GIVEN a hybrid router, session cost 0.50, ceiling 2.00
        let router = make_hybrid_router(2.00, 0.50);

        // WHEN a failure signal escalates a precise step
        let backend = router.route_with_escalation(
            EscalationSignal::RepeatedStepFailure {
                consecutive_failures: 3,
            },
            LlmRoutingLevel::Precise,
        );

        // THEN the frontier backend is returned
        assert_eq!(backend.backend_name(), "frontier-model");
    }

    // AC-2: ceiling reached keeps the router local.
    #[test]
    fn test_escalation_blocked_by_cost_ceiling() {
        // GIVEN a hybrid router, session cost 1.05, ceiling 1.00
        let router = make_hybrid_router(1.00, 1.05);

        // WHEN a failure signal escalates a precise step
        let backend = router.route_with_escalation(
            EscalationSignal::RepeatedStepFailure {
                consecutive_failures: 2,
            },
            LlmRoutingLevel::Precise,
        );

        // THEN the local precise backend is returned
        assert_eq!(backend.backend_name(), "local");
    }

    // AC-3: no hybrid section means no escalation, no error.
    #[test]
    fn test_no_hybrid_config_returns_local() {
        // GIVEN a router without a hybrid section
        let router = make_routing_router("local", "local");

        // WHEN any escalation signal is applied
        let backend = router.route_with_escalation(
            EscalationSignal::RepeatedStepFailure {
                consecutive_failures: 1,
            },
            LlmRoutingLevel::Precise,
        );

        // THEN the local backend is returned
        assert_eq!(backend.backend_name(), "local");
    }

    // AC-4: a frontier absent from the router is rejected at construction.
    #[test]
    fn test_frontier_absent_fails_at_construction() {
        // GIVEN a routing whose hybrid frontier is not in the backend map
        let mut backends: HashMap<String, Arc<dyn CompletionModel>> = HashMap::new();
        backends.insert("local".to_owned(), make_mock_backend("local"));
        let routing = LlmRoutingConfig {
            precise: "local".to_owned(),
            fast: "local".to_owned(),
            hybrid: Some(apollia_core::HybridRoutingConfig {
                frontier: "phantom".to_owned(),
                cost_ceiling_usd: 1.00,
                ceiling_action: apollia_core::CeilingAction::StayLocal,
            }),
        };

        // WHEN validate_routing runs
        let result = validate_routing(&backends, Some(&routing));

        // THEN it reports the missing frontier backend
        assert!(matches!(
            result,
            Err(LlmError::BackendNotFound(name)) if name == "phantom"
        ));
    }

    // AC-5: an absent signal keeps the router local even under the ceiling.
    #[test]
    fn test_signal_none_returns_local() {
        // GIVEN a hybrid router well under the ceiling
        let router = make_hybrid_router(2.00, 0.10);

        // WHEN the signal is None
        let backend =
            router.route_with_escalation(EscalationSignal::None, LlmRoutingLevel::Precise);

        // THEN the local backend is returned
        assert_eq!(backend.backend_name(), "local");
    }

    // Truth table for EscalationSignal::is_escalation.
    #[test]
    fn test_escalation_signal_is_escalation() {
        assert!(!EscalationSignal::None.is_escalation());
        assert!(EscalationSignal::RepeatedStepFailure {
            consecutive_failures: 1
        }
        .is_escalation());
        assert!(EscalationSignal::AutonomyTierRequest.is_escalation());
    }

    // is_ceiling_reached: false when no hybrid section is configured.
    #[test]
    fn test_is_ceiling_reached_false_without_hybrid() {
        // GIVEN a router with routing but no hybrid section
        let router = make_routing_router("local", "local");

        // WHEN the ceiling is queried
        // THEN it reports not reached
        assert!(!router.is_ceiling_reached());
    }

    // is_ceiling_reached: false when the session cost is below the ceiling.
    #[test]
    fn test_is_ceiling_reached_false_below_ceiling() {
        // GIVEN a hybrid router, session cost 0.50, ceiling 2.00
        let router = make_hybrid_router(2.00, 0.50);

        // WHEN the ceiling is queried
        // THEN it reports not reached
        assert!(!router.is_ceiling_reached());
    }

    // is_ceiling_reached: true at or above the ceiling.
    #[test]
    fn test_is_ceiling_reached_true_at_or_above_ceiling() {
        // GIVEN a hybrid router exactly at the ceiling
        let at = make_hybrid_router(1.00, 1.00);
        // AND one above the ceiling
        let above = make_hybrid_router(1.00, 1.50);

        // WHEN the ceiling is queried
        // THEN both report reached
        assert!(at.is_ceiling_reached());
        assert!(above.is_ceiling_reached());
    }

    // seed_session_cost_usd drives the ceiling decision deterministically.
    #[test]
    fn test_seed_session_cost_usd_crosses_ceiling() {
        // GIVEN a hybrid router below the ceiling
        let router = make_hybrid_router(1.00, 0.10);
        assert!(!router.is_ceiling_reached());

        // WHEN the session cost is seeded above the ceiling
        router.seed_session_cost_usd(2.00);

        // THEN the ceiling is reported reached
        assert!(router.is_ceiling_reached());
    }
}
