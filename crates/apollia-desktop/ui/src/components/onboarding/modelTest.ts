/**
 * The verdict of the model test of the first launch, read from the
 * conversation that carries it. Pure, so the rule is tested without a
 * component: the test is answered once the model wrote something and stopped
 * writing, and gives up after a wait a slow machine still fits in.
 */
import type { ChatSessionDetail } from "$lib/types";

/** How often the conversation of the test is read again. */
export const MODEL_TEST_POLL_MS = 1000;

/** How long the test waits for an answer before it gives up. */
export const MODEL_TEST_TIMEOUT_MS = 120_000;

export type ModelTestOutcome =
  | { kind: "waiting" }
  | { kind: "answered"; answer: string }
  | { kind: "timed_out" };

/** What the conversation says of the test, `elapsedMs` after it was sent. */
export function modelTestOutcome(
  session: Pick<ChatSessionDetail, "status" | "messages">,
  elapsedMs: number,
): ModelTestOutcome {
  const answer = session.messages
    .filter((message) => message.role === "assistant")
    .map((message) => message.content.trim())
    .findLast((content) => content.length > 0);
  if (answer !== undefined && session.status !== "processing") {
    return { kind: "answered", answer };
  }
  return elapsedMs >= MODEL_TEST_TIMEOUT_MS ? { kind: "timed_out" } : { kind: "waiting" };
}
