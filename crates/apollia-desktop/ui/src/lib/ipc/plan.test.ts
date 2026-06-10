import { describe, it, expect, expectTypeOf } from "vitest";
import { toStepStatus } from "./plan";
import type { PlanModePayload, StepStatus } from "./plan";

describe("plan ipc types", () => {
  it("StepStatus covers the five runtime statuses", () => {
    // GIVEN the StepStatus union
    // WHEN we enumerate it
    // THEN it matches the apollia_core::plan::StepStatus contract
    expectTypeOf<StepStatus>().toEqualTypeOf<
      "pending" | "in_progress" | "completed" | "skipped" | "failed"
    >();
  });

  it("PlanModePayload is externally tagged", () => {
    // GIVEN a PlanUpdated payload shape
    const sample: PlanModePayload = {
      PlanUpdated: {
        session_id: "s",
        plan: {
          plan_id: "p",
          revision: 0,
          status: "draft",
          steps: [],
        },
        mutation: {
          kind: "add_step",
          step_id: null,
          reason: null,
          before: null,
          after: null,
          at: 0,
        },
      },
    };
    // THEN the variant key is present
    expectTypeOf(sample).toMatchTypeOf<PlanModePayload>();
  });
});

describe("toStepStatus", () => {
  it("passes through every known status", () => {
    // GIVEN each wire status string
    const all = ["pending", "in_progress", "completed", "skipped", "failed"];
    // WHEN narrowed
    // THEN it is returned unchanged
    for (const s of all) {
      expect(toStepStatus(s)).toBe(s);
    }
  });

  it("falls back to pending on an unknown status", () => {
    // GIVEN an unexpected status from the wire (error case)
    // WHEN narrowed
    // THEN it defaults to pending so the DAG never renders an undefined status
    expect(toStepStatus("garbage")).toBe("pending");
    expect(toStepStatus("")).toBe("pending");
  });
});
