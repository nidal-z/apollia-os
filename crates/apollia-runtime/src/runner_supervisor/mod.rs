//! `runner_supervisor`: supervises the local LLM/STT sidecar runners.
//!
//! Spawns and supervises the `apollia-runner-{backend}` child process at daemon
//! boot, after automatic GPU detection via [`gpu_detection`].

pub mod client;
pub mod error;
pub mod gpu_detection;
pub mod lifecycle;
pub(crate) mod lifecycle_inner;
pub mod llm_backend;
pub mod proxy;
pub mod stt_backend;

pub use error::RunnerError;
pub use lifecycle::RunnerSupervisor;
pub use llm_backend::RunnerLlmBackend;
pub use proxy::RunnerProxy;
pub use stt_backend::RunnerSttBackend;

use std::sync::Arc;

use apollia_core::LlmBackendConfig;
use apollia_llm::types::CompletionModel;

/// Default number of inference slots when the backend config does not specify
/// one. `1` on purpose: measured throughput does not improve with more slots on a
/// single GPU (llama.cpp does not batch across independent contexts, so they
/// serialize), while each extra slot allocates a FULL KV cache. On a large trained
/// window (e.g. Ministral's 262144) a single context is already tens of GB, so
/// several slots can exceed the Metal/CUDA working-set and hard-abort the runner
/// at load (a Metal alloc abort is not a catchable error, so slot-pool degradation
/// cannot save it). Operators who want concurrent-request latency (no head-of-line
/// blocking) raise it via `config_json.slot_count`, ideally with a bounded context.
const DEFAULT_SLOT_COUNT: u32 = 1;

/// Read the desired slot count from a backend's `config_json`, falling back to
/// [`DEFAULT_SLOT_COUNT`]. A `0` or missing value resolves to the default.
fn resolve_slot_count(cfg: &LlmBackendConfig) -> u32 {
    cfg.config_json
        .get("slot_count")
        .and_then(serde_json::Value::as_u64)
        .filter(|&n| n > 0)
        .map(|n| n as u32)
        .unwrap_or(DEFAULT_SLOT_COUNT)
}

/// Read the KV cache data type from a backend's `config_json` (`kv_cache_type`,
/// e.g. `"q8_0"`), or `None` for the default `f16`. Opt-in: quantizing the KV
/// cache shrinks its footprint at a small precision cost.
fn resolve_kv_cache_type(cfg: &LlmBackendConfig) -> Option<String> {
    cfg.config_json
        .get("kv_cache_type")
        .and_then(serde_json::Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// `LlamaCpp -> runner` override factory shared by the supervisor boot and the
/// `LlmRouter` reload paths.
///
/// Each `LlamaCpp` provider backend is routed to a [`RunnerLlmBackend`] wired to
/// the provided runner `proxy`. Other (cloud) providers return `None` to keep
/// their standard instantiation; all backends return `None` if no runner proxy
/// is available.
///
/// Centralizing this logic prevents reloads from rebuilding a router without the
/// local backend (which would make the runner unreachable from agents/chat).
pub fn runner_llm_override(
    proxy: Option<RunnerProxy>,
) -> impl Fn(&LlmBackendConfig) -> Option<Arc<dyn CompletionModel>> {
    move |cfg: &LlmBackendConfig| {
        use apollia_core::LlmProvider;
        if !matches!(cfg.provider, LlmProvider::LlamaCpp) {
            return None;
        }
        let proxy = proxy.clone()?;
        // For llama-cpp, `LlmBackendConfig::model` is the absolute path to the .gguf.
        Some(RunnerLlmBackend::new(
            proxy,
            cfg.name.clone(),
            cfg.name.clone(),
            cfg.model.clone(),
            resolve_slot_count(cfg),
            resolve_kv_cache_type(cfg),
        ) as Arc<dyn CompletionModel>)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use apollia_core::LlmProvider;

    fn cfg(config_json: serde_json::Value) -> LlmBackendConfig {
        LlmBackendConfig {
            name: "local".to_string(),
            provider: LlmProvider::LlamaCpp,
            model: "/models/m.gguf".to_string(),
            config_json,
            enabled: true,
            is_default: true,
        }
    }

    #[test]
    fn resolve_slot_count_uses_config_value() {
        // GIVEN a backend config that pins slot_count
        let c = cfg(serde_json::json!({ "slot_count": 6 }));
        // WHEN resolving the slot count
        // THEN the configured value wins over the default
        assert_eq!(resolve_slot_count(&c), 6);
    }

    #[test]
    fn resolve_slot_count_defaults_when_absent() {
        // GIVEN a config with no slot_count key
        let c = cfg(serde_json::json!({}));
        // THEN the modest default applies
        assert_eq!(resolve_slot_count(&c), DEFAULT_SLOT_COUNT);
    }

    #[test]
    fn resolve_slot_count_defaults_on_zero() {
        // GIVEN an explicit zero (a slot pool needs at least one slot)
        let c = cfg(serde_json::json!({ "slot_count": 0 }));
        // THEN it falls back to the default rather than requesting zero slots
        assert_eq!(resolve_slot_count(&c), DEFAULT_SLOT_COUNT);
    }

    #[test]
    fn resolve_kv_cache_type_reads_config_or_none() {
        // GIVEN a config pinning the KV cache type
        assert_eq!(
            resolve_kv_cache_type(&cfg(serde_json::json!({ "kv_cache_type": "q8_0" }))),
            Some("q8_0".to_string())
        );
        // GIVEN no key -> None (runner keeps the f16 default)
        assert_eq!(resolve_kv_cache_type(&cfg(serde_json::json!({}))), None);
        // GIVEN an empty string -> None
        assert_eq!(
            resolve_kv_cache_type(&cfg(serde_json::json!({ "kv_cache_type": "" }))),
            None
        );
    }
}
