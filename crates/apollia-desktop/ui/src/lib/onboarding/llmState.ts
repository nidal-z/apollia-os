/**
 * Readiness of the LLM engine as seen by the onboarding chat step.
 *
 * Collapsing "not hydrated yet" and "confirmed empty" into one boolean is what
 * made the step announce "no engine available" during startup: the backend
 * list store starts empty and is only filled by an async round-trip. Pure and
 * node-testable; the component derives its branch exclusively through this.
 */

export type LlmReadiness = "checking" | "downloading" | "ready" | "none";

/**
 * Derive the engine state from the store hydration flag, the backend count,
 * and whether a language model is still on its way.
 *
 * `checking` until the first successful backend-list fetch, then `ready` or
 * `none` based on what that fetch actually confirmed. `downloading` when no
 * backend is there yet because its file is still being fetched or wired: the
 * person was let through the AI step on a download, so "no engine" would send
 * them back to a step they completed.
 */
export function deriveLlmState(
  hydrated: boolean,
  backendCount: number,
  downloadInFlight = false,
): LlmReadiness {
  if (backendCount > 0) {
    return "ready";
  }
  if (downloadInFlight) {
    return "downloading";
  }
  return hydrated ? "none" : "checking";
}
