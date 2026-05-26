//! Types IPC partagés entre daemon (`apollia-runtime`) et runner.
//!
//! Le daemon dépend de `apollia-runner` avec `default-features = false` pour
//! ne consommer que les types IPC sans tirer les backends llama-cpp/whisper.

pub mod error;
pub mod handshake;
pub mod llm;
pub mod request;
pub mod response;
pub mod stream;
pub mod stt;

pub use error::{ErrorBody, ErrorCode};
pub use handshake::{Backend, GpuInfoDto, HandshakeData, HealthData};
pub use llm::{
    ChatMessage, CompleteData, CompleteParams, EmbedData, EmbedParams, FinishReason,
    LoadModelData, LoadModelParams, Role, StreamChunk, Timing, TokenUsage, UnloadModelParams,
};
pub use request::Request;
pub use response::Response;
pub use stt::{TranscribeData, TranscribeParams, TranscribeSegment};
