//! LLM inference backends for `apollia-llm`.
//!
//! Local backends (llama.cpp, whisper) are hosted by the `apollia-runner`
//! crate (multi-runner sidecar). This module now holds only the cloud HTTP
//! clients:
//!
//! - `openai`   : OpenAI-compatible HTTP client `[feature = "cloud"]`
//! - `anthropic`: Anthropic HTTP client `[feature = "cloud"]`
//! - `vertex`   : Google Vertex AI client `[feature = "cloud"]`

#[cfg(feature = "cloud")]
pub mod anthropic;

#[cfg(feature = "cloud")]
pub mod openai;

#[cfg(feature = "cloud")]
pub mod vertex;
