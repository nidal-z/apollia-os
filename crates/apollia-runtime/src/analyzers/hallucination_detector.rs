//! Hallucination heuristics for tool outputs (US-SP42-039 — Pattern P3).
//!
//! Always-on, schema-driven where possible, with a fallback heuristic for
//! tools that do not declare a JSON schema. Shared with US-SP42-048
//! (session-level meta-layer) — this module is the single source of truth.
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
/// optional per-tool [`SchemaValidator`] — when absent, only the fallback
/// heuristic (null / empty / empty container) runs.
///
/// Returns a [`HeuristicReport`] so callers can both decide and explain.
pub fn detect_hallucination(
    output: &str,
    schema: Option<&dyn SchemaValidator>,
) -> HeuristicReport {
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
        assert_eq!(detect_hallucination("   \n\t", None), HeuristicReport::Empty);
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
}
