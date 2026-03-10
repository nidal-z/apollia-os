//! Topological layer computation for pipeline step scheduling.
//!
//! Provides [`topological_layers`], which groups pipeline steps into parallel
//! execution layers using Kahn's BFS algorithm. Steps in the same layer have
//! no inter-dependencies and can be submitted concurrently by the executor.

use crate::types::{PipelineStepDef, StepId};
use std::collections::{HashMap, HashSet, VecDeque};
use thiserror::Error;

// ── Error type ────────────────────────────────────────────────────────────────

/// Errors produced during topological validation of a pipeline's dependency graph.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum TopologicalError {
    /// A cycle was detected in the dependency graph; the pipeline cannot be scheduled.
    #[error("cycle detected involving step '{0}'")]
    CycleDetected(String),

    /// A step declares a dependency on a step identifier that does not exist.
    #[error("step '{dependent}' depends on unknown step '{dependency}'")]
    UnknownDependency {
        /// The step that declared the invalid dependency.
        dependent: String,
        /// The non-existent step that was referenced.
        dependency: String,
    },
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Organises pipeline steps into parallel execution layers using Kahn's BFS algorithm.
///
/// **Layer 0** contains every step with no dependencies.
/// **Layer N** contains every step whose direct dependencies all appear in layers `0..N-1`.
///
/// Steps in the same layer are independent and may be submitted concurrently by
/// the `PipelineExecutor` via `FuturesUnordered`.
///
/// Fallback steps are placed in the layer determined by their own `depends_on` list;
/// the `PipelineExecutor` is responsible for filtering inactive fallbacks at runtime.
///
/// # Errors
///
/// Returns [`TopologicalError::UnknownDependency`] if any step references a
/// non-existent step in its `depends_on`.
///
/// Returns [`TopologicalError::CycleDetected`] if the dependency graph contains
/// a cycle, which would make a valid execution order impossible.
///
/// # Complexity
///
/// O(V + E) where V is the number of steps and E the total number of declared
/// dependencies across all steps.
pub fn topological_layers(steps: &[PipelineStepDef]) -> Result<Vec<Vec<StepId>>, TopologicalError> {
    if steps.is_empty() {
        return Ok(Vec::new());
    }

    // ── AC-5: validate that every dependency references a known step ───────────
    let known: HashSet<&StepId> = steps.iter().map(|s| &s.id).collect();
    for step in steps {
        for dep in &step.depends_on {
            if !known.contains(dep) {
                return Err(TopologicalError::UnknownDependency {
                    dependent: step.id.0.clone(),
                    dependency: dep.0.clone(),
                });
            }
        }
    }

    // ── Build in-degree map and successor adjacency list ──────────────────────
    // in_degree[S] = number of dependencies S still has (decremented during Kahn)
    let mut in_degree: HashMap<StepId, usize> = steps.iter().map(|s| (s.id.clone(), 0)).collect();

    // successors[D] = steps that list D in their depends_on
    let mut successors: HashMap<StepId, Vec<StepId>> =
        steps.iter().map(|s| (s.id.clone(), Vec::new())).collect();

    for step in steps {
        for dep in &step.depends_on {
            if let Some(count) = in_degree.get_mut(&step.id) {
                *count += 1;
            }
            if let Some(succ_list) = successors.get_mut(dep) {
                succ_list.push(step.id.clone());
            }
        }
    }

    // ── Kahn's BFS: build one layer per iteration ─────────────────────────────
    // Initial layer: all steps with in_degree == 0, preserved in definition order.
    let mut queue: VecDeque<StepId> = steps
        .iter()
        .filter(|s| in_degree.get(&s.id).copied().unwrap_or(0) == 0)
        .map(|s| s.id.clone())
        .collect();

    let mut layers: Vec<Vec<StepId>> = Vec::new();
    let mut processed: usize = 0;

    while !queue.is_empty() {
        // All nodes currently in the queue form one parallel layer.
        let layer: Vec<StepId> = queue.drain(..).collect();
        processed += layer.len();

        // Decrement in-degree for every successor of each node in this layer.
        // Collect next-layer candidates after processing to avoid borrow conflicts.
        let mut next: Vec<StepId> = Vec::new();
        for step_id in &layer {
            if let Some(succ_list) = successors.get(step_id) {
                for succ in succ_list {
                    if let Some(deg) = in_degree.get_mut(succ) {
                        *deg -= 1;
                        if *deg == 0 {
                            next.push(succ.clone());
                        }
                    }
                }
            }
        }

        layers.push(layer);
        queue.extend(next);
    }

    // ── AC-4: any remaining nodes with in_degree > 0 are part of a cycle ─────
    if processed < steps.len() {
        let cycle_step = steps
            .iter()
            .find(|s| in_degree.get(&s.id).copied().unwrap_or(0) > 0)
            .map(|s| s.id.0.clone())
            .unwrap_or_else(|| "unknown".into());
        return Err(TopologicalError::CycleDetected(cycle_step));
    }

    Ok(layers)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{PipelineStepDef, StepFailurePolicy, StepId};

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

    // AC-1 — Sequential pipeline (A → B → C) produces 3 single-step layers.
    #[test]
    fn test_ac1_sequential_abc() {
        // GIVEN
        let steps = vec![step("A", &[]), step("B", &["A"]), step("C", &["B"])];
        // WHEN
        let layers = topological_layers(&steps).unwrap();
        // THEN
        assert_eq!(layers.len(), 3);
        assert_eq!(layers[0], vec![StepId("A".into())]);
        assert_eq!(layers[1], vec![StepId("B".into())]);
        assert_eq!(layers[2], vec![StepId("C".into())]);
    }

    // AC-2 — Fan-out: B and C both depend on A, so layer 1 contains both.
    #[test]
    fn test_ac2_fan_out() {
        // GIVEN
        let steps = vec![step("A", &[]), step("B", &["A"]), step("C", &["A"])];
        // WHEN
        let layers = topological_layers(&steps).unwrap();
        // THEN
        assert_eq!(layers.len(), 2);
        assert_eq!(layers[0], vec![StepId("A".into())]);
        let layer1: std::collections::HashSet<StepId> = layers[1].iter().cloned().collect();
        assert!(layer1.contains(&StepId("B".into())));
        assert!(layer1.contains(&StepId("C".into())));
    }

    // AC-3 — Fan-in: D waits for both B and C; layer ordering is [A], [B, C], [D].
    #[test]
    fn test_ac3_fan_in() {
        // GIVEN
        let steps = vec![
            step("A", &[]),
            step("B", &["A"]),
            step("C", &["A"]),
            step("D", &["B", "C"]),
        ];
        // WHEN
        let layers = topological_layers(&steps).unwrap();
        // THEN
        assert_eq!(layers.len(), 3);
        assert_eq!(layers[2], vec![StepId("D".into())]);
    }

    // AC-4 — Cycle A ↔ B is detected and reported.
    #[test]
    fn test_ac4_cycle_detected() {
        // GIVEN
        let steps = vec![step("A", &["B"]), step("B", &["A"])];
        // WHEN
        let result = topological_layers(&steps);
        // THEN
        assert!(matches!(result, Err(TopologicalError::CycleDetected(_))));
    }

    // AC-5 — A dependency on a non-existent step returns UnknownDependency.
    #[test]
    fn test_ac5_unknown_dependency() {
        // GIVEN
        let steps = vec![step("A", &["inexistant"])];
        // WHEN
        let result = topological_layers(&steps);
        // THEN
        assert!(matches!(
            result,
            Err(TopologicalError::UnknownDependency { .. })
        ));
    }

    // AC-6 — Fallback step is placed in its natural layer (same as the primary step).
    #[test]
    fn test_ac6_fallback_in_natural_layer() {
        // GIVEN — A[], B([A]), Fallback-B([A], fallback_for=Some(B))
        let fallback_b = PipelineStepDef {
            id: StepId("fallback-b".into()),
            agent: "fallback-b-agent".into(),
            input: "input".into(),
            depends_on: vec![StepId("A".into())],
            on_failure: StepFailurePolicy::Fail,
            condition: None,
            fallback_for: Some(StepId("B".into())),
        };
        let steps = vec![step("A", &[]), step("B", &["A"]), fallback_b];
        // WHEN
        let layers = topological_layers(&steps).unwrap();
        // THEN — Fallback-B and B share layer 1 (both depend only on A)
        assert_eq!(layers.len(), 2);
        assert_eq!(layers[0], vec![StepId("A".into())]);
        let layer1: std::collections::HashSet<StepId> = layers[1].iter().cloned().collect();
        assert!(layer1.contains(&StepId("B".into())));
        assert!(layer1.contains(&StepId("fallback-b".into())));
    }

    // Edge case — empty slice produces no layers.
    #[test]
    fn test_empty_pipeline_returns_empty_layers() {
        // GIVEN / WHEN
        let layers = topological_layers(&[]).unwrap();
        // THEN
        assert!(layers.is_empty());
    }

    // Single step with no dependencies → one layer.
    #[test]
    fn test_single_step_no_deps() {
        // GIVEN
        let steps = vec![step("only", &[])];
        // WHEN
        let layers = topological_layers(&steps).unwrap();
        // THEN
        assert_eq!(layers.len(), 1);
        assert_eq!(layers[0], vec![StepId("only".into())]);
    }
}
