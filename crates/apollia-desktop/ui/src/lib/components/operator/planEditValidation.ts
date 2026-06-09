// Pure, synchronous validation for the plan edit flow (fail fast, principle #4).
//
// Extracted from the component so the rules can be unit-tested without a DOM.
// The same DAG invariants are re-checked by the engine before execution.

import type { PlanStep } from "$lib/ipc/planMode";

/** A single validation problem, optionally scoped to a step. */
export interface ValidationError {
  stepId?: string;
  /** i18n key of the message to display. */
  message: string;
  kind: "empty_label" | "cyclic_dependency";
}

/** Detects a cycle in the step dependency graph via DFS. */
export function hasCycle(steps: PlanStep[]): boolean {
  const adjacency = new Map(steps.map((s) => [s.step_id, s.depends_on]));
  const visited = new Set<string>();
  const inStack = new Set<string>();

  function visit(id: string): boolean {
    if (inStack.has(id)) return true;
    if (visited.has(id)) return false;
    visited.add(id);
    inStack.add(id);
    for (const dep of adjacency.get(id) ?? []) {
      if (visit(dep)) return true;
    }
    inStack.delete(id);
    return false;
  }

  return steps.some((s) => visit(s.step_id));
}

/**
 * Validates the edited steps: no empty label, no cyclic dependency.
 *
 * Returns an empty array when the plan is submittable.
 */
export function validate(steps: PlanStep[]): ValidationError[] {
  const errors: ValidationError[] = [];

  for (const step of steps) {
    if (step.description.trim().length === 0) {
      errors.push({
        stepId: step.step_id,
        message: "plan_mode.error_empty_step_label",
        kind: "empty_label",
      });
    }
  }

  if (hasCycle(steps)) {
    errors.push({
      message: "plan_mode.error_cyclic_dependency",
      kind: "cyclic_dependency",
    });
  }

  return errors;
}
