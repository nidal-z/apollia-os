import { describe, it, expect } from "vitest";
import { layoutPlan } from "./planDagLayout";
import type { SessionPlan, SessionPlanStep } from "$lib/stores/chatPlanMode";

function step(
  id: string,
  dependsOn: string[],
  status = "pending",
): SessionPlanStep {
  return {
    step_id: id,
    title: `Step ${id}`,
    description: `Do ${id}`,
    status,
    depends_on: dependsOn,
    tool_hint: null,
    model_hint: null,
    rationale: null,
    provenance: { origin: "initial", reason: null, at: 0 },
  };
}

function plan(steps: SessionPlanStep[]): SessionPlan {
  return { plan_id: "p1", revision: 1, status: "executing", steps };
}

describe("layoutPlan", () => {
  it("emits one node per step and one edge per dependency", () => {
    // GIVEN a plan s1 -> s2 -> s3
    const p = plan([step("s1", []), step("s2", ["s1"]), step("s3", ["s2"])]);
    // WHEN laid out
    const { nodes, edges } = layoutPlan(p);
    // THEN three planStep nodes and two directed edges are produced
    expect(nodes).toHaveLength(3);
    expect(nodes.every((n) => n.type === "planStep")).toBe(true);
    expect(edges.map((e) => e.id).sort()).toEqual(["s1->s2", "s2->s3"]);
  });

  it("stacks dependents below their dependencies (top-to-bottom)", () => {
    // GIVEN s2 depends on s1
    const p = plan([step("s1", []), step("s2", ["s1"])]);
    // WHEN laid out
    const { nodes } = layoutPlan(p);
    // THEN s1 sits above s2 on the vertical axis
    const y = new Map(nodes.map((n) => [n.id, n.position.y]));
    expect(y.get("s1")!).toBeLessThan(y.get("s2")!);
  });

  it("animates an edge when its target is in progress", () => {
    // GIVEN s2 (in_progress) depends on s1
    const p = plan([step("s1", [], "completed"), step("s2", ["s1"], "in_progress")]);
    // WHEN laid out
    const { edges } = layoutPlan(p);
    // THEN the dependency edge is animated
    expect(edges.find((e) => e.id === "s1->s2")?.animated).toBe(true);
  });

  it("drops dependencies pointing at unknown steps (error case)", () => {
    // GIVEN s2 depends on a step that is not in the plan
    const p = plan([step("s2", ["ghost"])]);
    // WHEN laid out
    const { nodes, edges } = layoutPlan(p);
    // THEN the node renders without a dangling edge
    expect(nodes).toHaveLength(1);
    expect(edges).toHaveLength(0);
  });
});
