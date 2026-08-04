import { describe, test, expect, vi } from "vitest";
import { isChatComplete, runDirectSkip } from "./skipFlow";

const MIN_REAL_REPLIES = 2;
const SAFETY_REPLIES = 12;

describe("isChatComplete", () => {
  test("an explicit operator skip completes immediately", () => {
    // GIVEN a fresh chat where the operator clicked "skip the optional
    // questions" before typing anything
    const inputs = {
      skippedDirectly: true,
      agentFinalized: false,
      realReplies: 0,
    };
    // WHEN the completion gate is evaluated
    // THEN the chat is complete: the stale-memory guard does not apply to a
    // deliberate user action
    expect(isChatComplete(inputs, MIN_REAL_REPLIES, SAFETY_REPLIES)).toBe(true);
  });

  test("agent finalization still requires the minimum real replies", () => {
    // GIVEN a stale completed_at from a previous broken session (no replies)
    const stale = {
      skippedDirectly: false,
      agentFinalized: true,
      realReplies: 0,
    };
    // THEN the guard holds
    expect(isChatComplete(stale, MIN_REAL_REPLIES, SAFETY_REPLIES)).toBe(false);

    // GIVEN the same signal after real engagement
    const engaged = { ...stale, realReplies: MIN_REAL_REPLIES };
    // THEN the chat completes
    expect(isChatComplete(engaged, MIN_REAL_REPLIES, SAFETY_REPLIES)).toBe(true);
  });

  test("the safety net completes without any signal", () => {
    // GIVEN a long conversation with no finalization signal at all
    const inputs = {
      skippedDirectly: false,
      agentFinalized: false,
      realReplies: SAFETY_REPLIES,
    };
    // THEN the backstop completes so the user is never stranded
    expect(isChatComplete(inputs, MIN_REAL_REPLIES, SAFETY_REPLIES)).toBe(true);
  });
});

describe("runDirectSkip", () => {
  test("finalizes through the backend without a chat message", async () => {
    // GIVEN a working finalize command and a conversational fallback
    const finalize = vi.fn().mockResolvedValue(undefined);
    const nudge = vi.fn().mockResolvedValue(undefined);

    // WHEN the skip runs
    const path = await runDirectSkip(finalize, nudge);

    // THEN the backend command ran and no chat message was sent
    expect(path).toBe("finalized");
    expect(finalize).toHaveBeenCalledTimes(1);
    expect(nudge).not.toHaveBeenCalled();
  });

  test("falls back to the conversational nudge on backend failure", async () => {
    // GIVEN a finalize command that fails
    const finalize = vi.fn().mockRejectedValue(new Error("io"));
    const nudge = vi.fn().mockResolvedValue(undefined);

    // WHEN the skip runs
    const path = await runDirectSkip(finalize, nudge);

    // THEN the user is not stranded: the nudge path runs
    expect(path).toBe("nudged");
    expect(nudge).toHaveBeenCalledTimes(1);
  });
});
