#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
//! Apollia OS sidecar runner for local speech-to-text (STT) inference.
//!
//! This crate produces a binary `apollia-runner` (renamed to
//! `apollia-runner-{backend}` during packaging) that:
//!
//! 1. Binds an axum server on `127.0.0.1:0` (port picked by the OS).
//! 2. Announces the port to its parent via stdout (`READY <port>\n`).
//!
//! `unsafe_code` is allowed by this crate's manifest for two production
//! sites, the `Send`/`Sync` impls wrapping the whisper-rs FFI context in
//! `backends/whisper.rs`; both carry their `// SAFETY:` comment.
//! 3. Serves the `/handshake`, `/health`, `/stt/transcribe`, `/shutdown` endpoints.
//! 4. Loads `whisper-rs` with a single GPU backend compiled in.
//!
//! Local LLM inference runs through the embedded `llama-server` managed by the
//! daemon (see `apollia-runtime`'s `llama_server` module), not here.

pub mod ipc;
pub mod lifecycle;
pub mod observability;
pub mod server;

#[cfg(feature = "local-cpu")]
pub mod backends;
