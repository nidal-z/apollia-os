//! Structured HITL request payload (US-SP42-045 — Pattern P9).
//!
//! Shared in `apollia-core` so that the runtime, the desktop app, and any
//! frontend payload can refer to the same [`HitlRequest`] contract without
//! creating a dependency on `apollia-runtime`.
//!
//! # Design
//!
//! Two layers:
//! - **Always-on** : a static analyzer (`apollia-runtime/src/analyzers/risk_analyzer.rs`)
//!   maps an `action_type` to a [`RiskAnalysis`] (categories, severity 0..10,
//!   mitigations). Coût : 0.
//! - **Opt-in** : `MetaRoutine::GenerateAskUserConsequences` may be called to
//!   enrich `consequences_if_approved` / `consequences_if_rejected` with a
//!   short plain-text narration. Coût : ~400 in / 200 out tokens per HITL.
//!
//! Reject workflow : when the operator rejects, the textarea reason is
//! required (non-empty). The runtime emits
//! [`crate::RuntimeEvent::HitlRejected`] and propagates the reason to the
//! agent via `approval_outcome` on the SDK side.

use serde::{Deserialize, Serialize};

// ─────────────────────────────────────────────
// ImpactLevel
// ─────────────────────────────────────────────

/// High-level impact classification surfaced on HITL cards.
///
/// The frontend uses this to pick a badge color: `Low` → success,
/// `Medium` → warning, `High` → orange, `Critical` → destructive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImpactLevel {
    /// Read-only / easily reversible actions (read files, list resources).
    Low,
    /// Local side-effects with limited blast radius (write to workspace).
    Medium,
    /// Non-trivial side-effects (network writes, bulk memory writes).
    High,
    /// Hard-to-reverse actions (delete, external A2A, destructive shell).
    Critical,
}

impl ImpactLevel {
    /// Derive an `ImpactLevel` from a 0..10 severity score (static analyzer).
    pub fn from_severity(score: u8) -> Self {
        match score {
            0..=3 => Self::Low,
            4..=6 => Self::Medium,
            7..=8 => Self::High,
            _ => Self::Critical,
        }
    }

    /// Stable i18n key root used by the frontend (`hitl.impact.<snake>`).
    pub fn i18n_key(self) -> &'static str {
        match self {
            Self::Low => "hitl.impact.low",
            Self::Medium => "hitl.impact.medium",
            Self::High => "hitl.impact.high",
            Self::Critical => "hitl.impact.critical",
        }
    }
}

// ─────────────────────────────────────────────
// RiskAnalysis
// ─────────────────────────────────────────────

/// Structured risk breakdown attached to a [`HitlRequest`].
///
/// Produced by the always-on `risk_analyzer` from the action type and
/// arguments. Never empty — if no category applies, the analyzer falls
/// back to a generic `["unknown"]` category with severity 5.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RiskAnalysis {
    /// Human-readable risk categories (e.g. `"destructive"`, `"network"`,
    /// `"memory_write"`, `"external_a2a"`). Free-form slugs used by the UI
    /// to render chips.
    pub categories: Vec<String>,
    /// Severity on a 0..10 scale (0 = trivial, 10 = catastrophic).
    pub severity: u8,
    /// Short mitigation tips rendered under the risk block.
    pub mitigations: Vec<String>,
}

impl RiskAnalysis {
    /// Construct a [`RiskAnalysis`] clamping `severity` to the 0..=10 range.
    pub fn new(
        categories: impl IntoIterator<Item = impl Into<String>>,
        severity: u8,
        mitigations: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        Self {
            categories: categories.into_iter().map(Into::into).collect(),
            severity: severity.min(10),
            mitigations: mitigations.into_iter().map(Into::into).collect(),
        }
    }

    /// Generic fallback used when the analyzer cannot match any needle.
    pub fn unknown() -> Self {
        Self::new(
            ["unknown"],
            5,
            ["Review the action carefully before approving."],
        )
    }
}

// ─────────────────────────────────────────────
// HitlRequest
// ─────────────────────────────────────────────

/// Enriched payload for a human-in-the-loop request.
///
/// Built by the runtime before a HITL suspension is surfaced to the UI.
/// The `context_thinking` and `consequences_*` fields are optional — the
/// former is populated from the reasoner trace when available, the latter
/// only when the user opts into `routines.ask_user_consequences`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HitlRequest {
    /// Unique request identifier (corresponds to `task_id` or `request_id`).
    pub id: String,
    /// The question or action summary the user must approve.
    pub question: String,
    /// The agent's reasoning trace excerpt — "Why is the agent asking?".
    #[serde(default)]
    pub context_thinking: Option<String>,
    /// Coarse-grained impact classification (always-on).
    pub estimated_impact: ImpactLevel,
    /// Structured risk breakdown (always-on).
    pub risk_analysis: RiskAnalysis,
    /// Plain-text narration of what happens if the user approves.
    /// Opt-in via `routines.ask_user_consequences`; `None` → UI renders
    /// a static fallback template.
    #[serde(default)]
    pub consequences_if_approved: Option<String>,
    /// Plain-text narration of what happens if the user rejects.
    /// Opt-in via `routines.ask_user_consequences`; `None` → UI renders
    /// a static fallback template.
    #[serde(default)]
    pub consequences_if_rejected: Option<String>,
}

impl HitlRequest {
    /// Construct a [`HitlRequest`] deriving `estimated_impact` from the
    /// analyser's severity score.
    pub fn new(
        id: impl Into<String>,
        question: impl Into<String>,
        risk_analysis: RiskAnalysis,
    ) -> Self {
        let estimated_impact = ImpactLevel::from_severity(risk_analysis.severity);
        Self {
            id: id.into(),
            question: question.into(),
            context_thinking: None,
            estimated_impact,
            risk_analysis,
            consequences_if_approved: None,
            consequences_if_rejected: None,
        }
    }

    /// Attach a thinking excerpt.
    pub fn with_thinking(mut self, thinking: impl Into<String>) -> Self {
        self.context_thinking = Some(thinking.into());
        self
    }

    /// Attach opt-in consequences text (approved / rejected).
    pub fn with_consequences(
        mut self,
        if_approved: Option<String>,
        if_rejected: Option<String>,
    ) -> Self {
        self.consequences_if_approved = if_approved;
        self.consequences_if_rejected = if_rejected;
        self
    }
}

// ─────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// GIVEN severity scores across the 0..=10 range
    /// WHEN ImpactLevel::from_severity()
    /// THEN the mapping respects the documented thresholds.
    #[test]
    fn impact_level_from_severity_thresholds() {
        assert_eq!(ImpactLevel::from_severity(0), ImpactLevel::Low);
        assert_eq!(ImpactLevel::from_severity(3), ImpactLevel::Low);
        assert_eq!(ImpactLevel::from_severity(4), ImpactLevel::Medium);
        assert_eq!(ImpactLevel::from_severity(6), ImpactLevel::Medium);
        assert_eq!(ImpactLevel::from_severity(7), ImpactLevel::High);
        assert_eq!(ImpactLevel::from_severity(8), ImpactLevel::High);
        assert_eq!(ImpactLevel::from_severity(9), ImpactLevel::Critical);
        assert_eq!(ImpactLevel::from_severity(10), ImpactLevel::Critical);
        // Out-of-range — still maps to Critical (saturates).
        assert_eq!(ImpactLevel::from_severity(42), ImpactLevel::Critical);
    }

    /// GIVEN a RiskAnalysis built with severity > 10
    /// WHEN RiskAnalysis::new()
    /// THEN severity is clamped to 10.
    #[test]
    fn risk_analysis_clamps_severity() {
        let ra = RiskAnalysis::new(["destructive"], 42, ["review"]);
        assert_eq!(ra.severity, 10);
    }

    /// GIVEN a HitlRequest built from a risk analysis with severity 9
    /// WHEN new()
    /// THEN estimated_impact is Critical and JSON round-trips.
    #[test]
    fn hitl_request_derives_impact_and_round_trips() {
        let ra = RiskAnalysis::new(["destructive", "network"], 9, ["double-check target"]);
        let req = HitlRequest::new("t-1", "Delete prod DB?", ra)
            .with_thinking("User asked to clean up …")
            .with_consequences(Some("DB dropped.".into()), Some("Agent blocked.".into()));
        assert_eq!(req.estimated_impact, ImpactLevel::Critical);

        let json = serde_json::to_string(&req).expect("serialize");
        let parsed: HitlRequest = serde_json::from_str(&json).expect("round-trip");
        assert_eq!(parsed, req);
    }

    /// GIVEN the unknown fallback
    /// WHEN RiskAnalysis::unknown()
    /// THEN severity is 5, categories non-empty, mitigations non-empty.
    #[test]
    fn risk_analysis_unknown_fallback() {
        let ra = RiskAnalysis::unknown();
        assert_eq!(ra.severity, 5);
        assert!(!ra.categories.is_empty());
        assert!(!ra.mitigations.is_empty());
    }
}
