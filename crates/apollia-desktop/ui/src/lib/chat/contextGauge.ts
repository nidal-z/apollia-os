/**
 * Pure formatting of the chat context gauge.
 *
 * Extracted from `ContextIndicator.svelte` so the honesty contract is
 * node-testable: a backend that reports no usage (Agent mode, engine that
 * omits `usage`) must read as "unknown", never as a fake percentage.
 */

/** What the gauge says when hovered, in tokens. */
export interface ContextGaugeDetail {
  /** Tokens the conversation occupies in the context. */
  used: number;
  /** Tokens the loaded context holds. */
  window: number;
  /** Share of the context in use, a whole percentage clamped to 100. */
  pct: number;
  /** Tokens left before the context is full, never negative. */
  remaining: number;
}

/**
 * The detail behind the gauge, `null` when the backend reports no context
 * window: there is then nothing measured to detail.
 */
export function contextGaugeDetail(
  windowTokens: number,
  usedTokens: number,
): ContextGaugeDetail | null {
  if (windowTokens <= 0) return null;
  return {
    used: usedTokens,
    window: windowTokens,
    pct: Math.round(contextPct(windowTokens, usedTokens)),
    remaining: Math.max(0, windowTokens - usedTokens),
  };
}

/** From this share on, the gauge warns that the context is nearly full. */
export const CONTEXT_GAUGE_HIGH_PCT = 90;

/**
 * The dash offset that draws `pct` percent of a ring of `circumference`: the
 * whole circumference for an empty ring, zero for a full one.
 */
export function ringDashOffset(pct: number, circumference: number): number {
  const share = Math.min(100, Math.max(0, pct)) / 100;
  return circumference * (1 - share);
}

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
