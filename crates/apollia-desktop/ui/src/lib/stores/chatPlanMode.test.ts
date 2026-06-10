import { vi, describe, test, expect, beforeEach } from "vitest";

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(),
}));

import { get } from "svelte/store";
import {
  chatPlanState,
  resetChatPlan,
  dispatchChatPlan,
  setChatPlanError,
} from "./chatPlanMode";

function envelope(eventType: string, inner: Record<string, unknown>) {
  return {
    category: "plan-mode",
    event_type: eventType,
    payload: { [eventType]: inner },
  };
}

function samplePlan(sessionId: string, descriptions: string[]) {
  return {
    plan_id: `plan-${sessionId}`,
    scope: { Chat: { session_id: sessionId } },
    revision: 1,
    status: "submitted",
    steps: descriptions.map((d, i) => ({
      step_id: `s${i + 1}`,
      title: d,
      description: d,
      status: "pending",
      depends_on: [],
      tool_hint: null,
      model_hint: null,
      rationale: null,
      provenance: { origin: "agent", reason: null, at: 0 },
    })),
  };
}

beforeEach(() => {
  resetChatPlan();
});

describe("chatPlanMode session scoping", () => {
  test("PlanSubmitted for the active session opens the gate", () => {
    // GIVEN the listener is scoped to "s-1"
    // WHEN a plan is submitted for "s-1"
    dispatchChatPlan(envelope("PlanSubmitted", {
      session_id: "s-1",
      plan: samplePlan("s-1", ["do x", "do y"]),
    }), "s-1");
    // THEN the store is awaiting approval with the submitted plan
    const s = get(chatPlanState);
    expect(s.status).toBe("pending_approval");
    expect(s.phase).toBe("awaiting_approval");
    expect(s.sessionId).toBe("s-1");
    expect(s.plan?.steps).toHaveLength(2);
  });

  test("an event for another session is ignored", () => {
    // GIVEN the listener is scoped to "s-1"
    // WHEN a plan is submitted for "s-2"
    dispatchChatPlan(envelope("PlanSubmitted", {
      session_id: "s-2",
      plan: samplePlan("s-2", ["other"]),
    }), "s-1");
    // THEN the store stays idle (no cross-session leak)
    expect(get(chatPlanState).status).toBe("idle");
  });

  test("ChatPlanApproved moves the gate to executing", () => {
    // GIVEN a gate awaiting approval for "s-1"
    dispatchChatPlan(envelope("PlanSubmitted", {
      session_id: "s-1",
      plan: samplePlan("s-1", ["do x"]),
    }), "s-1");
    // WHEN approval arrives for "s-1"
    dispatchChatPlan(envelope("ChatPlanApproved", { session_id: "s-1" }), "s-1");
    // THEN the store is approved and the phase is executing
    const s = get(chatPlanState);
    expect(s.status).toBe("approved");
    expect(s.phase).toBe("executing");
  });

  test("ChatPlanRejected marks the gate rejected", () => {
    // GIVEN a gate awaiting approval for "s-1"
    dispatchChatPlan(envelope("PlanSubmitted", {
      session_id: "s-1",
      plan: samplePlan("s-1", ["do x"]),
    }), "s-1");
    // WHEN a rejection arrives for "s-1"
    dispatchChatPlan(envelope("ChatPlanRejected", {
      session_id: "s-1",
      reason: "too risky",
    }), "s-1");
    // THEN the store is rejected
    expect(get(chatPlanState).status).toBe("rejected");
  });

  test("ChatPlanPhaseChanged tracks the phase and closes a stale card", () => {
    // GIVEN a gate awaiting approval for "s-1"
    dispatchChatPlan(envelope("PlanSubmitted", {
      session_id: "s-1",
      plan: samplePlan("s-1", ["do x"]),
    }), "s-1");
    // WHEN the phase moves to drafting (a revision turn)
    dispatchChatPlan(envelope("ChatPlanPhaseChanged", {
      session_id: "s-1",
      phase: "drafting",
    }), "s-1");
    // THEN the phase is tracked and the awaiting card is dismissed
    const s = get(chatPlanState);
    expect(s.phase).toBe("drafting");
    expect(s.status).toBe("idle");
  });

  test("PlanUpdated refreshes the plan without changing the status", () => {
    // GIVEN a gate awaiting approval for "s-1"
    dispatchChatPlan(envelope("PlanSubmitted", {
      session_id: "s-1",
      plan: samplePlan("s-1", ["v1"]),
    }), "s-1");
    // WHEN an update carries a revised plan
    dispatchChatPlan(envelope("PlanUpdated", {
      session_id: "s-1",
      plan: samplePlan("s-1", ["v2", "v2b"]),
      mutation: { kind: "modify_step", at: 0 },
    }), "s-1");
    // THEN the plan is refreshed and the gate stays open
    const s = get(chatPlanState);
    expect(s.status).toBe("pending_approval");
    expect(s.plan?.steps[0]?.title).toBe("v2");
    expect(s.plan?.steps).toHaveLength(2);
  });

  test("envelopes from another category are ignored", () => {
    // GIVEN a gate awaiting approval for "s-1"
    dispatchChatPlan(envelope("PlanSubmitted", {
      session_id: "s-1",
      plan: samplePlan("s-1", ["do x"]),
    }), "s-1");
    // WHEN a non plan-mode envelope is dispatched
    dispatchChatPlan({
      category: "task-changed",
      event_type: "TaskCompleted",
      payload: { TaskCompleted: { session_id: "s-1" } },
    }, "s-1");
    // THEN the store is untouched
    expect(get(chatPlanState).status).toBe("pending_approval");
  });

  test("a failed decision surfaces an error without closing the gate", () => {
    // GIVEN a gate awaiting approval for "s-1"
    dispatchChatPlan(envelope("PlanSubmitted", {
      session_id: "s-1",
      plan: samplePlan("s-1", ["do x"]),
    }), "s-1");
    // WHEN the approve command fails and the handler records the error
    setChatPlanError("not awaiting");
    // THEN the store is in error with the message, no crash
    const s = get(chatPlanState);
    expect(s.status).toBe("error");
    expect(s.errorMessage).toBe("not awaiting");
  });
});
