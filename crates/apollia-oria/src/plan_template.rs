//! One plan for every item of a batch.
//!
//! Every item of a batch asks the same thing of the agent on other values: a
//! plan per item cost a planner call each time (about 30 seconds on a local
//! model) and repeated the same guesses item after item. The planner is now
//! asked once, for a template that names the values of an item by their field
//! (`{{File}}`), and every item fills the template with its own values before
//! it runs. The template is kept only once it has run through as it was, and
//! only when it holds no value of the item it was planned on: a plan that
//! copied a file name would read that file for every item.

use crate::plan::ExecutionPlan;

/// A value shorter than this is too common to tell a copied value apart from
/// a word of the plan (`1`, `oui`), and is not looked for.
const DISTINCTIVE_LEN: usize = 6;

/// The batch an item belongs to and the values of the item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanTemplate {
    /// What the template is kept under: the batch.
    pub key: String,
    /// The fields of the item, in the order of the source, with their values.
    pub fields: Vec<(String, String)>,
}

impl PlanTemplate {
    /// What the planner is told: the fields, and how to name their values.
    #[must_use]
    pub fn note(&self) -> String {
        let names: Vec<String> = self
            .fields
            .iter()
            .map(|(name, _)| format!("{{{{{name}}}}}"))
            .collect();
        format!(
            "THIS PLAN IS REUSED FOR EVERY ITEM OF A BATCH. The task above is one item. \
             Wherever a step uses a value of the item, write its field instead of the value: \
             {}. Never copy a value of this item into the plan.",
            names.join(", ")
        )
    }

    /// `template` with every `{{field}}` replaced by the value of this item,
    /// in the description and in every string of the arguments of each step.
    #[must_use]
    pub fn apply(&self, template: &ExecutionPlan) -> ExecutionPlan {
        let mut plan = template.clone();
        for step in &mut plan.steps {
            step.description = self.fill(&step.description);
            if let Some(args) = step.args.as_mut() {
                self.fill_value(args);
            }
        }
        plan
    }

    /// Whether `plan` can be kept as the template of the batch: it names no
    /// distinctive value of the item it was planned on.
    #[must_use]
    pub fn reusable(&self, plan: &ExecutionPlan) -> bool {
        let text = serde_json::to_string(&plan.steps).unwrap_or_default();
        !self.fields.iter().any(|(_, value)| {
            value.chars().count() >= DISTINCTIVE_LEN && text.contains(value.as_str())
        })
    }

    fn fill(&self, text: &str) -> String {
        let mut out = text.to_string();
        for (name, value) in &self.fields {
            out = out.replace(&format!("{{{{{name}}}}}"), value);
        }
        out
    }

    fn fill_value(&self, value: &mut serde_json::Value) {
        match value {
            serde_json::Value::String(text) => *text = self.fill(text),
            serde_json::Value::Array(items) => items.iter_mut().for_each(|v| self.fill_value(v)),
            serde_json::Value::Object(map) => map.values_mut().for_each(|v| self.fill_value(v)),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::PlanStep;

    fn template() -> PlanTemplate {
        PlanTemplate {
            key: "batch-1".into(),
            fields: vec![
                ("File".into(), "quote-07-joinery.pdf".into()),
                ("No".into(), "7".into()),
            ],
        }
    }

    fn plan_with(description: &str, args: serde_json::Value) -> ExecutionPlan {
        let mut step = PlanStep::new("s1", description);
        step.tool_hint = Some("file_read".into());
        step.args = Some(args);
        ExecutionPlan {
            plan_id: "p".into(),
            task_id: "t".into(),
            steps: vec![step],
        }
    }

    #[test]
    fn test_an_item_fills_the_template_with_its_values() {
        // GIVEN a template that names the file of an item by its field
        let plan = plan_with(
            "Read {{File}}",
            serde_json::json!({"path": "Quotes/{{File}}", "pages": [1]}),
        );

        // WHEN an item fills it
        let filled = template().apply(&plan);

        // THEN its value is in the description and in the arguments
        assert_eq!(filled.steps[0].description, "Read quote-07-joinery.pdf");
        assert_eq!(
            filled.steps[0].args,
            Some(serde_json::json!({"path": "Quotes/quote-07-joinery.pdf", "pages": [1]}))
        );
    }

    #[test]
    fn test_a_plan_that_copied_a_value_is_not_kept_as_template() {
        // GIVEN a plan that wrote the file of the item it was planned on
        let copied = plan_with(
            "Read it",
            serde_json::json!({"path": "Quotes/quote-07-joinery.pdf"}),
        );
        let named = plan_with("Read it", serde_json::json!({"path": "Quotes/{{File}}"}));

        // WHEN each is checked
        // THEN the copy is not reusable, the template is, and a short value is not looked for
        assert!(!template().reusable(&copied));
        assert!(template().reusable(&named));
    }

    #[test]
    fn test_the_planner_is_told_the_fields() {
        // GIVEN the template of a batch
        // WHEN its note is written
        let note = template().note();

        // THEN it names each field as a placeholder
        assert!(
            note.contains("{{File}}") && note.contains("{{No}}"),
            "{note}"
        );
    }
}
