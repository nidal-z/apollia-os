import { describe, test, expect } from "vitest";
import { reconstructPlanAt, revisionCount } from "./reconstructPlan";
import type { PlanMutation, PlanStep } from "$lib/ipc/plan";

function step(id: string): PlanStep {
  return {
    step_id: id,
    title: id,
    description: id,
    status: "pending",
    depends_on: [],
    tool_hint: null,
    model_hint: null,
    rationale: null,
    provenance: { origin: "agent_edit", reason: null, at: 0 },
  };
}

const mutations: PlanMutation[] = [
  { kind: "add_step", step_id: "s1", reason: null, before: null, after: step("s1"), at: 1 },
  { kind: "add_step", step_id: "s2", reason: null, before: null, after: step("s2"), at: 2 },
  {
    kind: "modify_step",
    step_id: "s2",
    reason: "clarif",
    before: step("s2"),
    after: { ...step("s2"), title: "s2 revised" },
    at: 3,
  },
  { kind: "remove_step", step_id: "s1", reason: "doublon", before: step("s1"), after: null, at: 4 },
];

describe("reconstructPlanAt", () => {
  test("rebuilds the state at an intermediate revision", () => {
    // GIVEN the ordered mutation list
    // WHEN reconstructing at revision 2 (after add s1, add s2)
    const result = reconstructPlanAt(mutations, 2);
    // THEN the plan holds s1 and s2 with no skipped entries
    const ids = result.steps.map((s) => s.step_id).sort();
    expect(ids).toEqual(["s1", "s2"]);
    expect(result.skipped).toHaveLength(0);
  });

  test("applies a modify through the after image", () => {
    // GIVEN the list including the modify at revision 3
    // WHEN reconstructing at revision 3
    const result = reconstructPlanAt(mutations, 3);
    // THEN s2 carries its revised title
    const s2 = result.steps.find((s) => s.step_id === "s2");
    expect(s2?.title).toBe("s2 revised");
  });

  test("applies a remove at the final revision", () => {
    // GIVEN the complete list
    // WHEN reconstructing at the final revision
    const result = reconstructPlanAt(mutations, mutations.length);
    // THEN s1 has been removed and only s2 remains
    expect(result.steps.map((s) => s.step_id)).toEqual(["s2"]);
  });

  test("revision 0 yields the empty plan", () => {
    // GIVEN any mutation list
    // WHEN reconstructing at revision 0
    const result = reconstructPlanAt(mutations, 0);
    // THEN the plan is empty
    expect(result.steps).toHaveLength(0);
  });

  test("a revision beyond the list applies every mutation", () => {
    // GIVEN the complete list
    // WHEN reconstructing past the end
    const result = reconstructPlanAt(mutations, 999);
    // THEN it equals the final state
    expect(result.steps.map((s) => s.step_id)).toEqual(["s2"]);
  });

  test("scrubbing to the latest matches the live final state", () => {
    // GIVEN the complete list
    // WHEN reconstructing at the last revision
    const latest = reconstructPlanAt(mutations, revisionCount(mutations));
    // THEN it equals applying all mutations
    const all = reconstructPlanAt(mutations, mutations.length);
    expect(latest.steps.map((s) => s.step_id)).toEqual(
      all.steps.map((s) => s.step_id),
    );
  });

  test("structural markers carry no step state and are not corrupt", () => {
    // GIVEN a propose marker (no step payload) followed by an add
    const withMarkers: PlanMutation[] = [
      { kind: "propose", step_id: null, reason: "draft", before: null, after: null, at: 1 },
      { kind: "add_step", step_id: "s1", reason: null, before: null, after: step("s1"), at: 2 },
    ];
    // WHEN reconstructing the whole list
    const result = reconstructPlanAt(withMarkers, withMarkers.length);
    // THEN the propose is not flagged and s1 lands from the add
    expect(result.skipped).toHaveLength(0);
    expect(result.steps.map((s) => s.step_id)).toEqual(["s1"]);
  });

  test("skips a corrupt entry with a marker and never throws (error case)", () => {
    // GIVEN a list with an add_step missing its after image
    const corrupt: PlanMutation[] = [
      { kind: "add_step", step_id: "s1", reason: null, before: null, after: null, at: 1 },
      mutations[1],
    ];
    // WHEN reconstructing the full list
    const result = reconstructPlanAt(corrupt, corrupt.length);
    // THEN the corrupt entry is skipped and reported, s2 still lands
    expect(result.skipped).toHaveLength(1);
    expect(result.skipped[0].revision).toBe(1);
    expect(result.steps.map((s) => s.step_id)).toEqual(["s2"]);
  });

  test("flags a remove_step missing its step id (error case)", () => {
    // GIVEN a remove_step with a null step_id
    const corrupt: PlanMutation[] = [
      mutations[0],
      { kind: "remove_step", step_id: null, reason: null, before: null, after: null, at: 2 },
    ];
    // WHEN reconstructing
    const result = reconstructPlanAt(corrupt, corrupt.length);
    // THEN the corrupt remove is skipped, s1 survives
    expect(result.skipped).toHaveLength(1);
    expect(result.skipped[0].revision).toBe(2);
    expect(result.steps.map((s) => s.step_id)).toEqual(["s1"]);
  });
});

describe("revisionCount", () => {
  test("counts one revision per mutation", () => {
    // GIVEN the mutation list WHEN counting THEN it matches the length
    expect(revisionCount(mutations)).toBe(4);
    expect(revisionCount([])).toBe(0);
  });
});
