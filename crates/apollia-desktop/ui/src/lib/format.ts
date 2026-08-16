/**
 * Shared display formatting for user-visible values.
 *
 * The application language governs every displayed date and number, never the
 * OS locale: Svelte components feed `$locale ?? "en"` from `svelte-i18n` into
 * the `locale` parameters here so a language switch re-renders the value.
 * Wire timestamps stay ISO 8601 UTC; only display formatting lives here.
 */

/**
 * Formats a USD cost identically on every surface.
 *
 * Zero renders as a plain `$0.00`; sub-cent amounts keep four decimals so
 * cheap LLM calls stay readable; everything else rounds to the cent.
 */
export function formatCost(usd: number): string {
  if (usd === 0) return "$0.00";
  if (usd < 0.01) return `$${usd.toFixed(4)}`;
  return `$${usd.toFixed(2)}`;
}

/**
 * Uppercase day headline, e.g. `SATURDAY, AUGUST 15` / `SAMEDI 15 AOÛT`.
 *
 * Returns an empty string when the date cannot be formatted, so the headline
 * degrades to nothing rather than crashing the dashboard.
 */
export function formatDayHeadline(date: Date, locale: string): string {
  if (Number.isNaN(date.getTime())) return "";
  try {
    return date
      .toLocaleDateString(locale, { weekday: "long", day: "numeric", month: "long" })
      .toUpperCase();
  } catch {
    return "";
  }
}
