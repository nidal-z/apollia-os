//! Step condition evaluation for pipeline steps.
//!
//! A [`StepCondition`] guards a pipeline step: if [`evaluate_condition`] returns
//! `false`, the step is skipped and downstream steps are not blocked.
//!
//! # Guarantees
//!
//! - **Pure function**: `evaluate_condition` has no side effects.
//! - **Panic-safe**: a missing field resolves to `""`, an invalid regex
//!   emits a `tracing::warn!` and returns `false`. No panic in any path.

use crate::template::TemplateContext;
use crate::types::{ConditionKind, StepCondition, StepId};

// ── Public API ────────────────────────────────────────────────────────────────

/// Evaluates a step condition against the current execution context.
///
/// Returns `true` if the condition is satisfied and the step should execute,
/// `false` otherwise (step should be skipped).
///
/// # Absent fields
///
/// If the field referenced by `condition.field` is not present in `context`,
/// the resolved value is treated as an empty string, which causes all
/// operators except `Equals("")` and `StartsWith("")`/`EndsWith("")` to
/// return `false`.
///
/// # Invalid regex
///
/// When `condition.when` is [`ConditionKind::Regex`] and `condition.value`
/// is not a valid pattern, a `tracing::warn!` is emitted and `false` is
/// returned.
pub fn evaluate_condition(condition: &StepCondition, context: &TemplateContext) -> bool {
    let field_value = resolve_field(&condition.field, context);

    match &condition.when {
        ConditionKind::Contains => field_value.contains(condition.value.as_str()),
        ConditionKind::Equals => field_value == condition.value,
        ConditionKind::StartsWith => field_value.starts_with(condition.value.as_str()),
        ConditionKind::EndsWith => field_value.ends_with(condition.value.as_str()),
        ConditionKind::Regex => match regex::Regex::new(&condition.value) {
            Ok(re) => re.is_match(&field_value),
            Err(_) => {
                tracing::warn!(
                    pattern = %condition.value,
                    "invalid regex in step condition — treating as false",
                );
                false
            }
        },
    }
}

// ── Private helpers ───────────────────────────────────────────────────────────

/// Resolves a dot-path field reference to its string value in `context`.
///
/// Supported paths:
///
/// | Path | Source |
/// |---|---|
/// | `"steps.<id>.output"` | `context.step_outputs[<id>]`, `""` if absent |
/// | `"trigger.payload"` | `context.trigger_payload` |
/// | `"pipeline.id"` | `context.pipeline_id` |
/// | `"pipeline.run_id"` | `context.run_id` |
///
/// Any unrecognised path returns an empty string without panicking.
fn resolve_field(field: &str, context: &TemplateContext) -> String {
    if let Some(step_id) = parse_step_output_path(field) {
        return context
            .step_outputs
            .get(&StepId(step_id.to_string()))
            .cloned()
            .unwrap_or_default();
    }

    match field {
        "trigger.payload" => context.trigger_payload.clone(),
        "pipeline.id" => context.pipeline_id.clone(),
        "pipeline.run_id" => context.run_id.clone(),
        _ => String::new(),
    }
}

/// Parses a `"steps.<id>.output"` path and returns the step-id substring.
///
/// Returns `None` when the path does not match this pattern or the extracted
/// step-id is empty or contains a dot (which would indicate a malformed path).
fn parse_step_output_path(field: &str) -> Option<&str> {
    let rest = field.strip_prefix("steps.")?;
    let step_id = rest.strip_suffix(".output")?;
    if step_id.is_empty() || step_id.contains('.') {
        return None;
    }
    Some(step_id)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::template::TemplateContext;
    use crate::types::{ConditionKind, StepCondition, StepId};

    /// Builds a context with a single step output pre-inserted.
    fn ctx_with_step_output(step: &str, output: &str) -> TemplateContext {
        let mut ctx = TemplateContext::new("payload".into(), "p1".into(), "r-1".into());
        ctx.step_outputs.insert(StepId(step.into()), output.into());
        ctx
    }

    // AC-1 — Contains: step executed when output contains the value
    #[test]
    fn test_ac1_contains_true() {
        // GIVEN
        let cond = StepCondition {
            when: ConditionKind::Contains,
            field: "steps.validation.output".into(),
            value: "FRAUDE_DETECTEE".into(),
        };
        let ctx = ctx_with_step_output("validation", "Facture FRAUDE_DETECTEE détectée");

        // WHEN / THEN
        assert!(evaluate_condition(&cond, &ctx));
    }

    // AC-2 — Contains: step skipped when output does not contain the value
    #[test]
    fn test_ac2_contains_false() {
        // GIVEN
        let cond = StepCondition {
            when: ConditionKind::Contains,
            field: "steps.validation.output".into(),
            value: "FRAUDE_DETECTEE".into(),
        };
        let ctx = ctx_with_step_output("validation", "Facture valide — RAS");

        // WHEN / THEN
        assert!(!evaluate_condition(&cond, &ctx));
    }

    // AC-3 — Equals: case-sensitive exact match
    #[test]
    fn test_ac3_equals() {
        // GIVEN
        let cond = StepCondition {
            when: ConditionKind::Equals,
            field: "steps.s.output".into(),
            value: "ok".into(),
        };

        // WHEN / THEN — matching case succeeds
        assert!(evaluate_condition(&cond, &ctx_with_step_output("s", "ok")));
        // WHEN / THEN — different case fails
        assert!(!evaluate_condition(&cond, &ctx_with_step_output("s", "OK")));
    }

    // AC-3 — StartsWith
    #[test]
    fn test_ac3_starts_with() {
        // GIVEN
        let cond = StepCondition {
            when: ConditionKind::StartsWith,
            field: "steps.s.output".into(),
            value: "ERR".into(),
        };

        // WHEN / THEN
        assert!(evaluate_condition(
            &cond,
            &ctx_with_step_output("s", "ERR: timeout")
        ));
        assert!(!evaluate_condition(
            &cond,
            &ctx_with_step_output("s", "OK: all good")
        ));
    }

    // AC-3 — EndsWith
    #[test]
    fn test_ac3_ends_with() {
        // GIVEN
        let cond = StepCondition {
            when: ConditionKind::EndsWith,
            field: "steps.s.output".into(),
            value: "DONE".into(),
        };

        // WHEN / THEN
        assert!(evaluate_condition(
            &cond,
            &ctx_with_step_output("s", "processing DONE")
        ));
        assert!(!evaluate_condition(
            &cond,
            &ctx_with_step_output("s", "DONE processing")
        ));
    }

    // AC-4 — Regex: match and non-match
    #[test]
    fn test_ac4_regex_match() {
        // GIVEN
        let cond = StepCondition {
            when: ConditionKind::Regex,
            field: "steps.ocr.output".into(),
            value: r"\d{4,}".into(),
        };

        // WHEN / THEN — 4+ consecutive digits match
        assert!(evaluate_condition(
            &cond,
            &ctx_with_step_output("ocr", "Montant : 4250€")
        ));
        // WHEN / THEN — fewer than 4 digits do not match
        assert!(!evaluate_condition(
            &cond,
            &ctx_with_step_output("ocr", "Montant : 42€")
        ));
    }

    // AC-5 — Absent field returns false, no panic
    #[test]
    fn test_ac5_absent_field_returns_false() {
        // GIVEN — context has no output for "absent"
        let cond = StepCondition {
            when: ConditionKind::Contains,
            field: "steps.absent.output".into(),
            value: "X".into(),
        };
        let ctx = TemplateContext::new("".into(), "p".into(), "r".into());

        // WHEN / THEN
        assert!(!evaluate_condition(&cond, &ctx));
    }

    // Invalid regex → false, no panic
    #[test]
    fn test_invalid_regex_returns_false_no_panic() {
        // GIVEN
        let cond = StepCondition {
            when: ConditionKind::Regex,
            field: "steps.s.output".into(),
            value: "[invalid".into(), // intentionally malformed
        };

        // WHEN / THEN — does not panic
        let result = evaluate_condition(&cond, &ctx_with_step_output("s", "test"));
        assert!(!result);
    }

    // trigger.payload field resolution
    #[test]
    fn test_trigger_payload_field() {
        // GIVEN
        let cond = StepCondition {
            when: ConditionKind::Contains,
            field: "trigger.payload".into(),
            value: "invoice".into(),
        };
        let ctx = TemplateContext::new("invoice.pdf".into(), "p".into(), "r".into());

        // WHEN / THEN
        assert!(evaluate_condition(&cond, &ctx));
    }

    // pipeline.id field resolution
    #[test]
    fn test_pipeline_id_field() {
        // GIVEN
        let cond = StepCondition {
            when: ConditionKind::Equals,
            field: "pipeline.id".into(),
            value: "billing".into(),
        };
        let ctx = TemplateContext::new("".into(), "billing".into(), "r-1".into());

        // WHEN / THEN
        assert!(evaluate_condition(&cond, &ctx));
    }

    // Unknown field resolves to "" → false for Contains("X")
    #[test]
    fn test_unknown_field_resolves_to_empty() {
        // GIVEN
        let cond = StepCondition {
            when: ConditionKind::Contains,
            field: "unknown.path.here".into(),
            value: "X".into(),
        };
        let ctx = TemplateContext::new("".into(), "p".into(), "r".into());

        // WHEN / THEN
        assert!(!evaluate_condition(&cond, &ctx));
    }
}
