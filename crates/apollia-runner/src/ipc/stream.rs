//! Helpers for the SSE format of the `/llm/stream` and `/stt/stream` endpoints.
//!
//! SSE wire format:
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

/// Envelope for an SSE chunk in an LLM stream.
#[derive(Debug, Clone, Serialize)]
pub struct SseChunk {
    pub request_id: Uuid,
    pub ok: bool,
    pub chunk: StreamChunk,
}

/// Final `done` event that closes the stream with usage stats.
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
