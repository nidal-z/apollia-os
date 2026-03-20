//! Validation structurelle des definitions de pipelines.
//!
//! Ce module centralise la validation executee avant chaque ecriture
//! (insert/update) dans le [`crate::definition_repository::PipelineDefinitionRepository`].
//! La validation DAG reutilise l'algorithme Kahn BFS de [`crate::topo::topological_layers`]
//! pour la detection de cycles (Principe #4 — Fail fast).

use std::collections::HashSet;

use crate::definition_repository::{PipelineDefinitionError, PipelineDefinitionRow};
use crate::topo::{topological_layers, TopologicalError};
use crate::types::{PipelineStepDef, StepFailurePolicy};

/// Valide une [`PipelineDefinitionRow`] complete avant insert ou update.
///
/// Delegue la validation des steps au DAG via [`validate_pipeline_dag`].
pub fn validate_pipeline(def: &PipelineDefinitionRow) -> Result<(), PipelineDefinitionError> {
    validate_pipeline_dag(&def.steps)
}

/// Valide la structure DAG d'une liste de steps de pipeline.
///
/// Verifications effectuees (fail-fast, dans cet ordre) :
/// 1. Le pipeline doit avoir au moins un step (AC-10)
/// 2. Les identifiants de steps doivent etre uniques (AC-7)
/// 3. Les `depends_on` doivent referencer des steps existants (AC-8)
/// 4. Le graphe de dependances doit etre acyclique (AC-6)
/// 5. Les `fallback_for` doivent referencer des steps avec `on_failure=fallback` (AC-9)
pub fn validate_pipeline_dag(steps: &[PipelineStepDef]) -> Result<(), PipelineDefinitionError> {
    // AC-10: au moins un step
    if steps.is_empty() {
        return Err(PipelineDefinitionError::ValidationError(
            "pipeline must have at least one step".to_string(),
        ));
    }

    // AC-7: step_id uniques
    let mut seen_ids = HashSet::with_capacity(steps.len());
    for step in steps {
        if !seen_ids.insert(&step.id) {
            return Err(PipelineDefinitionError::ValidationError(format!(
                "duplicate step id: {}",
                step.id
            )));
        }
    }

    // AC-6 + AC-8: cycle detection + depends_on valides via topo.rs
    topological_layers(steps).map_err(|e| match e {
        TopologicalError::CycleDetected(_) => {
            PipelineDefinitionError::ValidationError("cycle detected in pipeline DAG".to_string())
        }
        TopologicalError::UnknownDependency { dependency, .. } => {
            PipelineDefinitionError::ValidationError(format!(
                "depends_on references unknown step: {dependency}"
            ))
        }
    })?;

    // AC-9: fallback_for doit referencer un step avec on_failure=Fallback
    for step in steps {
        if let Some(ref target_id) = step.fallback_for {
            let target = steps.iter().find(|s| s.id == *target_id);
            match target {
                None => {
                    return Err(PipelineDefinitionError::ValidationError(format!(
                        "fallback_for references unknown step: {target_id}"
                    )));
                }
                Some(t) if t.on_failure != StepFailurePolicy::Fallback => {
                    return Err(PipelineDefinitionError::ValidationError(format!(
                        "fallback_for references step '{target_id}' without on_failure=fallback"
                    )));
                }
                _ => {}
            }
        }
    }

    Ok(())
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{GlobalFailurePolicy, StepCondition, StepId};

    /// Cree un step simple pour les tests.
    fn step(id: &str, depends_on: &[&str]) -> PipelineStepDef {
        PipelineStepDef {
            id: StepId(id.into()),
            agent: format!("{id}-agent"),
            input: "input".into(),
            depends_on: depends_on.iter().map(|s| StepId(s.to_string())).collect(),
            on_failure: StepFailurePolicy::Fail,
            condition: None,
            fallback_for: None,
        }
    }

    /// Cree une definition de pipeline de test.
    fn make_def(steps: Vec<PipelineStepDef>) -> PipelineDefinitionRow {
        PipelineDefinitionRow {
            id: "test-pipeline".to_string(),
            description: "Test pipeline".to_string(),
            on_failure: GlobalFailurePolicy::Fail,
            steps,
            enabled: true,
            created_at: String::new(),
            updated_at: String::new(),
        }
    }

    // AC-10 — pipeline vide
    #[test]
    fn test_validation_empty_steps() {
        // GIVEN un pipeline sans steps
        let def = make_def(vec![]);

        // WHEN validation
        let result = validate_pipeline(&def);

        // THEN erreur "pipeline must have at least one step"
        assert!(
            matches!(
                result,
                Err(PipelineDefinitionError::ValidationError(ref msg))
                    if msg == "pipeline must have at least one step"
            ),
            "expected empty steps error, got: {result:?}"
        );
    }

    // AC-7 — step_id duplique
    #[test]
    fn test_validation_duplicate_step_id() {
        // GIVEN deux steps avec le meme id "etape-1"
        let def = make_def(vec![step("etape-1", &[]), step("etape-1", &[])]);

        // WHEN validation
        let result = validate_pipeline(&def);

        // THEN erreur "duplicate step id: etape-1"
        assert!(
            matches!(
                result,
                Err(PipelineDefinitionError::ValidationError(ref msg))
                    if msg == "duplicate step id: etape-1"
            ),
            "expected duplicate step id error, got: {result:?}"
        );
    }

    // AC-6 — cycle detecte
    #[test]
    fn test_validation_cycle() {
        // GIVEN steps A->B->A (cycle)
        let def = make_def(vec![step("A", &["B"]), step("B", &["A"])]);

        // WHEN validation
        let result = validate_pipeline(&def);

        // THEN erreur "cycle detected in pipeline DAG"
        assert!(
            matches!(
                result,
                Err(PipelineDefinitionError::ValidationError(ref msg))
                    if msg == "cycle detected in pipeline DAG"
            ),
            "expected cycle error, got: {result:?}"
        );
    }

    // AC-8 — depends_on reference invalide
    #[test]
    fn test_validation_invalid_depends_on() {
        // GIVEN un step avec depends_on="step-inexistant"
        let def = make_def(vec![step("A", &["step-inexistant"])]);

        // WHEN validation
        let result = validate_pipeline(&def);

        // THEN erreur "depends_on references unknown step: step-inexistant"
        assert!(
            matches!(
                result,
                Err(PipelineDefinitionError::ValidationError(ref msg))
                    if msg == "depends_on references unknown step: step-inexistant"
            ),
            "expected unknown depends_on error, got: {result:?}"
        );
    }

    // AC-9 — fallback_for invalide (step cible sans on_failure=fallback)
    #[test]
    fn test_validation_invalid_fallback_for() {
        // GIVEN un step avec fallback_for="etape-1" mais etape-1 a on_failure=fail
        let mut fallback = step("fallback-1", &[]);
        fallback.fallback_for = Some(StepId("etape-1".into()));
        let def = make_def(vec![step("etape-1", &[]), fallback]);

        // WHEN validation
        let result = validate_pipeline(&def);

        // THEN erreur contenant "fallback_for references step without on_failure=fallback"
        assert!(
            matches!(
                result,
                Err(PipelineDefinitionError::ValidationError(ref msg))
                    if msg.contains("fallback_for references step") && msg.contains("without on_failure=fallback")
            ),
            "expected fallback_for validation error, got: {result:?}"
        );
    }

    // AC-9 — fallback_for valide (step cible avec on_failure=fallback)
    #[test]
    fn test_validation_valid_fallback_for() {
        // GIVEN etape-1 avec on_failure=Fallback, et fallback-1 avec fallback_for=etape-1
        let mut primary = step("etape-1", &[]);
        primary.on_failure = StepFailurePolicy::Fallback;

        let mut fallback = step("fallback-1", &[]);
        fallback.fallback_for = Some(StepId("etape-1".into()));

        let def = make_def(vec![primary, fallback]);

        // WHEN validation
        let result = validate_pipeline(&def);

        // THEN pas d'erreur
        assert!(result.is_ok(), "valid fallback_for should pass: {result:?}");
    }

    // AC-9 — fallback_for reference un step inexistant
    #[test]
    fn test_validation_fallback_for_unknown_step() {
        // GIVEN un step avec fallback_for="inexistant"
        let mut fallback = step("fallback-1", &[]);
        fallback.fallback_for = Some(StepId("inexistant".into()));
        let def = make_def(vec![step("A", &[]), fallback]);

        // WHEN validation
        let result = validate_pipeline(&def);

        // THEN erreur "fallback_for references unknown step: inexistant"
        assert!(
            matches!(
                result,
                Err(PipelineDefinitionError::ValidationError(ref msg))
                    if msg.contains("fallback_for references unknown step")
            ),
            "expected unknown fallback_for error, got: {result:?}"
        );
    }

    // Pipeline valide (3 steps, DAG acyclique, condition, fallback)
    #[test]
    fn test_validation_valid_pipeline() {
        // GIVEN un pipeline valide A -> B -> C
        let steps = vec![
            step("A", &[]),
            PipelineStepDef {
                id: StepId("B".into()),
                agent: "B-agent".into(),
                input: "{{steps.A.output}}".into(),
                depends_on: vec![StepId("A".into())],
                on_failure: StepFailurePolicy::Fallback,
                condition: Some(StepCondition {
                    when: crate::types::ConditionKind::Contains,
                    field: "steps.A.output".into(),
                    value: "OK".into(),
                }),
                fallback_for: None,
            },
            PipelineStepDef {
                id: StepId("B-fallback".into()),
                agent: "B-fallback-agent".into(),
                input: "fallback input".into(),
                depends_on: vec![StepId("A".into())],
                on_failure: StepFailurePolicy::Fail,
                condition: None,
                fallback_for: Some(StepId("B".into())),
            },
        ];

        let def = make_def(steps);
        let result = validate_pipeline(&def);
        assert!(result.is_ok(), "valid pipeline should pass: {result:?}");
    }
}
