/**
 * Anchor resolution for the guided tour.
 *
 * A single module owns both "where is this element" and "has this element
 * appeared yet". The annotated presentation needs the second question answered
 * for its approval-card annotation, and reusing the anchor resolver means the
 * tour needs no separate runtime-event contract.
 *
 * The measurement produced here feeds both the spotlight cutout and the step
 * card. The previous implementation measured them separately, on different
 * schedules, so the hole and the card could disagree.
 */
import type { StepAnchor } from "./types";

/** Default budget for {@link waitForAnchor}, in milliseconds. */
const DEFAULT_ANCHOR_BUDGET_MS = 1_500;

/** Builds the CSS selector matching an anchor. */
export function selectorFor(anchor: StepAnchor): string {
  return anchor.kind === "testid"
    ? `[data-testid="${anchor.value}"]`
    : `[data-testid^="${anchor.value}"]`;
}

/**
 * Turns an `nth` into an absolute index over `length` matches.
 *
 * Negative values count from the end, mirroring the automation scripts where
 * `-1` is the last match. Out-of-range values are returned as-is; the caller's
 * `NodeList.item` yields `null` for them.
 */
export function resolveIndex(nth: number, length: number): number {
  return nth < 0 ? length + nth : nth;
}

/**
 * Returns the anchor's element if it is in the DOM right now, else `null`.
 *
 * Honours `nth` on prefix anchors, so a step can point at the latest chat
 * message rather than the first one.
 */
export function resolveAnchor(anchor: StepAnchor): Element | null {
  if (globalThis.document === undefined) return null;

  const selector = selectorFor(anchor);
  const nth = anchor.kind === "testidPrefix" ? anchor.nth : undefined;
  if (nth === undefined) return document.querySelector(selector);

  const matches = document.querySelectorAll(selector);
  return matches.item(resolveIndex(nth, matches.length));
}

/** Options accepted by {@link waitForAnchor}. */
export interface WaitForAnchorOptions {
  /**
   * How long to wait before giving up. Pass `null` to wait indefinitely, which
   * the annotated presentation does: the approval card may take a while, or may
   * never come, and that silence is an accepted outcome rather than an error.
   */
  readonly budgetMs?: number | null;
  /** Aborts the wait, for instance when the user leaves the step. */
  readonly signal?: AbortSignal;
}

/**
 * Resolves once the anchor is in the DOM, or `null` when the budget expires or
 * the wait is aborted.
 *
 * Observes mutations rather than polling, so an anchor that appears mid-stream
 * is picked up on the same frame instead of up to a poll interval later.
 */
export function waitForAnchor(
  anchor: StepAnchor,
  options: WaitForAnchorOptions = {},
): Promise<Element | null> {
  const { budgetMs = DEFAULT_ANCHOR_BUDGET_MS, signal } = options;

  const immediate = resolveAnchor(anchor);
  if (immediate !== null) return Promise.resolve(immediate);
  if (globalThis.document === undefined) return Promise.resolve(null);
  if (signal?.aborted === true) return Promise.resolve(null);

  return new Promise<Element | null>((resolve) => {
    let settled = false;
    let timer: ReturnType<typeof setTimeout> | null = null;

    const observer = new MutationObserver(() => {
      const el = resolveAnchor(anchor);
      if (el !== null) settle(el);
    });

    function cleanup(): void {
      observer.disconnect();
      if (timer !== null) clearTimeout(timer);
      signal?.removeEventListener("abort", onAbort);
    }

    function settle(value: Element | null): void {
      if (settled) return;
      settled = true;
      cleanup();
      resolve(value);
    }

    function onAbort(): void {
      settle(null);
    }

    signal?.addEventListener("abort", onAbort, { once: true });
    observer.observe(document.body, { childList: true, subtree: true });

    if (budgetMs !== null) {
      timer = setTimeout(() => settle(null), budgetMs);
    }
  });
}

/**
 * Measures an anchor, returning `null` when it is absent or collapsed.
 *
 * A zero-sized box counts as absent: a rendered-but-invisible element would
 * otherwise produce a degenerate spotlight cutout, which is exactly the
 * full-black-overlay failure the previous runner shipped.
 */
export function measureAnchor(anchor: StepAnchor | null): DOMRect | null {
  if (anchor === null) return null;
  const el = resolveAnchor(anchor);
  if (el === null) return null;
  const rect = el.getBoundingClientRect();
  return rect.width > 0 && rect.height > 0 ? rect : null;
}
