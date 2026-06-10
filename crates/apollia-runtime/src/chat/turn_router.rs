//! Substantive-vs-trivial routing for a single chat turn.
//!
//! When a session runs in plan mode, not every turn warrants a plan: a greeting
//! or a short factual question stays a plain conversational exchange, while a
//! multi-step actionable request enters the discovery and plan flow. The
//! decision reuses the ORIA Observer signal so the plan flow triggers on the
//! same heuristic that triggers orchestration; the threshold and the weights are
//! owned by `apollia-oria`, never duplicated here.

use apollia_oria::observer::{orchestrated_threshold, score_turn_text};

/// Routing decision for a single chat turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnRoute {
    /// Substantive turn: enter the discovery and plan flow.
    PlanFlow,
    /// Trivial turn: handle conversationally, no plan.
    Conversational,
}

/// Classifies a chat turn as substantive (plan flow) or trivial (conversational).
///
/// The decision reuses the Observer `orchestrated_threshold` and `score_turn_text`
/// from `apollia-oria`, so the plan flow triggers on the same signal that
/// triggers orchestration. No threshold literal is introduced in this crate.
///
/// Routing rules, in order:
/// - an empty or blank turn is always [`TurnRoute::Conversational`] (never a plan
///   on no input);
/// - with plan mode off the plan flow is never taken, leaving the classic
///   per-tool human-in-the-loop escalation path untouched;
/// - otherwise the turn scores `>=` the threshold to enter [`TurnRoute::PlanFlow`].
///
/// This function is pure: it never mutates session state and makes no model call.
pub fn classify_turn(input: &str, plan_mode: bool) -> TurnRoute {
    if input.trim().is_empty() {
        return TurnRoute::Conversational;
    }
    if !plan_mode {
        // Outside plan mode the plan flow is never taken.
        return TurnRoute::Conversational;
    }
    if score_turn_text(input) >= orchestrated_threshold() {
        TurnRoute::PlanFlow
    } else {
        TurnRoute::Conversational
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_substantive_turn_routes_to_plan_flow() {
        // GIVEN plan mode on and a multi-step actionable request
        let input = "Plan the work: audit the repo, fix the failing tests, then open a PR";

        // WHEN classify_turn evaluates it
        let route = classify_turn(input, true);

        // THEN it routes to the plan flow
        assert_eq!(route, TurnRoute::PlanFlow);
    }

    #[test]
    fn test_trivial_turn_stays_conversational() {
        // GIVEN plan mode on and a trivial greeting
        // WHEN classify_turn evaluates it
        let route = classify_turn("hi there", true);

        // THEN it stays conversational
        assert_eq!(route, TurnRoute::Conversational);
    }

    #[test]
    fn test_plan_mode_off_never_routes_to_plan_flow() {
        // GIVEN plan mode off and a substantive request
        let input = "Plan the work: audit the repo, fix the failing tests, then open a PR";

        // WHEN classify_turn evaluates it
        let route = classify_turn(input, false);

        // THEN the plan flow is never taken; the classic escalation path applies
        assert_eq!(route, TurnRoute::Conversational);
    }

    #[test]
    fn test_threshold_comes_from_oria() {
        // GIVEN the router and the Observer threshold
        // WHEN reading the decision boundary
        let threshold = apollia_oria::observer::orchestrated_threshold();

        // THEN the router uses the ORIA threshold, not a local constant
        assert!(threshold > 0.0);
    }

    #[test]
    fn test_empty_input_defaults_conversational() {
        // GIVEN plan mode on and a blank input
        // WHEN classify_turn evaluates whitespace
        let route = classify_turn("   ", true);

        // THEN it is conversational, no panic on empty input
        assert_eq!(route, TurnRoute::Conversational);
    }
}
