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
