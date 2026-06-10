//! Deterministic replay of a captured run.
//!
//! The audit journal captures every non-deterministic input of a run (LLM
//! responses, and, from later stories, tool outputs, clock samples and random
//! draws). This module reads those captures back out and (in a later story)
//! re-injects them through a replay harness so the agentic loop can be replayed
//! deterministically and compared against the original trace.
//!
//! Nothing here performs network access or subscribes to the EventBus: it
//! operates on an immutable snapshot of journal entries.

pub mod capture;

pub use capture::{LlmCompletionSnapshot, LlmReplayCursor, ReplayCaptureError};
