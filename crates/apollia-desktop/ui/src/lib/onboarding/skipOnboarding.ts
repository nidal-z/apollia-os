/**
 * The "configure later" action of the onboarding modal.
 *
 * `dismiss_onboarding` is what writes the `onboarding_skipped` flag
 * (`crates/apollia-desktop/src/commands/onboarding.rs`), and `App.svelte`
 * re-reads exactly that flag at launch to decide whether the modal opens
 * again. Closing the modal on a failed write therefore told the operator the
 * skip had been recorded when it had not: onboarding reappeared at the next
 * launch, with nothing ever having been said.
 *
 * The decision lives in this module rather than inside the component because
 * the frontend runner mounts no Svelte component (`vitest.config.ts` is
 * node-only and collects `.test.ts` files), so a policy left in the modal is
 * out of reach of every guard this repository has.
 */

export interface SkipOnboardingDeps {
  /** Persists the skip. Rejects when the backend refused or was unreachable. */
  dismiss: () => Promise<void>;
  /** Surfaces the failure where the operator is looking. */
  report: (err: unknown) => void;
  /** Closes the modal. Reached only once the skip is actually persisted. */
  close: () => void;
}

/**
 * Skip onboarding, and close the modal only if the backend recorded it.
 *
 * Returns `true` when the flag was written and the modal closed, `false` when
 * the failure was reported and the modal was left open.
 */
export async function skipOnboarding(
  deps: SkipOnboardingDeps,
): Promise<boolean> {
  try {
    await deps.dismiss();
  } catch (err) {
    deps.report(err);
    return false;
  }
  deps.close();
  return true;
}
