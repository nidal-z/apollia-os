import { describe, test, expect } from "vitest";
import { computeDiff, resolveDependencyLabels } from "./planDiff";
import type { ProposedPlan, PlanStep } from "$lib/ipc/planMode";

function step(id: string, description: string, depends_on: string[] = []): PlanStep {
  return { step_id: id, description, depends_on, tool_hint: null, model_hint: null };
}

function plan(steps: PlanStep[]): ProposedPlan {
  return {
    run_id: "run-1",
    plan_id: "plan-1",
    task_id: "run-1",
    step_count: steps.length,
    steps,
    ttl_secs: 120,
  };
}

describe("computeDiff", () => {
  // GIVEN no previous plan WHEN diffed THEN every step is unchanged
  test("first plan has no diff badges", () => {
    // GIVEN / WHEN
    const result = computeDiff(plan([step("s1", "a"), step("s2", "b")]), null);
    // THEN
    expect(result.map((r) => r.diffStatus)).toEqual(["unchanged", "unchanged"]);
  });

  // GIVEN a replan WHEN a step is modified and one added THEN flagged accordingly
  test("modified and added steps are flagged", () => {
    // GIVEN
    const previous = plan([step("s1", "a"), step("s2", "b")]);
    const current = plan([step("s1", "a"), step("s2", "b-edited"), step("s3", "c")]);
    // WHEN
    const result = computeDiff(current, previous);
    // THEN
    expect(result.find((r) => r.step.step_id === "s1")?.diffStatus).toBe("unchanged");
    expect(result.find((r) => r.step.step_id === "s2")?.diffStatus).toBe("modified");
    expect(result.find((r) => r.step.step_id === "s3")?.diffStatus).toBe("added");
  });

  // GIVEN a replan dropping a step WHEN diffed THEN it is appended as removed
  test("removed step is appended with removed status", () => {
    // GIVEN
    const previous = plan([step("s1", "a"), step("s2", "b"), step("s3", "c")]);
    const current = plan([step("s1", "a"), step("s3", "c")]);
    // WHEN
    const result = computeDiff(current, previous);
    // THEN
    const removed = result.filter((r) => r.diffStatus === "removed");
    expect(removed).toHaveLength(1);
    expect(removed[0]?.step.step_id).toBe("s2");
  });

  // GIVEN no current plan WHEN diffed THEN the result is empty
  test("null current plan yields an empty diff", () => {
    // GIVEN / WHEN / THEN
    expect(computeDiff(null, plan([step("s1", "a")]))).toEqual([]);
  });
});

describe("resolveDependencyLabels", () => {
  // GIVEN dependency ids WHEN resolved THEN descriptions are returned
  test("resolves ids to descriptions and falls back to the id", () => {
    // GIVEN
    const steps = [step("s1", "first"), step("s2", "second")];
    // WHEN
    const labels = resolveDependencyLabels(["s1", "missing"], steps);
    // THEN
    expect(labels).toEqual(["first", "missing"]);
  });
});
