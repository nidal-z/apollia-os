//! LLM routing level: separates deep reasoning from light extraction.
//!
//! [`LlmRoutingLevel`] encodes the cost/latency/quality tradeoff into two
//! categories directly derivable from the scaling laws (Kaplan et al., 2020).
//! Used by [`crate::router::LlmRouter::route_precise`] and
//! [`crate::router::LlmRouter::route_fast`] to select the right backend.

/// LLM routing level by task type.
///
/// Derived from the cost/latency/quality tradeoff documented in the scaling
/// laws (Kaplan et al., 2020, "Scaling Laws for Neural Language Models").
/// The two levels match the two natural axes of LLM use: deep reasoning vs
/// deterministic extraction.
///
/// Used to select the backend via
/// [`LlmRouter::route_precise`](crate::router::LlmRouter::route_precise) and
/// [`LlmRouter::route_fast`](crate::router::LlmRouter::route_fast).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LlmRoutingLevel {
    /// Reasoning tasks: planning, complex analysis, judgment.
    ///
    /// Favors quality at the expense of cost and latency.
    /// Configurable via `[llm.routing] precise` in `apollia.toml`.
    Precise,

    /// Extraction tasks: metadata, short summaries, classification, parsing.
    ///
    /// Favors speed and cost at the expense of nuance.
    /// Configurable via `[llm.routing] fast` in `apollia.toml`.
    Fast,
}

/// Signal emitted by the agent loop to request frontier escalation.
///
/// A typed enum rather than a boolean, so future waves can add richer signals
/// (confidence score, task category) without breaking the call sites.
/// [`EscalationSignal::None`] means "no escalation requested": the router stays
/// local. Consumed by
/// [`LlmRouter::route_with_escalation`](crate::router::LlmRouter::route_with_escalation).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EscalationSignal {
    /// No escalation requested. The router uses the local backend.
    None,

    /// A step failed repeatedly. `consecutive_failures` is the count since the
    /// last successful step, as recorded by the agent loop.
    RepeatedStepFailure {
        /// Number of consecutive failures since the last success.
        consecutive_failures: u32,
    },

    /// The autonomy tier explicitly requests frontier quality for this step.
    AutonomyTierRequest,
}

impl EscalationSignal {
    /// Return `true` if this signal requests escalation.
    pub fn is_escalation(&self) -> bool {
        !matches!(self, Self::None)
    }
}
