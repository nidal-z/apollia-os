/**
 * Whether a control would accept a click, and how long to wait for it.
 *
 * Lives beside the runner rather than inside it: runner.ts sits against the
 * 800-line ceiling the tree holds, and this is a self-contained rule.
 */
const POLL_INTERVAL_MS = 150;
// How long a click waits for a disabled control to enable before going
// through anyway. Bounded on purpose: see waitClickable.
const CLICKABLE_BUDGET_MS = 3_000;

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

/** What `clickIsBlocked` needs of a target, so it can be tested without a DOM. */
export interface ClickTarget {
  disabled?: boolean;
  getAttribute(name: string): string | null;
}

/** True when a control would ignore a click: the browser drops it silently. */
export function clickIsBlocked(el: ClickTarget): boolean {
  return el.disabled === true || el.getAttribute("aria-disabled") === "true";
}

/**
 * Give a control a bounded moment to become clickable.
 *
 * A disabled button ignores a click and the browser reports nothing, so the
 * step passes while the gesture never happened, and the next step waits out
 * its whole timeout on a screen that will not come. That is what a book does
 * when it ticks four boxes and clicks "next" in the same instant: the DOM has
 * not reflected the enable yet. Measured on 2026-09-08 on Windows, where the
 * reflow lands later than the click; the same book passes on macOS and Linux
 * for no better reason than speed.
 *
 * The wait is bounded and the click happens either way, so a recipe that
 * deliberately clicks a disabled control keeps its meaning and only pays the
 * wait. Returns how long it waited, for the step's own report.
 */
export async function waitClickable(el: ClickTarget, timeoutMs: number): Promise<number> {
  const budget = Math.min(timeoutMs, CLICKABLE_BUDGET_MS);
  const startedAt = Date.now();
  const deadline = startedAt + budget;
  while (clickIsBlocked(el) && Date.now() < deadline) {
    await sleep(POLL_INTERVAL_MS);
  }
  return Date.now() - startedAt;
}
