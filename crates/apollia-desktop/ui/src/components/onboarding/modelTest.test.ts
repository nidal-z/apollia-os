import { describe, expect, test } from "vitest";
import { MODEL_TEST_TIMEOUT_MS, modelTestOutcome } from "./modelTest";

const message = (role: "user" | "assistant", content: string, seq: number) => ({
  id: `m${seq}`,
  role,
  content,
  tool_calls: null,
  tool_name: null,
  seq,
  created_at: "2026-10-09T09:00:00Z",
});

describe("the model test of the first launch", () => {
  test("an answer the model finished writing ends the test", () => {
    // GIVEN a conversation where the model answered and stopped
    const session = {
      status: "active" as const,
      messages: [message("user", "Say hello.", 1), message("assistant", " Hello! ", 2)],
    };

    // WHEN the outcome is read
    const outcome = modelTestOutcome(session, 3000);

    // THEN the test is answered, with the answer trimmed
    expect(outcome).toEqual({ kind: "answered", answer: "Hello!" });
  });

  test("an answer still being written is waited for", () => {
    // GIVEN a conversation the model is still writing in
    const session = {
      status: "processing" as const,
      messages: [message("user", "Say hello.", 1), message("assistant", "Hel", 2)],
    };

    // WHEN the outcome is read before the limit
    const outcome = modelTestOutcome(session, 3000);

    // THEN the test goes on waiting
    expect(outcome).toEqual({ kind: "waiting" });
  });

  test("no answer at the limit gives the test up", () => {
    // GIVEN a conversation with no answer at all
    const session = { status: "active" as const, messages: [message("user", "Say hello.", 1)] };

    // WHEN the outcome is read at the limit, and just before it
    const late = modelTestOutcome(session, MODEL_TEST_TIMEOUT_MS);
    const early = modelTestOutcome(session, MODEL_TEST_TIMEOUT_MS - 1);

    // THEN the test gives up at the limit and not before
    expect(late).toEqual({ kind: "timed_out" });
    expect(early).toEqual({ kind: "waiting" });
  });
});
