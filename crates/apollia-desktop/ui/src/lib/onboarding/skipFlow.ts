/**
 * Pure decision logic of the onboarding chat step's completion and skip flow.
 *
 * Extracted from `OnboardingChatStep.svelte` so the contract is unit-testable
 * in the node test environment (components are not rendered in tests, see
 * `vitest.config.ts`). The component derives its state exclusively through
 * these helpers.
 */

export interface ChatCompletionInputs {
  /** The operator explicitly skipped the optional questions. */
  skippedDirectly: boolean;
  /** The agent (or the direct skip) stamped `onboarding.completed_at`. */
  agentFinalized: boolean;
  /** User replies excluding the auto-kick message. */
  realReplies: number;
}

/**
 * Whether the acquaintance chat is complete and the flow may enter the
 * permissions phase.
 *
 * An explicit operator skip completes immediately: the `minRealReplies` guard
 * only exists to distrust a stale `completed_at` from a previous broken
 * session, which cannot apply to a deliberate user action.
 */
export function isChatComplete(
  inputs: ChatCompletionInputs,
  minRealReplies: number,
  safetyReplies: number,
): boolean {
  return (
    inputs.skippedDirectly ||
    (inputs.agentFinalized && inputs.realReplies >= minRealReplies) ||
    inputs.realReplies >= safetyReplies
  );
}

/**
 * Run the direct-skip flow: finalize through the backend command (no model
 * turn); only when that fails, fall back to the conversational nudge so the
 * user is never stranded.
 *
 * Returns which path ran, so the caller updates its state accordingly.
 */
export async function runDirectSkip(
  finalize: () => Promise<void>,
  nudge: () => Promise<void>,
): Promise<"finalized" | "nudged"> {
  try {
    await finalize();
    return "finalized";
  } catch {
    await nudge();
    return "nudged";
  }
}
