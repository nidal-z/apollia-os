//! Structured retry and fallback tracking.
//!
//! Captures each attempt of a tool invocation so the UI can render a timeline
//! (retry badge + horizontal lane) instead of hiding retries behind a single
//! final status. Also carries the LLM fallback shape used when the router
//! switches provider after a primary failure.
//!
//! Always-on, no opt-in: retry attempts are emitted as side events and
//! accumulated into [`crate::events::RuntimeEvent::ChatToolCallCompleted`]
//! via `ToolCallRecord.retry_attempts`. Cost: 0.

use serde::{Deserialize, Serialize};

use crate::error_analysis::ErrorAnalysis;

/// Outcome of a single attempt within a retry chain.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AttemptOutcome {
    /// The attempt succeeded.
    Success,
    /// The attempt failed with a classified error.
    Failed,
    /// The attempt exceeded its per-call timeout.
    TimedOut,
    /// The attempt was replaced by a fallback target (e.g. secondary LLM).
    Fallback {
        /// Identifier of the replacement target (backend name, tool alias).
        to: String,
    },
}

/// A single attempt in a retry / fallback chain for a tool call.
///
/// Timestamps are ms since the Unix epoch. `reason` is populated when the
/// attempt failed and the error classifier produced an [`ErrorAnalysis`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetryAttempt {
    /// 1-based attempt index inside the retry chain.
    pub attempt_number: u32,
    /// Start of the attempt, ms since Unix epoch.
    pub started_at: u64,
    /// End of the attempt, ms since Unix epoch.
    pub ended_at: u64,
    /// Outcome category — success / failure / timeout / fallback.
    pub outcome: AttemptOutcome,
    /// Classified error for non-success outcomes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<ErrorAnalysis>,
}

impl RetryAttempt {
    /// Convenience constructor.
    pub fn new(
        attempt_number: u32,
        started_at: u64,
        ended_at: u64,
        outcome: AttemptOutcome,
    ) -> Self {
        Self {
            attempt_number,
            started_at,
            ended_at,
            outcome,
            reason: None,
        }
    }

    /// Builder: attach the classified error driving this attempt's failure.
    pub fn with_reason(mut self, analysis: ErrorAnalysis) -> Self {
        self.reason = Some(analysis);
        self
    }
}

/// Snapshot of an LLM provider fallback event.
///
/// Emitted by [`crate::events::RuntimeEvent::LlmFallbackTriggered`] when the
/// router silently switches from a primary backend to a secondary one after
/// a non-recoverable failure. Surfaced by the UI as a discreet banner.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LlmFallback {
    /// Name of the backend that failed.
    pub from_provider: String,
    /// Name of the backend that handled the retry.
    pub to_provider: String,
    /// Human-readable reason (error category label or short message).
    pub reason: String,
}
