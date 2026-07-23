//! IPC types for the runner's HTTP API.
//!
//! The runner exposes the STT (`/stt/*`) and control (`/handshake`, `/health`,
//! `/shutdown`) endpoints. Local LLM inference runs through the embedded
//! llama-server in the daemon, so no LLM IPC types live here.

pub mod error;
pub mod handshake;
pub mod request;
pub mod response;
pub mod stt;

pub use error::{ErrorBody, ErrorCode};
pub use handshake::{Backend, GpuInfoDto, HandshakeData, HealthData};
pub use request::Request;
pub use response::Response;
pub use stt::{TranscribeData, TranscribeParams, TranscribeSegment};
