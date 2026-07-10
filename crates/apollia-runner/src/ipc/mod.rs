//! IPC types shared between the daemon (`apollia-runtime`) and the runner.
//!
//! The daemon depends on `apollia-runner` with `default-features = false` so it
//! pulls in the IPC types only, without the llama-cpp/whisper backends.

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
    ChatMessage, CompleteData, CompleteParams, EmbedData, EmbedParams, FinishReason, LoadModelData,
    LoadModelParams, Role, StreamChunk, Timing, TokenUsage, TokenizeData, TokenizeParams, ToolCall,
    UnloadModelParams,
};
pub use request::Request;
pub use response::Response;
pub use stt::{TranscribeData, TranscribeParams, TranscribeSegment};
