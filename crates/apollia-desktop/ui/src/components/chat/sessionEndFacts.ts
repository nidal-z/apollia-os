/**
 * The debrief facts a closed conversation hands to the Next Steps panel.
 *
 * A pure reduction of the thread: the tail of the exchange, and the tools it
 * used. The counters the panel also reads are not conversation facts, so they
 * stay zero here rather than being invented.
 */
import type { NextStepsFacts } from "$lib/stores/nextSteps";
import type { ChatMessageView } from "$lib/types";

/** How much of the thread the debrief quotes. */
const RECENT_MESSAGES = 8;
/** How much of one message the debrief quotes. */
const EXCERPT_CHARS = 240;
/** How many distinct tools the debrief names. */
const MAX_TOOLS = 12;

export function sessionEndFacts(
  messages: ChatMessageView[],
  memoriesRecalled: number,
): NextStepsFacts {
  return {
    recentMessages: messages
      .slice(-RECENT_MESSAGES)
      .filter((m) => m.role === "user" || m.role === "assistant")
      .map((m) => `${m.role}: ${m.content.slice(0, EXCERPT_CHARS)}`),
    toolsUsed: Array.from(
      new Set(messages.flatMap((m) => m.tool_calls ?? []).map((tc) => tc.tool_name)),
    ).slice(0, MAX_TOOLS),
    memoriesCreated: 0,
    memoriesRecalled,
    inboxPending: 0,
    tasksFailed: 0,
    tasksCompleted: 0,
    automationsFailing: 0,
    signals: [],
  };
}
