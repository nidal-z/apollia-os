/**
 * When the first-launch modal opens, and when it must not open again.
 *
 * The modal opens on a first launch, and on a language model that arrives
 * after a start without one. Once it was closed, finished or put off, neither
 * reopens it: the close is remembered for the rest of this run before the
 * backend has recorded anything, so a model that arrives in between finds a
 * first launch that is over.
 */
import type { OnboardingState } from "$lib/types";

/** Whether the stored state is the one of a first launch never begun. */
export function shouldOpenOnboarding(state: OnboardingState): boolean {
  return !state.completed && !state.skipped && state.started_at === null;
}

/** What the arrival of a language model is judged on. */
export interface EngineArrival {
  /** How many language models were configured before the change. */
  previous: number;
  /** How many are configured now. */
  count: number;
  /** Whether the modal was closed during this run of the application. */
  closedThisRun: boolean;
}

/**
 * Whether a change of the configured language models is worth re-reading the
 * stored state for: the first one just arrived, and the first launch was not
 * closed during this run.
 */
export function engineArrivalAsksAgain(arrival: EngineArrival): boolean {
  if (arrival.closedThisRun) return false;
  return arrival.previous === 0 && arrival.count > 0;
}
