//! Hallucination heuristics for tool outputs.
//!
//! Always-on, schema-driven where possible, with a fallback heuristic for
//! tools that do not declare a JSON schema. For per-output detection this
//! module is the single source of truth.
//!
//! # Heuristic
//!
//! The output is considered suspect when any of these is true:
//! - empty / whitespace-only ;
//! - exact `null` literal (`null`, `"null"`) ;
//! - empty JSON container (`{}`, `[]`) ;
//! - a [`SchemaValidator`] is provided and rejects the payload.
//!
//! The runtime calls [`detect_hallucination`] on every tool output (success
//! or failure). On a positive flag, the surrounding event carries an
//! [`apollia_core::ErrorAnalysis`] with category
//! [`apollia_core::ErrorCategory::HallucinationSuspected`].

use apollia_core::error_analysis::{ErrorAnalysis, ErrorCategory};

/// Per-tool schema validator. Implementors typically wrap a `jsonschema`
/// compiled validator or a custom Rust check.
///
/// Returning `Ok(())` means the payload is conformant; any `Err` is treated
/// as a schema violation (and contributes to the hallucination flag).
pub trait SchemaValidator: Send + Sync {
    /// Validate the JSON payload. Implementations should be cheap (compiled
    /// once, called per tool invocation).
    fn validate(&self, payload: &serde_json::Value) -> Result<(), String>;
}

/// Why the heuristic fired (returned alongside the boolean flag for
/// telemetry and "Show details" panels).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HeuristicReport {
    /// Output passed every check.
    Ok,
    /// Output was empty / whitespace-only.
    Empty,
    /// Output was the literal `null` (or quoted `"null"`).
    NullLiteral,
    /// Output parsed as an empty JSON container (`{}` / `[]`).
    EmptyContainer,
    /// A registered schema validator rejected the payload.
    SchemaViolation(String),
}

impl HeuristicReport {
    /// `true` for any non-`Ok` variant.
    pub fn is_suspect(&self) -> bool {
        !matches!(self, Self::Ok)
    }

    fn human_message(&self) -> &'static str {
        match self {
            Self::Ok => "",
            Self::Empty => "The tool returned an empty response.",
            Self::NullLiteral => "The tool returned `null` instead of data.",
            Self::EmptyContainer => "The tool returned an empty result set.",
            Self::SchemaViolation(_) => "The tool returned data that does not match its schema.",
        }
    }
}

/// Run the hallucination heuristic on a tool output.
///
/// `output` is the raw stringified output from the tool. `schema` is an
/// optional per-tool [`SchemaValidator`], when absent, only the fallback
/// heuristic (null / empty / empty container) runs.
///
/// Returns a [`HeuristicReport`] so callers can both decide and explain.
pub fn detect_hallucination(output: &str, schema: Option<&dyn SchemaValidator>) -> HeuristicReport {
    let trimmed = output.trim();

    if trimmed.is_empty() {
        return HeuristicReport::Empty;
    }
    if trimmed == "null" || trimmed == "\"null\"" || trimmed == "\"\"" {
        return HeuristicReport::NullLiteral;
    }

    // Try parsing as JSON for the empty-container and schema checks.
    let parsed = serde_json::from_str::<serde_json::Value>(trimmed);

    if let Ok(value) = &parsed {
        match value {
            serde_json::Value::Object(map) if map.is_empty() => {
                return HeuristicReport::EmptyContainer;
            }
            serde_json::Value::Array(arr) if arr.is_empty() => {
                return HeuristicReport::EmptyContainer;
            }
            serde_json::Value::Null => return HeuristicReport::NullLiteral,
            _ => {}
        }
    }

    if let Some(validator) = schema {
        let value = parsed.unwrap_or(serde_json::Value::String(trimmed.to_owned()));
        if let Err(e) = validator.validate(&value) {
            return HeuristicReport::SchemaViolation(e);
        }
    }

    HeuristicReport::Ok
}

/// Build an [`ErrorAnalysis`] from a positive [`HeuristicReport`].
///
/// Callers should only invoke this when `report.is_suspect()` returns `true`.
pub fn analysis_from_report(report: &HeuristicReport, raw_output: &str) -> ErrorAnalysis {
    ErrorAnalysis::new(
        ErrorCategory::HallucinationSuspected,
        report.human_message(),
        raw_output,
    )
    .with_hallucination(true)
}

// ─────────────────────────────────────────────
// Session-level aggregation
// ─────────────────────────────────────────────

/// Session-level aggregated inputs used to compute a global risk score.
///
/// The runtime fills this struct from:
/// - heuristic flags accumulated by `detect_hallucination` on tool outputs;
/// - assertions made without a supporting citation;
/// - contradictions detected between successive thinking steps.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct SessionHallucinationInputs {
    /// Number of positive heuristic flags (empty/null/schema violations).
    pub heuristic_flag_count: u32,
    /// Number of tool outputs observed in the session (denominator).
    pub total_tool_outputs: u32,
    /// Number of assertions made without a citation.
    pub assertion_citation_gaps: u32,
    /// Total number of assertions (denominator for the ratio).
    pub total_assertions: u32,
    /// Number of detected contradictions between thinking steps.
    pub thinking_contradictions: u32,
}

/// Aggregated score (0-100) plus short explanatory factors. Produced by the
/// always-on heuristic [`compute_session_hallucination_risk`]; may be
/// overwritten by the LLM routine `GenerateHallucinationRisk` when opted in.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct HallucinationRisk {
    /// Score 0-100.
    pub score: u8,
    /// Top factors (at most 5), short phrases.
    pub factors: Vec<String>,
}

impl HallucinationRisk {
    /// Safe fallback when no signal is available (score 0, no factors).
    pub fn zero() -> Self {
        Self {
            score: 0,
            factors: Vec::new(),
        }
    }

    /// Parse an LLM output (tolerates Markdown backticks).
    pub fn parse(raw: &str) -> Result<Self, serde_json::Error> {
        let clean = raw
            .trim()
            .trim_start_matches("```json")
            .trim_start_matches("```")
            .trim_end_matches("```")
            .trim();
        let mut parsed: HallucinationRisk = serde_json::from_str(clean)?;
        if parsed.score > 100 {
            parsed.score = 100;
        }
        if parsed.factors.len() > 5 {
            parsed.factors.truncate(5);
        }
        Ok(parsed)
    }
}

/// Deterministic heuristic, computes a 0-100 score from session signals.
/// Always-on: zero cost, no LLM.
///
/// # Formula
///
/// - 40 points weighted by the ratio `heuristic_flag_count / total_tool_outputs`.
/// - 40 points weighted by the ratio `assertion_citation_gaps / total_assertions`.
/// - 20 points weighted by `min(1.0, thinking_contradictions / 3.0)`.
pub fn compute_session_hallucination_risk(
    inputs: &SessionHallucinationInputs,
) -> HallucinationRisk {
    let tool_ratio = if inputs.total_tool_outputs == 0 {
        0.0
    } else {
        f64::from(inputs.heuristic_flag_count) / f64::from(inputs.total_tool_outputs)
    };
    let citation_ratio = if inputs.total_assertions == 0 {
        0.0
    } else {
        f64::from(inputs.assertion_citation_gaps) / f64::from(inputs.total_assertions)
    };
    let contradiction_ratio = (f64::from(inputs.thinking_contradictions) / 3.0).min(1.0);

    let raw = (tool_ratio * 40.0) + (citation_ratio * 40.0) + (contradiction_ratio * 20.0);
    let score = raw.round().clamp(0.0, 100.0) as u8;

    let mut factors: Vec<String> = Vec::new();
    if inputs.heuristic_flag_count > 0 {
        factors.push(format!(
            "{} suspect tool output{}",
            inputs.heuristic_flag_count,
            if inputs.heuristic_flag_count > 1 {
                "s"
            } else {
                ""
            }
        ));
    }
    if inputs.assertion_citation_gaps > 0 {
        factors.push(format!(
            "{} unsupported assertion{}",
            inputs.assertion_citation_gaps,
            if inputs.assertion_citation_gaps > 1 {
                "s"
            } else {
                ""
            }
        ));
    }
    if inputs.thinking_contradictions > 0 {
        factors.push(format!(
            "{} thinking contradiction{}",
            inputs.thinking_contradictions,
            if inputs.thinking_contradictions > 1 {
                "s"
            } else {
                ""
            }
        ));
    }

    HallucinationRisk { score, factors }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// GIVEN the tool output is a literal `null`
    /// WHEN detect_hallucination()
    /// THEN HeuristicReport::NullLiteral and is_suspect() = true.
    #[test]
    fn detects_null_literal() {
        let r = detect_hallucination("null", None);
        assert_eq!(r, HeuristicReport::NullLiteral);
        assert!(r.is_suspect());
    }

    /// GIVEN the tool output is an empty / whitespace-only string
    /// WHEN detect_hallucination()
    /// THEN HeuristicReport::Empty.
    #[test]
    fn detects_empty_output() {
        assert_eq!(detect_hallucination("", None), HeuristicReport::Empty);
        assert_eq!(
            detect_hallucination("   \n\t", None),
            HeuristicReport::Empty
        );
    }

    /// GIVEN the tool output is malformed JSON paired with a schema validator
    /// WHEN detect_hallucination()
    /// THEN HeuristicReport::SchemaViolation.
    #[test]
    fn detects_malformed_against_schema() {
        struct StrictObject;
        impl SchemaValidator for StrictObject {
            fn validate(&self, payload: &serde_json::Value) -> Result<(), String> {
                if payload.is_object() {
                    Ok(())
                } else {
                    Err("expected object".into())
                }
            }
        }
        // Not valid JSON → falls back to a String value → schema rejects it.
        let r = detect_hallucination("not json at all {{", Some(&StrictObject));
        assert!(matches!(r, HeuristicReport::SchemaViolation(_)));
        assert!(r.is_suspect());
    }

    /// GIVEN a valid non-empty JSON object with no schema
    /// WHEN detect_hallucination()
    /// THEN HeuristicReport::Ok.
    #[test]
    fn passes_valid_output() {
        let r = detect_hallucination(r#"{"answer": 42}"#, None);
        assert_eq!(r, HeuristicReport::Ok);
        assert!(!r.is_suspect());
    }

    /// GIVEN an empty JSON container `{}`
    /// WHEN detect_hallucination()
    /// THEN HeuristicReport::EmptyContainer.
    #[test]
    fn detects_empty_container() {
        assert_eq!(
            detect_hallucination("{}", None),
            HeuristicReport::EmptyContainer
        );
        assert_eq!(
            detect_hallucination("[]", None),
            HeuristicReport::EmptyContainer
        );
    }

    /// GIVEN a positive heuristic report
    /// WHEN analysis_from_report()
    /// THEN the resulting ErrorAnalysis carries the hallucination flag and
    ///      category HallucinationSuspected.
    #[test]
    fn builds_analysis_with_flag_set() {
        let report = HeuristicReport::NullLiteral;
        let a = analysis_from_report(&report, "null");
        assert_eq!(a.category, ErrorCategory::HallucinationSuspected);
        assert!(a.hallucination_suspected);
        assert!(!a.human_message.is_empty());
    }

    /// GIVEN a session with no signals
    /// WHEN compute_session_hallucination_risk()
    /// THEN score is 0 and factors is empty.
    #[test]
    fn session_risk_zero_when_no_signals() {
        let risk = compute_session_hallucination_risk(&SessionHallucinationInputs::default());
        assert_eq!(risk.score, 0);
        assert!(risk.factors.is_empty());
    }

    /// GIVEN all signals saturated
    /// WHEN compute_session_hallucination_risk()
    /// THEN score is 100 and factors enumerate contributors.
    #[test]
    fn session_risk_saturates_at_100() {
        let inputs = SessionHallucinationInputs {
            heuristic_flag_count: 5,
            total_tool_outputs: 5,
            assertion_citation_gaps: 10,
            total_assertions: 10,
            thinking_contradictions: 5,
        };
        let risk = compute_session_hallucination_risk(&inputs);
        assert_eq!(risk.score, 100);
        assert_eq!(risk.factors.len(), 3);
    }

    /// GIVEN only one empty tool output out of four
    /// WHEN compute_session_hallucination_risk()
    /// THEN score is around 10 (25% × 40 points).
    #[test]
    fn session_risk_partial_tool_flags() {
        let inputs = SessionHallucinationInputs {
            heuristic_flag_count: 1,
            total_tool_outputs: 4,
            ..Default::default()
        };
        let risk = compute_session_hallucination_risk(&inputs);
        assert_eq!(risk.score, 10);
        assert_eq!(risk.factors.len(), 1);
    }

    /// GIVEN a well-formed LLM response
    /// WHEN HallucinationRisk::parse()
    /// THEN struct is returned with clamped values.
    #[test]
    fn hallucination_risk_parses_and_clamps() {
        let raw = r#"```json
{"score": 150, "factors": ["a", "b", "c", "d", "e", "f", "g"]}
```"#;
        let parsed = HallucinationRisk::parse(raw).expect("parse ok");
        assert_eq!(parsed.score, 100);
        assert_eq!(parsed.factors.len(), 5);
    }
}
