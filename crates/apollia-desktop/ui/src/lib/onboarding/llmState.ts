/**
 * Tri-state readiness of the LLM engine as seen by the onboarding chat step.
 *
 * Collapsing "not hydrated yet" and "confirmed empty" into one boolean is what
 * made the step announce "no engine available" during startup: the backend
 * list store starts empty and is only filled by an async round-trip. Pure and
 * node-testable; the component derives its branch exclusively through this.
 */

export type LlmReadiness = "checking" | "ready" | "none";

/**
 * Derive the engine state from the store hydration flag and the backend count.
 *
 * `checking` until the first successful backend-list fetch, then `ready` or
 * `none` based on what that fetch actually confirmed.
 */
export function deriveLlmState(
  hydrated: boolean,
  backendCount: number,
): LlmReadiness {
  if (backendCount > 0) {
    return "ready";
  }
  return hydrated ? "none" : "checking";
}
