import { describe, test, expect, beforeEach, vi } from "vitest";

const { invokeMock } = vi.hoisted(() => ({
  invokeMock: vi.fn(async (..._args: unknown[]) => "March invoices"),
}));
vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));

import {
  AUTO_NAME_MIN_TOKENS_PER_SECOND,
  autoNameDecision,
  noteGenerationSpeed,
  resetAutoNameState,
  tokensPerSecondOf,
  triggerAutoName,
} from "./autoName";
import { chatAutoNaming } from "$lib/stores/chatAutoNaming";

const FAST = AUTO_NAME_MIN_TOKENS_PER_SECOND + 10;
const SLOW = AUTO_NAME_MIN_TOKENS_PER_SECOND / 4;

function namingCalls(): number {
  return invokeMock.mock.calls.filter((c) => c[0] === "generate_chat_session_name").length;
}

beforeEach(() => {
  invokeMock.mockClear();
  resetAutoNameState();
  chatAutoNaming.set("auto");
});

describe("auto-naming - the three choices", () => {
  test("never: the first message does not ask the model for a name", () => {
    // GIVEN naming set to never, and a fast model
    chatAutoNaming.set("never");
    noteGenerationSpeed(FAST);

    // WHEN a conversation receives its first message
    triggerAutoName("s-never", "Add up the March invoices.");

    // THEN generate_chat_session_name is not called
    expect(namingCalls()).toBe(0);
  });

  test("always: the first message asks for a name, however slow the model", () => {
    // GIVEN naming set to always, and a slow model
    chatAutoNaming.set("always");
    noteGenerationSpeed(SLOW);

    // WHEN a conversation receives its first message
    triggerAutoName("s-always", "Add up the March invoices.");

    // THEN generate_chat_session_name is called once, with the message
    expect(namingCalls()).toBe(1);
    expect(invokeMock).toHaveBeenCalledWith("generate_chat_session_name", {
      sessionId: "s-always",
      firstMessage: "Add up the March invoices.",
    });
  });

  test("auto: a fast model names the conversation, a slow one does not", () => {
    // GIVEN automatic naming, a fast model measured
    noteGenerationSpeed(FAST);

    // WHEN a first message arrives, then the speed drops and another arrives
    triggerAutoName("s-fast", "First message");
    noteGenerationSpeed(SLOW);
    triggerAutoName("s-slow", "First message");

    // THEN only the conversation started on the fast model is named
    expect(namingCalls()).toBe(1);
    expect(invokeMock.mock.calls[0]?.[1]).toMatchObject({ sessionId: "s-fast" });
  });

  test("auto: before any measure, the conversation waits for the first speed", () => {
    // GIVEN automatic naming and no speed measured yet
    triggerAutoName("s-wait", "First message");
    const before = namingCalls();

    // WHEN the first answer completes at a fast speed
    noteGenerationSpeed(FAST);

    // THEN the waiting conversation is named then, not before
    expect(before).toBe(0);
    expect(namingCalls()).toBe(1);
  });

  test("the decision follows the speed threshold written in the code", () => {
    // GIVEN the three choices and speeds around the threshold
    // WHEN each pair is decided
    // THEN never skips, always names, auto names from the threshold up
    expect(autoNameDecision("never", FAST)).toBe("skip");
    expect(autoNameDecision("always", null)).toBe("name");
    expect(autoNameDecision("auto", null)).toBe("wait");
    expect(autoNameDecision("auto", AUTO_NAME_MIN_TOKENS_PER_SECOND)).toBe("name");
    expect(autoNameDecision("auto", AUTO_NAME_MIN_TOKENS_PER_SECOND - 0.1)).toBe("skip");
  });

  test("the speed of a call is its generated tokens over its generation time", () => {
    // GIVEN a call that generated 412 tokens in 10.3 s, and one not measured
    const measured = { completion_tokens: 412, generation_ms: 10_300 };
    const unmeasured = { completion_tokens: 412 };

    // WHEN their speeds are read
    // THEN the first gives 40 tokens a second, the second nothing
    expect(tokensPerSecondOf(measured)).toBeCloseTo(40, 5);
    expect(tokensPerSecondOf(unmeasured)).toBeNull();
  });
});
