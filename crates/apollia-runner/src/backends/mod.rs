//! Backends d'inférence locale.
//!
//! Phase 1 : module placeholder. Phase 2 (STORY-004/005) ajoutera :
//! - `llama_cpp.rs` : wrap llama-cpp-2
//! - `whisper.rs` : wrap whisper-rs
//!
//! Compilé uniquement avec une feature `local-*` active.

#[cfg(feature = "local-cpu")]
pub mod llama_cpp;

#[cfg(feature = "local-cpu")]
pub mod whisper;
