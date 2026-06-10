import { vi, describe, test, expect, beforeEach } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue(undefined),
}));

import { invoke } from "@tauri-apps/api/core";
import { setPlanMode, approvePlan, rejectPlan } from "./planMode";

const mockedInvoke = vi.mocked(invoke);

beforeEach(() => {
  vi.clearAllMocks();
});

describe("planMode IPC - session-keyed wrappers", () => {
  test("setPlanMode forwards the command name and camelCase args", async () => {
    // GIVEN a mocked invoke
    // WHEN plan mode is enabled for a session
    await setPlanMode("s-1", true);
    // THEN the typed command is called with the session id and flag
    expect(mockedInvoke).toHaveBeenCalledWith("set_plan_mode", {
      sessionId: "s-1",
      enabled: true,
    });
  });

  test("approvePlan forwards only the session id", async () => {
    // GIVEN a mocked invoke
    // WHEN the plan is approved
    await approvePlan("s-2");
    // THEN the approve command is called with the session id
    expect(mockedInvoke).toHaveBeenCalledWith("approve_plan", {
      sessionId: "s-2",
    });
  });

  test("rejectPlan forwards the session id and the optional reason", async () => {
    // GIVEN a mocked invoke
    // WHEN the plan is rejected with a reason
    await rejectPlan("s-3", "too risky");
    // THEN the reject command carries both fields
    expect(mockedInvoke).toHaveBeenCalledWith("reject_plan", {
      sessionId: "s-3",
      reason: "too risky",
    });
  });

  test("rejectPlan passes an undefined reason when omitted", async () => {
    // GIVEN a mocked invoke
    // WHEN the plan is rejected without a reason
    await rejectPlan("s-4");
    // THEN the reason is undefined (the runtime treats it as no feedback)
    expect(mockedInvoke).toHaveBeenCalledWith("reject_plan", {
      sessionId: "s-4",
      reason: undefined,
    });
  });
});
