//! `runner_supervisor`: supervises the local STT sidecar runner.
//!
//! Spawns and supervises the `apollia-runner-{backend}` child process at daemon
//! boot, after automatic GPU detection via [`gpu_detection`]. The runner serves
//! speech-to-text (whisper) only; local LLM inference runs through the embedded
//! llama-server (see [`crate::llama_server`]).

pub mod client;
pub mod error;
pub mod gpu_detection;
pub mod lifecycle;
pub(crate) mod lifecycle_inner;
pub mod proxy;
pub mod stt_backend;

pub use error::RunnerError;
pub use lifecycle::RunnerSupervisor;
pub use proxy::RunnerProxy;
pub use stt_backend::RunnerSttBackend;

/// The runner binary the daemon starts for dictation on this machine, the one
/// the engine list asks for its version. `None` when no runner is in reach.
///
/// Blocking: detection may spawn a hardware probe.
pub(crate) fn dictation_runner_binary() -> Option<std::path::PathBuf> {
    let backend = gpu_detection::boot_backend(&gpu_detection::detect_gpu());
    lifecycle::locate_runner_binary(backend).ok()
}
