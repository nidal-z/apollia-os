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
