#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
//! Apollia OS sidecar runner for local LLM and STT inference.
//!
//! This crate produces a binary `apollia-runner` (renamed to
//! `apollia-runner-{backend}` during packaging) that:
//!
//! 1. Binds an axum server on `127.0.0.1:0` (port picked by the OS).
//! 2. Announces the port to its parent via stdout (`READY <port>\n`).
//! 3. Serves the `/handshake`, `/health`, `/llm/*`, `/stt/*`, `/shutdown` endpoints.
//! 4. Loads `llama-cpp-2` and `whisper-rs` with a single GPU backend compiled in.

pub mod ipc;
pub mod lifecycle;
pub mod observability;
pub mod server;

#[cfg(feature = "local-cpu")]
pub mod backends;
