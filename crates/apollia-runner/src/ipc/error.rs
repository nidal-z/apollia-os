//! Normalized error codes and serializable error body.

use serde::{Deserialize, Serialize};

/// Normalized error codes.
///
/// The list can grow with a MINOR change (a new variant) without breaking the
/// protocol. Removing a variant is a MAJOR change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    /// LLM call before `/llm/load_model`.
    ModelNotLoaded,
    /// GGUF load failed (invalid path, corrupted file, etc.).
    ModelLoadFailed,
    /// GPU memory exceeded during load or inference.
    BackendOom,
    /// Inference failed (kernel crash, runtime error).
    InferenceFailed,
    /// Malformed JSON or invalid params.
    BadRequest,
    /// Endpoint not implemented by this backend (e.g. STT on vulkan).
    UnsupportedOperation,
    /// Internal runner bug, should not happen.
    Internal,
}

/// Serializable body of an IPC error.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorBody {
    /// Normalized error code.
    pub code: ErrorCode,
    /// Human-readable message, lowercase, no trailing period.
    pub message: String,
    /// Optional contextual details (e.g. path, received value, etc.).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

impl ErrorBody {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            details: None,
        }
    }
}
