import { vi, describe, test, expect, beforeEach } from "vitest";

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(),
}));

import { get } from "svelte/store";
import { chatPlanState, resetChatPlan, dispatchChatPlan } from "./chatPlanMode";

function planUpdatedEnvelope(
  sessionId: string,
  mutation: Record<string, unknown> | undefined,
) {
  return {
    category: "plan-mode",
    event_type: "PlanUpdated",
    payload: {
      PlanUpdated: {
        session_id: sessionId,
        plan: {
          plan_id: "p1",
          scope: { Chat: { session_id: sessionId } },
          revision: 1,
          status: "executing",
          steps: [],
        },
        mutation,
      },
    },
  };
}

beforeEach(() => {
  resetChatPlan();
});

describe("chatPlanMode lastMutation tracking", () => {
  test("PlanUpdated records the last mutation", () => {
    // GIVEN an empty store
    // WHEN a PlanUpdated lands with an add_step mutation for the active session
    dispatchChatPlan(
      planUpdatedEnvelope("s-1", {
        kind: "add_step",
        step_id: "s1",
        reason: "new dependency",
      }),
      "s-1",
    );
    // THEN the store captures the mutation kind, step and reason
    const s = get(chatPlanState);
    expect(s.lastMutation?.kind).toBe("add_step");
    expect(s.lastMutation?.step_id).toBe("s1");
    expect(s.lastMutation?.reason).toBe("new dependency");
    expect(s.plan?.plan_id).toBe("p1");
  });

  test("a malformed mutation leaves the store uncorrupted", () => {
    // GIVEN a PlanUpdated whose mutation is malformed (error case)
    // WHEN dispatched
    expect(() =>
      dispatchChatPlan(planUpdatedEnvelope("s-1", undefined), "s-1"),
    ).not.toThrow();
    // THEN the plan still updates but lastMutation stays null
    const s = get(chatPlanState);
    expect(s.plan?.plan_id).toBe("p1");
    expect(s.lastMutation).toBeNull();
  });

  test("a mutation for another session is ignored", () => {
    // GIVEN the active session is s-1
    // WHEN a PlanUpdated lands for s-2
    dispatchChatPlan(
      planUpdatedEnvelope("s-2", { kind: "remove_step", step_id: "s9" }),
      "s-1",
    );
    // THEN nothing is recorded (no cross-session leak)
    expect(get(chatPlanState).lastMutation).toBeNull();
    expect(get(chatPlanState).plan).toBeNull();
  });
});
