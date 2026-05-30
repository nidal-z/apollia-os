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
        ) as Arc<dyn CompletionModel>)
    }
}
