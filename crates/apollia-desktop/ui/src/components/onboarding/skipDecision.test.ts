import { describe, test, expect } from "vitest";
import { decideSkip } from "./skipDecision";

describe("configure later, on the onboarding conversation", () => {
  test("finalizes the conversation that is running", () => {
    // GIVEN a conversation in progress
    const state = { sessionId: "session-1", completed: false };

    // WHEN the user asks to configure later
    const outcome = decideSkip(state);

    // THEN the remaining optional questions are finalized, not abandoned
    expect(outcome).toBe("finalize");
  });

  test("leaves onboarding when no conversation ever started", () => {
    // GIVEN a step whose agent never started, so there is no session
    const state = { sessionId: null, completed: false };

    // WHEN the user asks to configure later
    const outcome = decideSkip(state);

    // THEN onboarding is abandoned: this step swallows Escape, and answering
    // nothing here is what trapped a first run on the packaged build
    expect(outcome).toBe("abandon");
  });

  test("does nothing once the agent has finalized", () => {
    // GIVEN a conversation the agent already closed
    const state = { sessionId: "session-1", completed: true };

    // WHEN the user asks to configure later again
    const outcome = decideSkip(state);

    // THEN nothing is asked of the agent a second time
    expect(outcome).toBe("ignore");
  });
});
