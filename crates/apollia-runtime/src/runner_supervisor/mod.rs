//! `runner_supervisor` : supervision des sidecar runners LLM/STT locaux.
//!
//! Spawn et supervise le process enfant `apollia-runner-{backend}` au boot
//! du daemon, après détection automatique du GPU via [`gpu_detection`].
//!
//! Architecture détaillée : voir [ADR-113](../../../docs/adr/ADR-113-multi-runner-sidecar-architecture.md)
//! et [IPC-PROTOCOL](../../../docs/internal/architecture/IPC-PROTOCOL.md).

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
