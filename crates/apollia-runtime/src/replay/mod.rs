//! Deterministic replay of a captured run.
//!
//! The audit journal captures every non-deterministic input of a run: LLM
//! responses, tool outputs, clock samples and random draws. This module reads
//! those captures back out ([`capture`]), exposes the instrumented sources of
//! non-determinism ([`nondeterminism`]), wraps them as replay injectors
//! ([`injectors`]), and drives a deterministic replay that compares a run
//! against its trace ([`harness`]).
//!
//! Nothing here performs network access or subscribes to the EventBus: it
//! operates on an immutable snapshot of journal entries.

pub mod capture;
pub mod harness;
pub mod injectors;
pub mod nondeterminism;

pub use capture::{
    ClockReplayCursor, ClockSample, LlmCompletionSnapshot, LlmReplayCursor,
    PlanMutationReplayCursor, RandomReplayCursor, RandomSample, ReplayBundle, ReplayCaptureError,
    ReplayCursor, ToolOutputSnapshot, ToolReplayCursor,
};
pub use harness::{ReplayDivergence, ReplayError, ReplayFailReason, ReplayHarness, ReplayReport};
pub use injectors::{ReplayBackend, ReplayClock, ReplayRandom, ReplayToolInvoker};
pub use nondeterminism::{ClockSource, RandomSource, RealClock, RealRandom};
