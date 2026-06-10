import { describe, test, expect } from "vitest";
import {
  decisionClass,
  decisionLabelKey,
  appendDecision,
  type LoggedDecision,
} from "./HookDecisionLog.svelte";

// DOM rendering is exercised by the Playwright E2E layer (vitest runs in `node`).
// These tests lock the color mapping, label keys and the live accumulation.

function entry(id: number, decision: LoggedDecision["decision"]): LoggedDecision {
  return {
    id,
    at: "12:00:00",
    run_id: "run-1",
    session_id: "s-1",
    tool_name: "bash_executor",
    decision,
    rewritten_args: null,
  };
}

describe("HookDecisionLog - decisionClass", () => {
  test("maps each kind to its token color", () => {
    // GIVEN the three decision kinds
    // THEN allow is success, deny destructive, rewrite warning
    expect(decisionClass("allow")).toBe("text-success");
    expect(decisionClass("deny")).toBe("text-destructive");
    expect(decisionClass("rewrite")).toBe("text-warning");
  });
});

describe("HookDecisionLog - decisionLabelKey", () => {
  test("builds the namespaced i18n key for a kind", () => {
    expect(decisionLabelKey("deny")).toBe("observability.hooks_decision_deny");
  });
});

describe("HookDecisionLog - appendDecision", () => {
  test("prepends the newest decision", () => {
    // GIVEN a log with one allow
    const list = [entry(0, "allow")];

    // WHEN a deny arrives
    const next = appendDecision(list, entry(1, "deny"));

    // THEN the newest is first and the original is untouched
    expect(next[0].id).toBe(1);
    expect(next[1].id).toBe(0);
    expect(list).toHaveLength(1);
  });

  test("caps the log length, dropping the oldest", () => {
    // GIVEN a full log at the cap of 2
    const list = [entry(2, "allow"), entry(1, "allow")];

    // WHEN a new decision arrives with cap 2
    const next = appendDecision(list, entry(3, "deny"), 2);

    // THEN the length stays at the cap and the oldest (id 1) is dropped
    expect(next).toHaveLength(2);
    expect(next.map((d) => d.id)).toEqual([3, 2]);
  });
});
