//! Helpers pour le format SSE des endpoints `/llm/stream` et `/stt/stream`.
//!
//! Format conforme à [IPC-PROTOCOL §2.5](../../../../docs/internal/architecture/IPC-PROTOCOL.md) :
//!
//! ```text
//! data: {"request_id":"...","ok":true,"chunk":{"text":"Le ","finish_reason":null}}
//!
//! data: {"request_id":"...","ok":true,"chunk":{"text":"chat ","finish_reason":null}}
//!
//! event: done
//! data: {"request_id":"...","ok":true,"usage":{...}}
//! ```

use serde::Serialize;
use uuid::Uuid;

use super::llm::{StreamChunk, TokenUsage};

/// Enveloppe d'un chunk SSE de stream LLM.
#[derive(Debug, Clone, Serialize)]
pub struct SseChunk {
    pub request_id: Uuid,
    pub ok: bool,
    pub chunk: StreamChunk,
}

/// Event final `done` qui clôture le stream avec les stats d'usage.
#[derive(Debug, Clone, Serialize)]
pub struct SseDone {
    pub request_id: Uuid,
    pub ok: bool,
    pub usage: TokenUsage,
}

impl SseChunk {
    pub fn new(request_id: Uuid, chunk: StreamChunk) -> Self {
        Self {
            request_id,
            ok: true,
            chunk,
        }
    }
}

impl SseDone {
    pub fn new(request_id: Uuid, usage: TokenUsage) -> Self {
        Self {
            request_id,
            ok: true,
            usage,
        }
    }
}
