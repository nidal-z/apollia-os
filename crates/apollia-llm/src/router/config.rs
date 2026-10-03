//! The router's declarative configuration.
//!
//! The TOML shapes of the `[llm]` section live in `apollia-core`, next to the
//! single root of `apollia.toml` that names them; this module re-exports them
//! under the names the router has always used.

pub use apollia_core::config::{
    LlmApiBackendEntry, LlmBackendEntry as BackendConfig, LlmBackendKind as BackendKind, LlmConfig,
    LlmObservabilityConfig as ObservabilityConfig,
};

#[cfg(feature = "cloud")]
use crate::backends::openai::ApiBackendConfig;

/// A path or URL hint for the `LlmModelLoading` event: the API URL of a cloud
/// backend.
pub(crate) fn model_path_hint(cfg: &BackendConfig) -> String {
    match &cfg.kind {
        BackendKind::Api(api) => api.api_url.clone(),
    }
}

#[cfg(feature = "cloud")]
impl From<&LlmApiBackendEntry> for ApiBackendConfig {
    fn from(entry: &LlmApiBackendEntry) -> Self {
        Self {
            name: entry.name.clone(),
            api_url: entry.api_url.clone(),
            api_key_env: entry.api_key_env.clone(),
            model: entry.model.clone(),
            context_window: entry.context_window,
            llama_cpp_extensions: entry.llama_cpp_extensions,
        }
    }
}
