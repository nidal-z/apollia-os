//! Deterministic replay of a captured run.
//!
//! The audit journal captures every non-deterministic input of a run: LLM
//! responses, tool outputs, clock samples and random draws. This module reads
//! those captures back out (see [`capture`]) and exposes the instrumented
//! sources of non-determinism ([`nondeterminism`]) that the replay harness (a
//! later story) substitutes to reproduce a run deterministically and compare it
//! against the original trace.
//!
//! Nothing here performs network access or subscribes to the EventBus: it
//! operates on an immutable snapshot of journal entries.

pub mod capture;
pub mod nondeterminism;

pub use capture::{
    ClockReplayCursor, ClockSample, LlmCompletionSnapshot, LlmReplayCursor, RandomReplayCursor,
    RandomSample, ReplayCaptureError, ReplayCursor, ToolOutputSnapshot, ToolReplayCursor,
};
pub use nondeterminism::{ClockSource, RandomSource, RealClock, RealRandom};
