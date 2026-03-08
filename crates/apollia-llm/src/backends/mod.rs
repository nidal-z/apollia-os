//! Backends d'inférence LLM pour `apollia-llm`.
//!
//! Chaque backend est protégé par sa feature flag :
//! - `embedded` : moteur in-process `[feature = "local"]`
//! - `openai`   : client HTTP OpenAI-compatible `[feature = "cloud"]`
//! - `anthropic`: client HTTP Anthropic `[feature = "cloud"]`

#[cfg(feature = "local")]
pub mod embedded;

#[cfg(feature = "cloud")]
pub mod anthropic;

#[cfg(feature = "cloud")]
pub mod openai;
