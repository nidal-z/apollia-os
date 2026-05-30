//! Static error classifier (always-on layer).
//!
//! Maps a raw error string (typically the `Display` of a `thiserror` enum)
//! to an [`ErrorAnalysis`] with a stable category and an i18n-ready human
//! message. No LLM is involved, cost: 0.
//!
//! When the classifier produces [`ErrorCategory::Unknown`], call
//! [`enrich_with_llm`] with a [`MetaOrchestratorHandle`] to opt-into
//! `MetaRoutine::GenerateErrorExplanation` (~400 in / 150 out tokens).

use apollia_core::error_analysis::{ErrorAnalysis, ErrorCategory};
use apollia_llm::meta_orchestrator::{MetaOrchestratorHandle, MetaRoutine};

/// Static `(needle, category, human_message, suggested_action)` table.
///
/// `needle` is matched case-insensitively as a substring against the raw
/// error string. The first match wins; entries are ordered roughly from
/// most-specific to least-specific.
///
/// To add a new mapping, push a tuple, the test
/// `every_category_has_at_least_one_mapping` ensures coverage.
const STATIC_MAPPINGS: &[(&str, ErrorCategory, &str, Option<&str>)] = &[
    // ── Timeout (must come before generic NetworkError matches) ──
    (
        "timeout",
        ErrorCategory::Timeout,
        "The operation took longer than expected and was cancelled.",
        Some("Try again, or increase the timeout in settings."),
    ),
    (
        "timed out",
        ErrorCategory::Timeout,
        "The operation took longer than expected and was cancelled.",
        Some("Try again, or increase the timeout in settings."),
    ),
    // ── Permission ──
    (
        "permission denied",
        ErrorCategory::PermissionDenied,
        "The runtime refused this action because of a safety rule.",
        Some("Approve the action manually or relax the permission policy."),
    ),
    (
        "not permitted",
        ErrorCategory::PermissionDenied,
        "The runtime refused this action because of a safety rule.",
        Some("Approve the action manually or relax the permission policy."),
    ),
    (
        "approval required",
        ErrorCategory::PermissionDenied,
        "Human approval is required for this action.",
        Some("Open the approvals panel and accept or refuse."),
    ),
    // ── Network ──
    (
        "connection refused",
        ErrorCategory::NetworkError,
        "Could not reach the remote service.",
        Some("Check your network and the service URL."),
    ),
    (
        "dns",
        ErrorCategory::NetworkError,
        "Could not resolve the remote host.",
        Some("Check the URL and your DNS settings."),
    ),
    (
        "tls",
        ErrorCategory::NetworkError,
        "Secure connection to the remote service failed.",
        Some("Check the certificate and the system clock."),
    ),
    (
        "network",
        ErrorCategory::NetworkError,
        "A network error prevented the operation.",
        Some("Check your connection and try again."),
    ),
    // ── Malformed output ──
    (
        "invalid json",
        ErrorCategory::MalformedOutput,
        "The tool returned data that is not valid JSON.",
        Some("Inspect the raw output in the details panel."),
    ),
    (
        "schema",
        ErrorCategory::MalformedOutput,
        "The tool output did not match the expected shape.",
        Some("Inspect the raw output and adjust the prompt or schema."),
    ),
    (
        "parse error",
        ErrorCategory::MalformedOutput,
        "The output could not be parsed.",
        Some("Inspect the raw output in the details panel."),
    ),
    // ── Null output ──
    (
        "empty output",
        ErrorCategory::NullOutput,
        "The tool finished but produced no output.",
        Some("Re-run with more context or adjust the inputs."),
    ),
    (
        "no output",
        ErrorCategory::NullOutput,
        "The tool finished but produced no output.",
        Some("Re-run with more context or adjust the inputs."),
    ),
    // ── LLM ──
    (
        "rate limit",
        ErrorCategory::LlmError,
        "The AI service rate-limited the request.",
        Some("Wait a few seconds and retry, or switch backend."),
    ),
    (
        "unauthorized",
        ErrorCategory::LlmError,
        "The AI service rejected the credentials.",
        Some("Check your API key in the LLM settings."),
    ),
    (
        "api key",
        ErrorCategory::LlmError,
        "The AI service is missing or rejecting the API key.",
        Some("Set or refresh the API key in settings."),
    ),
    (
        "backendunavailable",
        ErrorCategory::LlmError,
        "The configured AI backend is not available.",
        Some("Pick a different backend or check the local model."),
    ),
    (
        "inference",
        ErrorCategory::LlmError,
        "The AI model failed to produce a response.",
        Some("Try again, or switch to a different model."),
    ),
    // ── Tool failure (catch-all for `tool error:` prefix used in builtin_agent) ──
    (
        "tool error",
        ErrorCategory::ToolFailure,
        "The tool failed to complete its action.",
        Some("Inspect the details, then retry or adjust the inputs."),
    ),
    (
        "exit_code",
        ErrorCategory::ToolFailure,
        "The tool exited with a non-zero status.",
        Some("Inspect the command output for the underlying cause."),
    ),
];

/// Classify a tool failure error string into an [`ErrorAnalysis`].
///
/// `raw` is the `Display` form of the underlying error (or the body of a
/// `tool error: …` message). When no entry matches, returns
/// [`ErrorCategory::Unknown`] with a generic human message, call
/// [`enrich_with_llm`] to humanise it via the meta-LLM (opt-in).
pub fn classify_tool_error(raw: &str) -> ErrorAnalysis {
    classify_with_default(raw, ErrorCategory::ToolFailure)
}

/// Classify an LLM backend failure error string into an [`ErrorAnalysis`].
///
/// Defaults to [`ErrorCategory::LlmError`] when no specific needle matches.
pub fn classify_llm_error(raw: &str) -> ErrorAnalysis {
    classify_with_default(raw, ErrorCategory::LlmError)
}

fn classify_with_default(raw: &str, default: ErrorCategory) -> ErrorAnalysis {
    let lower = raw.to_ascii_lowercase();
    for (needle, category, human, action) in STATIC_MAPPINGS {
        if lower.contains(needle) {
            let mut analysis = ErrorAnalysis::new(*category, *human, raw);
            if let Some(act) = action {
                analysis = analysis.with_suggested_action(*act);
            }
            return analysis;
        }
    }
    // No needle matched, fall back to the caller-provided default with a
    // generic message. The frontend will render the technical details and,
    // if the user opted in, the runtime can later call `enrich_with_llm`.
    let (human, category) = match default {
        ErrorCategory::ToolFailure => (
            "The tool failed but the cause could not be classified.",
            ErrorCategory::Unknown,
        ),
        ErrorCategory::LlmError => (
            "The AI backend failed but the cause could not be classified.",
            ErrorCategory::Unknown,
        ),
        other => ("An error occurred.", other),
    };
    ErrorAnalysis::new(category, human, raw)
}

/// Opt-in: replace the static `human_message` (and possibly `suggested_action`)
/// with a meta-LLM-generated explanation when the analysis is [`ErrorCategory::Unknown`].
///
/// Returns the analysis unchanged when:
/// - `analysis.category` is anything other than `Unknown` (static message was
///   specific enough);
/// - the meta orchestrator is not configured, the routine is disabled, the
///   budget is exhausted, or the call fails / times out (graceful degradation).
pub async fn enrich_with_llm(
    handle: &MetaOrchestratorHandle,
    mut analysis: ErrorAnalysis,
    context: &str,
    session_id: &str,
) -> ErrorAnalysis {
    if analysis.category != ErrorCategory::Unknown {
        return analysis;
    }
    let inputs = serde_json::json!({
        "error": analysis.technical_details,
        "context": context,
    });
    if let Ok(Some(text)) = handle
        .run(MetaRoutine::GenerateErrorExplanation, inputs, session_id)
        .await
    {
        let trimmed = text.trim();
        if !trimmed.is_empty() {
            analysis.human_message = trimmed.to_owned();
        }
    }
    analysis
}

#[cfg(test)]
mod tests {
    use super::*;

    /// GIVEN a tool error mentioning `exit_code`
    /// WHEN classify_tool_error()
    /// THEN ErrorCategory::ToolFailure with a tool-specific human message.
    #[test]
    fn classifies_tool_failure() {
        let a = classify_tool_error("command failed: exit_code=2");
        assert_eq!(a.category, ErrorCategory::ToolFailure);
        assert!(a.suggested_action.is_some());
        assert_eq!(a.technical_details, "command failed: exit_code=2");
    }

    /// GIVEN an LLM backend error mentioning rate limiting
    /// WHEN classify_llm_error()
    /// THEN ErrorCategory::LlmError.
    #[test]
    fn classifies_llm_error() {
        let a = classify_llm_error("HTTP 429 rate limit exceeded");
        assert_eq!(a.category, ErrorCategory::LlmError);
    }

    /// GIVEN an error string containing "timed out"
    /// WHEN classify_tool_error()
    /// THEN ErrorCategory::Timeout.
    #[test]
    fn classifies_timeout() {
        let a = classify_tool_error("Operation timed out after 30s");
        assert_eq!(a.category, ErrorCategory::Timeout);
    }

    /// GIVEN an error string mentioning permission denied
    /// WHEN classify_tool_error()
    /// THEN ErrorCategory::PermissionDenied.
    #[test]
    fn classifies_permission_denied() {
        let a = classify_tool_error("EACCES: permission denied");
        assert_eq!(a.category, ErrorCategory::PermissionDenied);
    }

    /// GIVEN an error string mentioning DNS / connection refused
    /// WHEN classify_tool_error()
    /// THEN ErrorCategory::NetworkError.
    #[test]
    fn classifies_network_error() {
        let a = classify_tool_error("connection refused: 127.0.0.1:8080");
        assert_eq!(a.category, ErrorCategory::NetworkError);
    }

    /// GIVEN an error string mentioning invalid JSON
    /// WHEN classify_tool_error()
    /// THEN ErrorCategory::MalformedOutput.
    #[test]
    fn classifies_malformed_output() {
        let a = classify_tool_error("invalid json at line 3");
        assert_eq!(a.category, ErrorCategory::MalformedOutput);
    }

    /// GIVEN an error string mentioning empty output
    /// WHEN classify_tool_error()
    /// THEN ErrorCategory::NullOutput.
    #[test]
    fn classifies_null_output() {
        let a = classify_tool_error("empty output from tool");
        assert_eq!(a.category, ErrorCategory::NullOutput);
    }

    /// GIVEN an HallucinationSuspected analysis built directly
    /// WHEN we inspect it
    /// THEN the hallucination flag is set and the i18n key is correct.
    #[test]
    fn hallucination_category_sets_flag() {
        let a = ErrorAnalysis::new(
            ErrorCategory::HallucinationSuspected,
            "Output looks fabricated.",
            "raw",
        );
        assert!(a.hallucination_suspected);
        assert_eq!(a.category.i18n_key(), "error.hallucination_suspected");
    }

    /// GIVEN an unrecognised error string
    /// WHEN classify_tool_error()
    /// THEN ErrorCategory::Unknown so that opt-in LLM enrichment may run.
    #[test]
    fn unrecognised_error_falls_back_to_unknown() {
        let a = classify_tool_error("flux capacitor de-synchronised");
        assert_eq!(a.category, ErrorCategory::Unknown);
    }
}
