/**
 * What "configure later" does on the conversation step.
 *
 * The step reached three different states and one button served them all, so
 * the decision lived inside an effect where nothing could hold it. It is a
 * function now, because the branch that was missing is the one that traps a
 * user: no conversation ever started, the card swallows Escape, and the only
 * button answered nothing. Measured on 2026-09-08 on the packaged Windows
 * build, where the onboarding agent was absent from the runtime registry.
 */
export type SkipOutcome = "finalize" | "abandon" | "ignore";

export function decideSkip(state: {
  sessionId: string | null;
  completed: boolean;
}): SkipOutcome {
  if (!state.sessionId) {
    // Nothing to finalize: the conversation never started. Leave onboarding
    // rather than stay on a step that cannot advance.
    return "abandon";
  }
  if (state.completed) {
    // The agent already finalized; a second skip would ask it to redo so.
    return "ignore";
  }
  return "finalize";
}
