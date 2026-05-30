//! Structured error analysis for tool / LLM failures.
//!
//! Lives in `apollia-core` so that [`crate::RuntimeEvent`] can carry an
//! [`ErrorAnalysis`] without `apollia-runtime` exposing an unrelated type. The
//! actual classifier (`error_analyzer.rs`) and the hallucination detector
//! (`hallucination_detector.rs`) live in `apollia-runtime/src/analyzers/`.
//!
//! # Design
//!
//! Two layers:
//! - **Always-on**: a static lookup maps a thiserror message to [`ErrorCategory`]
//!   plus an i18n-ready human key (`error.<snake>.human`). Cost: 0.
//! - **Opt-in**: when [`ErrorCategory::Unknown`], the runtime may invoke
//!   `MetaRoutine::GenerateErrorExplanation` to enrich `human_message` and
//!   produce a `suggested_action`. Cost: ~400+150 tokens when enabled.

use serde::{Deserialize, Serialize};

/// High-level classification of a runtime failure.
///
/// The variants are ordered roughly from "transient and recoverable" to
/// "needs human intervention". The frontend uses this to pick an icon and
/// a color in `ErrorCard.svelte`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCategory {
    /// A tool was invoked but its execution failed (non-zero exit, panic).
    ToolFailure,
    /// The LLM backend returned a non-recoverable error (auth, quota, etc.).
    LlmError,
    /// An operation exceeded its timeout window.
    Timeout,
    /// The tool/LLM produced no output at all (`null`, empty string).
    NullOutput,
    /// Output was returned but did not match the expected schema or shape.
    MalformedOutput,
    /// A permission gate refused the operation (filesystem, network, HITL).
    PermissionDenied,
    /// A network-level failure (DNS, TCP reset, TLS).
    NetworkError,
    /// The output looks like a fabrication (empty payload despite `success`,
    /// schema violation despite a "ok" field, etc.).
    HallucinationSuspected,
    /// Could not be classified; humanisation requires the opt-in LLM routine.
    Unknown,
}

impl ErrorCategory {
    /// Stable i18n key root used by the frontend (`error.<snake>.human`).
    pub fn i18n_key(self) -> &'static str {
        match self {
            Self::ToolFailure => "error.tool_failure",
            Self::LlmError => "error.llm_error",
            Self::Timeout => "error.timeout",
            Self::NullOutput => "error.null_output",
            Self::MalformedOutput => "error.malformed_output",
            Self::PermissionDenied => "error.permission_denied",
            Self::NetworkError => "error.network_error",
            Self::HallucinationSuspected => "error.hallucination_suspected",
            Self::Unknown => "error.unknown",
        }
    }
}

/// Structured analysis attached to failure-bearing [`crate::RuntimeEvent`]s.
///
/// Always carries a static `human_message` (no LLM required); when the
/// classifier returns [`ErrorCategory::Unknown`] and the user has opted in
/// to `routines.error_explanation`, the runtime replaces it with an LLM-
/// generated plain-language explanation and may set `suggested_action`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorAnalysis {
    /// Coarse category, drives the icon, color, and i18n root key.
    pub category: ErrorCategory,
    /// One sentence in plain language. Always populated.
    pub human_message: String,
    /// One actionable suggestion. `None` when the static mapping has none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suggested_action: Option<String>,
    /// `true` if the failure shape matches a hallucination heuristic.
    #[serde(default)]
    pub hallucination_suspected: bool,
    /// Raw error message (or stringified `thiserror`), for "Show details".
    pub technical_details: String,
}

impl ErrorAnalysis {
    /// Convenience constructor for the always-on (non-LLM) static path.
    pub fn new(
        category: ErrorCategory,
        human_message: impl Into<String>,
        technical: impl Into<String>,
    ) -> Self {
        Self {
            category,
            human_message: human_message.into(),
            suggested_action: None,
            hallucination_suspected: matches!(category, ErrorCategory::HallucinationSuspected),
            technical_details: technical.into(),
        }
    }

    /// Builder: attach a suggested action.
    pub fn with_suggested_action(mut self, action: impl Into<String>) -> Self {
        self.suggested_action = Some(action.into());
        self
    }

    /// Builder: flag this analysis as a hallucination candidate.
    pub fn with_hallucination(mut self, flag: bool) -> Self {
        self.hallucination_suspected = flag;
        self
    }
}
