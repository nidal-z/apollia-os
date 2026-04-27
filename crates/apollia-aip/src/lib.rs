//! Apollia OS — Agent Interface Protocol (AIP) bridge.
//!
//! The PyO3-based bridge enabling Python agents to run inside the Rust runtime.
//! Implements duck-typing validation: any Python object with `manifest()` and
//! `async run()` is AIP-compatible (ADR-003).
//!
//! Components (Sprint 4):
//! - `loader` — loads a Python module and validates AIP duck-typing.
//! - `bridge` — async Rust→Python calls via pyo3-async-runtimes.
//! - `context` — `RuntimeContext` injected into agent `run()` calls.
//! - `wrapper` — `AIPWrapper` for non-native agents (LangGraph, CrewAI).

pub mod bridge;
#[allow(clippy::useless_conversion)]
pub mod context;
#[allow(clippy::useless_conversion)]
pub mod llm;
pub mod loader;
#[allow(clippy::useless_conversion)]
pub mod memory;
pub mod notify;
pub mod package_loader;
pub mod python_provider;
pub mod stt;
pub mod validator;
