/**
 * Shared tick store for the unified inbox (US-SP42-053, finding E.31).
 *
 * A single `setInterval` ticks every 5 seconds and publishes `Date.now()`.
 * Each inbox card derives its own elapsed display from this shared source,
 * avoiding one timer per item (measured: 200 items → 1 timer).
 *
 * Subscribers are ref-counted: the interval starts on the first subscribe
 * and clears when the last subscriber leaves.
 */
import { readable } from "svelte/store";

const TICK_INTERVAL_MS = 5_000;

export const tickStore = readable<number>(Date.now(), (set) => {
  const intervalId = setInterval(() => {
    set(Date.now());
  }, TICK_INTERVAL_MS);
  return () => clearInterval(intervalId);
});

/** Exposed for tests only — the public contract is the tick interval value. */
export const _tickIntervalMs = TICK_INTERVAL_MS;
