//! Local inference backends.
//!
//! - `llama_cpp.rs`: wraps llama-cpp-2 for LLM inference.
//! - `whisper.rs`: wraps whisper-rs for speech-to-text.
//!
//! Compiled only when one of the `local-*` features is active.

#[cfg(feature = "local-cpu")]
pub mod llama_cpp;

#[cfg(feature = "local-cpu")]
pub mod whisper;
