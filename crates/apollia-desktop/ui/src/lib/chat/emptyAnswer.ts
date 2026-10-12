// Whether an agent turn that ends without text is an exchange that produced
// nothing, or one whose answer stands elsewhere on the screen.

import type { ToolCallView } from "$lib/types/chat";

/** The tools through which a turn lays a plan before the user, shown as a card. */
const PLAN_PROPOSAL_TOOLS = new Set(["plan_propose", "plan_submit"]);

/**
 * True when the turn proposed a plan: the plan card under the message is its
 * answer, so the message has nothing to apologise for.
 */
export function answeredByPlan(calls: ToolCallView[]): boolean {
  return calls.some((c) => c.status === "executed" && PLAN_PROPOSAL_TOOLS.has(c.tool_name));
}

/**
 * True when a finished agent turn left the user with nothing: no text, no
 * action button, and no plan laid before them.
 */
export function isEmptyAnswer(input: {
  isUser: boolean;
  text: string;
  buttons: number;
  calls: ToolCallView[];
}): boolean {
  if (input.isUser) return false;
  if (input.text.trim() !== "" || input.buttons > 0) return false;
  return !answeredByPlan(input.calls);
}
