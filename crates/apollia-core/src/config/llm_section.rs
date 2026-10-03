//! The TOML shapes of the `[llm]` section of `apollia.toml`.
//!
//! They live here, rather than in `apollia-llm`, so that the single root of the
//! file, [`ApolliaConfig`](super::ApolliaConfig), can name them: `apollia-llm`
//! depends on this crate and not the reverse. `apollia-llm` re-exports each of
//! them under its historical name (`LlmConfig`, `BackendConfig`,
//! `BackendKind`, `ObservabilityConfig`) and builds its router and its clients
//! from them.

use serde::{Deserialize, Serialize};

use super::LlmRoutingConfig;
use crate::{LlmBackendConfig, LlmProvider};

/// LLM configuration deserialized from the `[llm]` section of `apollia.toml`.
///
/// Read by `LlmRouter::from_config`, which no production path calls: the
/// daemon builds its router from the backends stored in `system.db`. Of this
/// struct, `routing` and `observability` (grafted after construction by the
/// supervisor) and `backends` (migrated into `system.db` on a first boot by
/// [`LlmConfig::to_db_configs`]) reach a running daemon.
///
/// `default` and `backends` have no default: a `[llm]` table without them is
/// refused, as it always was.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LlmConfig {
    /// Version of this on-disk format. A file written before versioning has
    /// no key and reads as format 1; a future format bumps the default and the
    /// reader branches on the value it finds.
    #[serde(default = "default_format_version")]
    pub format_version: u32,
    /// Default backend name (must exist in `backends`).
    pub default: String,
    /// Backends declared with `[[llm.backends]]`.
    pub backends: Vec<LlmBackendEntry>,
    /// Observability settings of the router: the prompt debug log.
    #[serde(default)]
    pub observability: LlmObservabilityConfig,
    /// LLM routing by precision level (`[llm.routing]` section). Optional.
    #[serde(default)]
    pub routing: Option<LlmRoutingConfig>,
}

impl LlmConfig {
    /// The smallest valid section: no backend, an empty default name, every
    /// optional key at its default.
    ///
    /// Not what an absent section means (an absent `[llm]` is `None` on the
    /// root); used where the full shape of the section is needed, such as the
    /// key registry's test.
    pub fn minimal() -> Self {
        Self {
            format_version: default_format_version(),
            default: String::new(),
            backends: Vec::new(),
            observability: LlmObservabilityConfig::default(),
            routing: None,
        }
    }

    /// Converts the declared backends into [`LlmBackendConfig`] rows for
    /// `system.db`.
    ///
    /// Used by the supervisor at startup to migrate from `apollia.toml` to
    /// `system.db` when no backend is found in the database.
    pub fn to_db_configs(&self) -> Vec<LlmBackendConfig> {
        self.backends
            .iter()
            .map(|b| b.to_db_config(b.name() == self.default))
            .collect()
    }
}

/// Observability settings of the LLM router (`[llm.observability]`).
///
/// Grafted onto the production router by the supervisor, after the router is
/// built from `system.db`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LlmObservabilityConfig {
    /// Log the full prompt at `TRACE` level (debug only). Off by default.
    #[serde(default)]
    pub debug_log_prompt: bool,
}

/// Current version of the persisted format, and the value read for files
/// written before the key existed.
fn default_format_version() -> u32 {
    1
}

/// One entry of `[[llm.backends]]`.
///
/// Flattens [`LlmBackendKind`], which the TOML `type` field discriminates.
/// serde forbids `deny_unknown_fields` on a struct that flattens, so each
/// variant's own structure refuses the keys it does not declare.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmBackendEntry {
    /// Backend type and parameters.
    #[serde(flatten)]
    pub kind: LlmBackendKind,
}

impl LlmBackendEntry {
    /// The backend's logical name.
    pub fn name(&self) -> &str {
        match &self.kind {
            LlmBackendKind::Api(cfg) => &cfg.name,
        }
    }

    /// The `system.db` row this entry migrates to.
    pub fn to_db_config(&self, is_default: bool) -> LlmBackendConfig {
        match &self.kind {
            LlmBackendKind::Api(api) => LlmBackendConfig {
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
}

/// Backend type discriminant in `[[llm.backends]]`.
///
/// `type = "api"`: an OpenAI-compatible HTTP backend. Local text inference
/// runs on the bundled llama-server and is never declared here.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum LlmBackendKind {
    /// OpenAI-compatible HTTP backend.
    Api(LlmApiBackendEntry),
}

/// The keys of an OpenAI-compatible backend in `[[llm.backends]]`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LlmApiBackendEntry {
    /// Version of this on-disk format; read as 1 when absent.
    #[serde(default = "default_format_version", skip_serializing)]
    pub format_version: u32,
    /// Logical backend name (the key used by the router).
    pub name: String,
    /// API base URL (for example `https://api.openai.com/v1`).
    pub api_url: String,
    /// Name of the environment variable holding the API key.
    pub api_key_env: String,
    /// Default model identifier for this backend.
    pub model: String,
    /// Usable context window in tokens, when it is known.
    #[serde(default)]
    pub context_window: Option<usize>,
    /// Whether this endpoint is a llama.cpp server accepting its `grammar`
    /// extension.
    #[serde(default)]
    pub llama_cpp_extensions: bool,
    /// Provider name, written for the reader by the backend mirror of the
    /// desktop application. Informative only: the provider is inferred from
    /// `api_url` when the entry is migrated.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
}

/// Default port Ollama listens on. Used only to recognise an Ollama backend
/// declared in a legacy TOML file, never to build a URL.
const OLLAMA_DEFAULT_PORT: &str = ":11434";

/// Infers a [`LlmProvider`] from the API base URL.
///
/// Only used when migrating a TOML backend into the database, where no
/// explicit provider is recorded. Matching on the port alone keeps an Ollama
/// server running on another machine recognisable.
pub fn infer_api_provider_from_url(api_url: &str) -> LlmProvider {
    if api_url.contains(OLLAMA_DEFAULT_PORT) || api_url.contains("ollama") {
        LlmProvider::Ollama
    } else {
        LlmProvider::OpenAi
    }
}
