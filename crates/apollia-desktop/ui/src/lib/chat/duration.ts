/**
 * Shared duration formatting for the chat surface.
 *
 * Tool durations are rendered as locale-aware seconds with one decimal (e.g.
 * "0,3" in fr, "0.3" in en), matching the validated chat mockup, rather than
 * raw milliseconds. Callers append the unit (" s") themselves so they control
 * spacing and any surrounding markup.
 *
 * The name deliberately avoids the `formatDuration` prefix. It is a different
 * rendering from `formatDuration` in `lib/utils.ts`, locale-aware and without a
 * unit, and two helpers whose names share that prefix cannot be told apart by a
 * reader looking for the duration formatter.
 */

/** Format a millisecond duration as locale-aware seconds, one fraction digit. */
export function formatSecondsLocalized(durationMs: number, loc: string): string {
  const seconds = durationMs / 1000;
  try {
    return new Intl.NumberFormat(loc, {
      minimumFractionDigits: 1,
      maximumFractionDigits: 1,
    }).format(seconds);
  } catch {
    return seconds.toFixed(1);
  }
}
