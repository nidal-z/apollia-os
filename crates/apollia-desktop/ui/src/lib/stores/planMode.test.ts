import { vi, describe, test, expect, beforeEach } from "vitest";

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(),
}));

import { get } from "svelte/store";
import { planModeState, resetPlanMode, __test } from "./planMode";

const { dispatch } = __test;

function approvalRequired(
  runId: string,
  steps: Array<{ step_id: string; description: string }>,
) {
  return {
    category: "plan-approval",
    event_type: "PlanApprovalRequired",
    payload: {
      PlanApprovalRequired: {
        run_id: runId,
        plan_id: `plan-${runId}`,
        task_id: runId,
        step_count: steps.length,
        steps: steps.map((s) => ({ ...s, depends_on: [] })),
        ttl_secs: 120,
      },
    },
  };
}

beforeEach(() => {
  resetPlanMode();
});

describe("planMode store", () => {
  // GIVEN an idle store WHEN a gate event arrives THEN it becomes pending
  test("PlanApprovalRequired moves the store to pending_approval", () => {
    // GIVEN
    expect(get(planModeState).status).toBe("idle");
    // WHEN
    dispatch(approvalRequired("run-1", [{ step_id: "s1", description: "do x" }]));
    // THEN
    const s = get(planModeState);
    expect(s.status).toBe("pending_approval");
    expect(s.runId).toBe("run-1");
    expect(s.plan?.steps).toHaveLength(1);
  });

  // GIVEN a pending gate WHEN approval for that run arrives THEN approved
  test("PlanApproved for the tracked run transitions to approved", () => {
    // GIVEN
    dispatch(approvalRequired("run-1", [{ step_id: "s1", description: "x" }]));
    // WHEN
    dispatch({
      category: "plan-approval",
      event_type: "PlanApproved",
      payload: { PlanApproved: { run_id: "run-1" } },
    });
    // THEN
    expect(get(planModeState).status).toBe("approved");
  });

  // GIVEN a pending gate WHEN an event for another run arrives THEN ignored
  test("events for an untracked run id are ignored", () => {
    // GIVEN
    dispatch(approvalRequired("run-1", [{ step_id: "s1", description: "x" }]));
    // WHEN
    dispatch({
      category: "plan-approval",
      event_type: "PlanApproved",
      payload: { PlanApproved: { run_id: "run-Z" } },
    });
    // THEN
    expect(get(planModeState).status).toBe("pending_approval");
  });

  // GIVEN a pending plan WHEN a replan re-opens the gate THEN previousPlan is kept
  test("a replan keeps the prior plan as previousPlan", () => {
    // GIVEN
    dispatch(approvalRequired("run-1", [{ step_id: "s1", description: "v1" }]));
    // WHEN
    dispatch(approvalRequired("run-1", [{ step_id: "s1", description: "v2" }]));
    // THEN
    const s = get(planModeState);
    expect(s.plan?.steps[0]?.description).toBe("v2");
    expect(s.previousPlan?.steps[0]?.description).toBe("v1");
  });

  // GIVEN a non plan-approval envelope WHEN dispatched THEN the store is untouched
  test("envelopes from other categories are ignored", () => {
    // GIVEN
    dispatch(approvalRequired("run-1", [{ step_id: "s1", description: "x" }]));
    // WHEN
    dispatch({
      category: "task-changed",
      event_type: "TaskCompleted",
      payload: { TaskCompleted: { task_id: "run-1" } },
    });
    // THEN
    expect(get(planModeState).status).toBe("pending_approval");
  });
});
