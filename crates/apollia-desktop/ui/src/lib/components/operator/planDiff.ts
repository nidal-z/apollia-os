// Pure diff helpers for the Builder plan review.
//
// Extracted from the component so the diff logic can be unit-tested without a
// DOM. Compares the current plan against the previous one (kept by the store
// across a replan) and classifies each step.

import type { ProposedPlan, PlanStep } from "$lib/ipc/planMode";

/** Classification of a step relative to the previous plan. */
export type StepDiffStatus = "added" | "removed" | "modified" | "unchanged";

/** A plan step paired with its diff classification. */
export interface StepWithDiff {
  step: PlanStep;
  diffStatus: StepDiffStatus;
}

/**
 * Classifies each step of `current` against `previous`, keyed by `step_id`.
 *
 * Steps absent from `previous` are `added`, steps whose content changed are
 * `modified`, and steps present in `previous` but missing from `current` are
 * appended as `removed`. With no `previous`, every step is `unchanged`.
 */
export function computeDiff(
  current: ProposedPlan | null,
  previous: ProposedPlan | null,
): StepWithDiff[] {
  if (!current) return [];
  if (!previous) {
    return current.steps.map((step) => ({ step, diffStatus: "unchanged" }));
  }

  const prevById = new Map(previous.steps.map((s) => [s.step_id, s]));
  const result: StepWithDiff[] = current.steps.map((step) => {
    const prev = prevById.get(step.step_id);
    if (!prev) return { step, diffStatus: "added" };
    if (JSON.stringify(step) !== JSON.stringify(prev)) {
      return { step, diffStatus: "modified" };
    }
    return { step, diffStatus: "unchanged" };
  });

  const currentIds = new Set(current.steps.map((s) => s.step_id));
  for (const prev of previous.steps) {
    if (!currentIds.has(prev.step_id)) {
      result.push({ step: prev, diffStatus: "removed" });
    }
  }
  return result;
}

/**
 * Resolves dependency ids to their step descriptions within `steps`.
 *
 * Falls back to the raw id when no matching step exists (e.g. a dependency on a
 * removed step).
 */
export function resolveDependencyLabels(
  dependsOn: string[],
  steps: PlanStep[],
): string[] {
  const byId = new Map(steps.map((s) => [s.step_id, s.description]));
  return dependsOn.map((id) => byId.get(id) ?? id);
}
