import { describe, test, expect } from "vitest";
import { validate, hasCycle } from "./planEditValidation";
import type { PlanStep } from "$lib/ipc/planMode";

function step(id: string, description: string, depends_on: string[] = []): PlanStep {
  return { step_id: id, description, depends_on, tool_hint: null, model_hint: null };
}

describe("hasCycle", () => {
  // GIVEN an acyclic chain WHEN checked THEN no cycle
  test("returns false for an acyclic plan", () => {
    expect(hasCycle([step("s1", "a"), step("s2", "b", ["s1"])])).toBe(false);
  });

  // GIVEN a direct cycle A->B->A WHEN checked THEN cycle detected
  test("detects a direct cycle", () => {
    expect(hasCycle([step("s1", "a", ["s2"]), step("s2", "b", ["s1"])])).toBe(true);
  });

  // GIVEN a self-dependency WHEN checked THEN cycle detected
  test("detects a self-dependency", () => {
    expect(hasCycle([step("s1", "a", ["s1"])])).toBe(true);
  });
});

describe("validate", () => {
  // GIVEN a valid plan WHEN validated THEN no errors
  test("accepts a valid plan", () => {
    expect(validate([step("s1", "a"), step("s2", "b", ["s1"])])).toEqual([]);
  });

  // GIVEN an empty label WHEN validated THEN an empty_label error scoped to the step
  test("flags an empty step label", () => {
    const errors = validate([step("s1", "   "), step("s2", "b")]);
    expect(errors).toHaveLength(1);
    expect(errors[0]).toMatchObject({ kind: "empty_label", stepId: "s1" });
  });

  // GIVEN a cycle WHEN validated THEN a global cyclic_dependency error
  test("flags a cyclic dependency", () => {
    const errors = validate([step("s1", "a", ["s2"]), step("s2", "b", ["s1"])]);
    expect(errors.some((e) => e.kind === "cyclic_dependency")).toBe(true);
  });
});
