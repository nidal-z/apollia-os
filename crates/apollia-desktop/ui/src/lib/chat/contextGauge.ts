/**
 * Pure formatting of the chat context gauge.
 *
 * Extracted from `ContextIndicator.svelte` so the honesty contract is
 * node-testable: a backend that reports no usage (Agent mode, engine that
 * omits `usage`) must read as "unknown", never as a fake percentage.
 */

/** Context occupancy in percent, clamped to 100. Zero when the window is unknown. */
export function contextPct(windowTokens: number, usedTokens: number): number {
  return windowTokens > 0
    ? Math.min(100, (usedTokens / windowTokens) * 100)
    : 0;
}

/**
 * Display label for the gauge: a rounded percentage when the window is known,
 * `--` when the backend reports none (0 means "no measurement", not "empty").
 */
export function contextGaugeLabel(
  windowTokens: number,
  usedTokens: number,
): string {
  if (windowTokens <= 0) {
    return "--";
  }
  return `${Math.round(contextPct(windowTokens, usedTokens))}%`;
}
