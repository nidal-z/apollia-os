//! The router's declarative configuration.
//!
//! Split out of `router.rs`: the router stays in the parent, the TOML shapes
//! it is built from and the provider inference that reads them live here.

use std::collections::HashMap;

use apollia_core::{
    LlmBackendConfig, LlmProvider, LlmRoutingConfig, LlmRunnerConfig, VertexConfig,
};

use crate::pricing::PricingTier;

#[cfg(feature = "cloud")]
use crate::backends::openai::ApiBackendConfig;

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
pub(crate) fn backend_config_to_db(cfg: &BackendConfig, is_default: bool) -> LlmBackendConfig {
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
/// Default port Ollama listens on. Used only to recognise an Ollama backend
/// declared in a legacy TOML file, never to build a URL: the host and port
/// always come from the configured `api_url`.
#[cfg(feature = "cloud")]
const OLLAMA_DEFAULT_PORT: &str = ":11434";
/// Infers a [`LlmProvider`] from the API base URL.
///
/// Only used when migrating a legacy TOML backend into the database, where no
/// explicit provider is recorded. Matching on the port alone (rather than on
/// `localhost:11434`) keeps an Ollama server running on another machine
/// recognisable, which is the normal case as soon as inference is offloaded to
/// a second host.
#[cfg(feature = "cloud")]
pub(crate) fn infer_api_provider_from_url(api_url: &str) -> LlmProvider {
    if api_url.contains("anthropic.com") {
        LlmProvider::Anthropic
    } else if api_url.contains("mistral.ai") {
        LlmProvider::Mistral
    } else if api_url.contains(OLLAMA_DEFAULT_PORT) || api_url.contains("ollama") {
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
pub(crate) fn default_true() -> bool {
    true
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
    pub(crate) fn model_path_hint(&self) -> String {
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
