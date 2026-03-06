//! Apollia OS — Agent Interface Protocol (AIP) bridge.
//!
//! The PyO3-based bridge enabling Python agents to run inside the Rust runtime.
//! Implements duck-typing validation: any Python object with `manifest()` and
//! `async run()` is AIP-compatible (ADR-003).
//!
//! Components (Sprint 4):
//! - `loader` — loads a Python module and validates AIP duck-typing (STORY-024, STORY-025)
//! - `bridge` — async Rust→Python calls via pyo3-async-runtimes (STORY-026)
//! - `context` — `RuntimeContext` injected into agent `run()` calls (STORY-027, STORY-028)
//! - `wrapper` — `AIPWrapper` for non-native agents (LangGraph, CrewAI) (STORY-025)

pub mod bridge;
#[allow(clippy::useless_conversion)]
pub mod context;
pub mod loader;
#[allow(clippy::useless_conversion)]
pub mod memory;
pub mod validator;
