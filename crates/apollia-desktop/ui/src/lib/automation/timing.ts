// Dev-only automation harness: the budget a recipe holds a duration to.
//
// A recipe that promises a duration (the first launch, from the window to the
// first result of a task) reads the clock once, at the step that closes the
// promise, and fails past its budget. The reading is kept apart from the
// runner so the verdict is tested without a webview.

/** What an `expectElapsed` step measured, and whether it holds its budget. */
export interface ElapsedVerdict {
  /** Milliseconds since the interface started loading, rounded. */
  elapsedMs: number;
  /** Whether `elapsedMs` is within the budget (the budget itself is within). */
  ok: boolean;
  /** The sentence the report carries, whichever way the verdict fell. */
  detail: string;
}

function seconds(ms: number): string {
  return `${(ms / 1000).toFixed(1)}s`;
}

/**
 * The verdict of a duration against its budget. `elapsedMs` is what
 * `performance.now()` answers: the time since the webview started loading the
 * interface, which is the launch as a recipe can observe it. A budget that is
 * not a positive number is a recipe error, said as such instead of passing.
 */
export function elapsedVerdict(elapsedMs: number, maxMs: number): ElapsedVerdict {
  const rounded = Math.round(elapsedMs);
  if (!Number.isFinite(maxMs) || maxMs <= 0) {
    return {
      elapsedMs: rounded,
      ok: false,
      detail: `expectElapsed needs a positive maxMs, got ${String(maxMs)}`,
    };
  }
  const ok = rounded <= maxMs;
  return {
    elapsedMs: rounded,
    ok,
    detail: ok
      ? `elapsed ${seconds(rounded)} since launch (budget ${seconds(maxMs)})`
      : `elapsed ${seconds(rounded)} since launch, over the budget of ${seconds(maxMs)}`,
  };
}
