//! Backends d'inférence LLM pour `apollia-llm`.
//!
//! Depuis ADR-113 (multi-runner sidecar), les backends locaux (llama.cpp,
//! whisper) sont hébergés par le crate `apollia-runner`. Ce module ne
//! contient plus que les clients HTTP cloud :
//!
//! - `openai`   : client HTTP OpenAI-compatible `[feature = "cloud"]`
//! - `anthropic`: client HTTP Anthropic `[feature = "cloud"]`
//! - `vertex`   : client Google Vertex AI `[feature = "cloud"]`

#[cfg(feature = "cloud")]
pub mod anthropic;

#[cfg(feature = "cloud")]
pub mod openai;

#[cfg(feature = "cloud")]
pub mod vertex;
