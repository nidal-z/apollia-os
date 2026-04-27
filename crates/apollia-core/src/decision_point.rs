//! Structured decision branches.
//!
//! Captures, for a *significant* agent decision (tool choice, agent delegate,
//! memory write/no-write), the path that was taken *and* the alternatives
//! considered so the UI can render a `<DecisionBranchesPanel />` that makes the
//! trade-off explicit.
//!
//! Populated by [`crate::events::RuntimeEvent::DecisionPointRecorded`] after
//! the thinking phase, via the meta-LLM routine `GenerateAlternativeBranches`.
//! Opt-in (`routines.decision_branches`, default off) — no cost when disabled.

use serde::{Deserialize, Serialize};

/// Coarse kind of decision — used to filter which selections warrant the
/// extra meta-LLM call (`Significant` only).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecisionKind {
    /// A tool was selected among several candidates.
    ToolChoice,
    /// A sub-agent was delegated the next step.
    AgentDelegate,
    /// The agent chose to write (or not write) something to memory.
    MemoryWrite,
    /// Catch-all for other significant selections.
    Significant,
}

/// One alternative path the agent considered but rejected.
///
/// `confidence_delta` is expressed relative to the chosen path — it is
/// therefore expected to be ≤ 0 (UI renders a negative-going bar).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConsideredAlternative {
    /// Short human-readable label (e.g. `"read_file(foo.rs)"`).
    pub label: String,
    /// Explanation of why this alternative was rejected (≤ 1 sentence).
    pub rejected_reason: String,
    /// Signed gap between this alternative's confidence and the chosen
    /// path's confidence. Negative by construction (the chosen path is,
    /// by definition, the most confident one).
    pub confidence_delta: f32,
}

/// A decision point captured from the reasoner thinking trace.
///
/// Emitted inside [`crate::events::RuntimeEvent::DecisionPointRecorded`].
/// At most 3 alternatives are carried (LLM prompt caps this).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DecisionPoint {
    /// Identifier of the turn this decision belongs to. Correlates with
    /// `ThinkingStarted` / `ThinkingEnded` events and with the chat turn UI.
    pub turn_id: String,
    /// Kind of selection this point represents.
    pub kind: DecisionKind,
    /// Short label of the option the agent actually picked.
    pub chosen: String,
    /// Up to 3 alternatives considered and rejected.
    #[serde(default)]
    pub alternatives: Vec<ConsideredAlternative>,
}

impl DecisionPoint {
    /// Parses the JSON payload produced by the
    /// `GenerateAlternativeBranches` meta routine.
    ///
    /// Tolerates Markdown code fences (` ```json … ``` `) the way sister
    /// routines do. Caps `alternatives` at 3 entries — anything above is
    /// truncated silently (the prompt already enforces the limit).
    pub fn parse(
        raw: &str,
        turn_id: impl Into<String>,
        kind: DecisionKind,
    ) -> Result<Self, serde_json::Error> {
        let clean = raw
            .trim()
            .trim_start_matches("```json")
            .trim_start_matches("```")
            .trim_end_matches("```")
            .trim();

        #[derive(Deserialize)]
        struct Raw {
            chosen: String,
            #[serde(default)]
            alternatives: Vec<ConsideredAlternative>,
        }
        let parsed: Raw = serde_json::from_str(clean)?;
        let mut alternatives = parsed.alternatives;
        alternatives.truncate(3);
        Ok(Self {
            turn_id: turn_id.into(),
            kind,
            chosen: parsed.chosen,
            alternatives,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // GIVEN a well-formed JSON response with 2 alternatives
    // WHEN DecisionPoint::parse is called
    // THEN a DecisionPoint is returned with chosen + 2 alternatives
    #[test]
    fn parse_accepts_full_payload() {
        let raw = r#"{
            "chosen": "read_file",
            "alternatives": [
                {"label":"bash_executor","rejected_reason":"overkill for a single file","confidence_delta":-0.3},
                {"label":"grep","rejected_reason":"path already known","confidence_delta":-0.5}
            ]
        }"#;
        let dp = DecisionPoint::parse(raw, "turn-1", DecisionKind::ToolChoice).expect("parse ok");
        assert_eq!(dp.chosen, "read_file");
        assert_eq!(dp.alternatives.len(), 2);
        assert_eq!(dp.kind, DecisionKind::ToolChoice);
        assert!(dp.alternatives[0].confidence_delta < 0.0);
    }

    // GIVEN Markdown fences around the JSON
    // WHEN DecisionPoint::parse is called
    // THEN the fences are stripped and parsing succeeds
    #[test]
    fn parse_strips_markdown_fences() {
        let raw = "```json\n{\"chosen\":\"x\",\"alternatives\":[]}\n```";
        let dp = DecisionPoint::parse(raw, "t", DecisionKind::Significant).expect("parse ok");
        assert_eq!(dp.chosen, "x");
        assert!(dp.alternatives.is_empty());
    }

    // GIVEN more than 3 alternatives in the payload
    // WHEN DecisionPoint::parse is called
    // THEN only the first 3 are kept
    #[test]
    fn parse_caps_alternatives_at_three() {
        let raw = r#"{
            "chosen":"a",
            "alternatives":[
                {"label":"b","rejected_reason":"r","confidence_delta":-0.1},
                {"label":"c","rejected_reason":"r","confidence_delta":-0.2},
                {"label":"d","rejected_reason":"r","confidence_delta":-0.3},
                {"label":"e","rejected_reason":"r","confidence_delta":-0.4}
            ]
        }"#;
        let dp = DecisionPoint::parse(raw, "t", DecisionKind::ToolChoice).expect("parse ok");
        assert_eq!(dp.alternatives.len(), 3);
        assert_eq!(dp.alternatives[2].label, "d");
    }

    // GIVEN an invalid JSON payload
    // WHEN DecisionPoint::parse is called
    // THEN an error is returned
    #[test]
    fn parse_rejects_invalid_json() {
        let raw = "not json";
        let err = DecisionPoint::parse(raw, "t", DecisionKind::ToolChoice);
        assert!(err.is_err());
    }
}
