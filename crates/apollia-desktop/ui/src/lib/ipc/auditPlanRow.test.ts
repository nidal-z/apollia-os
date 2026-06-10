import { describe, it, expect } from "vitest";

import {
  mapAuditRow,
  computePlanFieldDiff,
  planMutationIconKey,
} from "./auditPlanRow";
import type { PlanMutationEntry } from "./audit";
import type { PlanMutationKind, PlanStep, StepProvenance } from "./plan";

const PROVENANCE: StepProvenance = { origin: "initial", reason: null, at: 0 };

function step(overrides: Partial<PlanStep> = {}): PlanStep {
  return {
    step_id: "s1",
    title: "Fetch data",
    description: "Pull the dataset",
    status: "pending",
    depends_on: [],
    tool_hint: null,
    model_hint: null,
    rationale: null,
    provenance: PROVENANCE,
    ...overrides,
  };
}

function entry(overrides: Partial<PlanMutationEntry> = {}): PlanMutationEntry {
  return {
    type: "plan_mutation",
    run_id: "run-1",
    session_id: "sess-1",
    ordinal: 0,
    kind: "modify_step",
    step_id: "s1",
    reason: null,
    before: null,
    after: null,
    revision: 1,
    ts: "2026-06-10T10:00:00Z",
    ...overrides,
  };
}

describe("mapAuditRow", () => {
  it("maps a plan_mutation raw entry to a plan_mutation row", () => {
    // GIVEN a raw entry tagged plan_mutation with kind modify_step
    const raw = entry({ kind: "modify_step", ordinal: 3 });

    // WHEN mapAuditRow is called
    const row = mapAuditRow(raw);

    // THEN the row is typed plan_mutation and carries the entry
    expect(row.type).toBe("plan_mutation");
    if (row.type === "plan_mutation") {
      expect(row.entry.kind).toBe("modify_step");
      expect(row.entry.ordinal).toBe(3);
    }
  });

  it("falls back to tool for a classic tool audit entry", () => {
    // GIVEN a tool audit entry (no plan_mutation discriminator)
    const raw = { id: "t1", tool_name: "shell", agent_name: "scribe" };

    // WHEN mapAuditRow is called
    const row = mapAuditRow(raw);

    // THEN the row is typed tool and preserves the entry
    expect(row.type).toBe("tool");
    if (row.type === "tool") {
      expect(row.entry.id).toBe("t1");
    }
  });

  it("falls back to tool when the discriminator is malformed", () => {
    // GIVEN a partial entry claiming plan_mutation but missing kind/ordinal
    const raw = { type: "plan_mutation" };

    // WHEN mapAuditRow is called
    const row = mapAuditRow(raw);

    // THEN it does not masquerade as a plan mutation
    expect(row.type).toBe("tool");
  });
});

describe("computePlanFieldDiff", () => {
  it("marks every present field added when before is null (Propose)", () => {
    // GIVEN a Propose entry, before null, after with title and description
    const e = entry({
      kind: "propose",
      step_id: null,
      before: null,
      after: step({ title: "Plan A", description: "Do the thing" }),
    });

    // WHEN computePlanFieldDiff is called
    const diff = computePlanFieldDiff(e);

    // THEN all present fields are added and none are removed
    expect(diff.length).toBeGreaterThan(0);
    expect(diff.every((d) => d.status === "added")).toBe(true);
    expect(diff.some((d) => d.status === "removed")).toBe(false);
    expect(diff.find((d) => d.field === "title")?.after).toBe("Plan A");
  });

  it("marks only the changed fields as modified", () => {
    // GIVEN before and after differing only on title
    const e = entry({
      kind: "modify_step",
      before: step({ title: "Old" }),
      after: step({ title: "New" }),
    });

    // WHEN computePlanFieldDiff is called
    const diff = computePlanFieldDiff(e);

    // THEN a single modified diff on title, unchanged fields dropped
    expect(diff).toHaveLength(1);
    expect(diff[0].field).toBe("title");
    expect(diff[0].status).toBe("modified");
    expect(diff[0].before).toBe("Old");
    expect(diff[0].after).toBe("New");
  });

  it("marks every field removed when after is null (RemoveStep)", () => {
    // GIVEN a RemoveStep entry, before set, after null
    const e = entry({
      kind: "remove_step",
      before: step({ title: "Gone", description: "drop me" }),
      after: null,
    });

    // WHEN computePlanFieldDiff is called
    const diff = computePlanFieldDiff(e);

    // THEN all present fields are removed
    expect(diff.length).toBeGreaterThan(0);
    expect(diff.every((d) => d.status === "removed")).toBe(true);
  });

  it("returns an empty diff for a whole-plan mutation without step (Submit)", () => {
    // GIVEN a Submit entry, step_id null, before and after null
    const e = entry({ kind: "submit", step_id: null, before: null, after: null });

    // WHEN computePlanFieldDiff is called
    const diff = computePlanFieldDiff(e);

    // THEN no field diff is produced
    expect(diff).toEqual([]);
  });
});

describe("planMutationIconKey", () => {
  it("returns a distinct non-empty icon key for each kind", () => {
    // GIVEN the nine plan mutation kinds
    const kinds: PlanMutationKind[] = [
      "propose",
      "add_step",
      "modify_step",
      "remove_step",
      "reorder",
      "status_change",
      "submit",
      "approve",
      "reject",
    ];

    // WHEN planMutationIconKey is called for each
    const keys = kinds.map(planMutationIconKey);

    // THEN every key is non-empty and approve/reject differ
    expect(keys.every((k) => k.length > 0)).toBe(true);
    expect(new Set(keys).size).toBe(kinds.length);
    expect(planMutationIconKey("approve")).not.toBe(planMutationIconKey("reject"));
  });
});
