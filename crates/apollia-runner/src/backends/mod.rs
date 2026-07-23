//! Local inference backends.
//!
//! - `whisper.rs`: wraps whisper-rs for speech-to-text.
//!
//! Compiled only when one of the `local-*` features is active. Local LLM
//! inference runs through the embedded llama-server in the daemon, not here.

#[cfg(feature = "local-cpu")]
pub mod whisper;
