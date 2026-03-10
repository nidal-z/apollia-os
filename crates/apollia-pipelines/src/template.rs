//! Template renderer for pipeline step inputs.
//!
//! Interpolates `{{steps.<id>.output}}`, `{{trigger.payload}}`,
//! `{{pipeline.id}}` and `{{pipeline.run_id}}` placeholders at step submission
//! time. Unknown or unresolved variables are replaced with an empty string —
//! no panic, no error.

use crate::types::StepId;
use std::collections::HashMap;

// ── TemplateContext ────────────────────────────────────────────────────────────

/// Interpolation context available when a pipeline step is about to be submitted.
///
/// Build one `TemplateContext` per pipeline run and call [`TemplateContext::insert_step_output`]
/// after each step completes. Pass `&self` to [`TemplateContext::render`] to
/// resolve the template string of every subsequent step.
pub struct TemplateContext {
    /// Value substituted for `{{trigger.payload}}`.
    ///
    /// Empty when the pipeline was triggered manually without a payload.
    pub trigger_payload: String,
    /// Value substituted for `{{pipeline.id}}`.
    pub pipeline_id: String,
    /// Value substituted for `{{pipeline.run_id}}`.
    pub run_id: String,
    /// Map of `step_id → output` populated as steps complete.
    ///
    /// Each entry provides the value for the `{{steps.<id>.output}}` placeholder.
    pub step_outputs: HashMap<StepId, String>,
}

impl TemplateContext {
    /// Creates a minimal context with no step outputs yet.
    ///
    /// # Arguments
    ///
    /// * `trigger_payload` — raw payload from the trigger (file path, webhook body, …).
    ///   Pass an empty string when there is no payload.
    /// * `pipeline_id` — identifier of the pipeline definition (e.g. `"traitement-facture"`).
    /// * `run_id` — identifier of the current run (e.g. `"r-0017"`).
    pub fn new(trigger_payload: String, pipeline_id: String, run_id: String) -> Self {
        Self {
            trigger_payload,
            pipeline_id,
            run_id,
            step_outputs: HashMap::new(),
        }
    }

    /// Records the output of a completed step so it can be referenced by
    /// downstream steps via `{{steps.<id>.output}}`.
    pub fn insert_step_output(&mut self, step_id: StepId, output: String) {
        self.step_outputs.insert(step_id, output);
    }

    /// Interpolates all known variables in `template` and replaces any
    /// remaining `{{…}}` placeholders with an empty string.
    ///
    /// This function is pure: it has no side effects and always returns a
    /// value — it never panics regardless of the template contents.
    ///
    /// # Variable reference
    ///
    /// | Placeholder | Source |
    /// |---|---|
    /// | `{{trigger.payload}}` | [`Self::trigger_payload`] |
    /// | `{{pipeline.id}}` | [`Self::pipeline_id`] |
    /// | `{{pipeline.run_id}}` | [`Self::run_id`] |
    /// | `{{steps.<id>.output}}` | [`Self::step_outputs`] for the given `<id>` |
    ///
    /// # Example
    ///
    /// ```rust
    /// use apollia_pipelines::template::TemplateContext;
    ///
    /// let ctx = TemplateContext::new(
    ///     "invoice.pdf".into(),
    ///     "billing".into(),
    ///     "r-0001".into(),
    /// );
    /// assert_eq!(
    ///     ctx.render("File: {{trigger.payload}} — pipeline {{pipeline.id}}"),
    ///     "File: invoice.pdf — pipeline billing",
    /// );
    /// ```
    pub fn render(&self, template: &str) -> String {
        let mut result = template.to_string();

        // Replace well-known fixed variables first.
        result = result
            .replace("{{trigger.payload}}", &self.trigger_payload)
            .replace("{{pipeline.id}}", &self.pipeline_id)
            .replace("{{pipeline.run_id}}", &self.run_id);

        // Replace dynamic per-step variables: {{steps.<id>.output}}
        for (step_id, output) in &self.step_outputs {
            let placeholder = format!("{{{{steps.{}.output}}}}", step_id);
            result = result.replace(&placeholder, output);
        }

        // Remove any remaining unresolved {{…}} placeholders.
        // Implemented without the `regex` crate — pure string scanning.
        loop {
            match result.find("{{") {
                None => break,
                Some(start) => match result[start..].find("}}") {
                    None => break, // unclosed `{{` — leave as-is
                    Some(relative_end) => {
                        let end = start + relative_end + 2;
                        result.replace_range(start..end, "");
                    }
                },
            }
        }

        result
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::StepId;

    /// Helper: build a context pre-populated with the given step outputs.
    fn ctx_with_outputs(outputs: &[(&str, &str)]) -> TemplateContext {
        let mut ctx = TemplateContext::new(
            "acme-2026-03.pdf".into(),
            "traitement-facture".into(),
            "r-0017".into(),
        );
        for (k, v) in outputs {
            ctx.step_outputs
                .insert(StepId(k.to_string()), v.to_string());
        }
        ctx
    }

    #[test]
    fn test_ac1_step_output_interpolated() {
        // GIVEN
        let ctx = ctx_with_outputs(&[("ocr", "texte extrait 2 pages")]);
        // WHEN
        let result = ctx.render("Valide ceci : {{steps.ocr.output}}");
        // THEN
        assert_eq!(result, "Valide ceci : texte extrait 2 pages");
    }

    #[test]
    fn test_ac2_trigger_payload_interpolated() {
        // GIVEN
        let ctx = ctx_with_outputs(&[]);
        // WHEN
        let result = ctx.render("Traite le fichier : {{trigger.payload}}");
        // THEN
        assert_eq!(result, "Traite le fichier : acme-2026-03.pdf");
    }

    #[test]
    fn test_ac3_unknown_variable_replaced_with_empty() {
        // GIVEN — no output registered for "validation"
        let ctx = ctx_with_outputs(&[]);
        // WHEN
        let result = ctx.render("Résultat : {{steps.validation.output}} — fin");
        // THEN
        assert_eq!(result, "Résultat :  — fin");
    }

    #[test]
    fn test_ac4_multi_variable_template() {
        // GIVEN
        let ctx = ctx_with_outputs(&[("ocr", "ok"), ("validation", "valide")]);
        // WHEN
        let result =
            ctx.render("{{trigger.payload}} / {{steps.ocr.output}} / {{steps.validation.output}}");
        // THEN
        assert_eq!(result, "acme-2026-03.pdf / ok / valide");
    }

    #[test]
    fn test_ac5_pipeline_id_and_run_id() {
        // GIVEN
        let ctx = ctx_with_outputs(&[]);
        // WHEN
        let result = ctx.render("Pipeline {{pipeline.id}} run {{pipeline.run_id}}");
        // THEN
        assert_eq!(result, "Pipeline traitement-facture run r-0017");
    }

    #[test]
    fn test_no_panic_on_unclosed_braces() {
        // GIVEN
        let ctx = ctx_with_outputs(&[]);
        // WHEN — malformed template with an unclosed {{
        let result = ctx.render("Hello {{trigger.payload}} et {{ non fermé");
        // THEN — does not panic; closed placeholders are resolved
        assert!(result.contains("acme-2026-03.pdf"));
    }
}
