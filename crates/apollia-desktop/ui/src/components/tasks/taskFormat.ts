// Formatting helpers shared by the Tasks detail header and its panes.

/** Human duration: `418ms`, `1.4s`, or `1m 48s`. Returns `-` when unknown. */
export function formatDurationLong(ms: number | undefined | null): string {
  if (ms === undefined || ms === null) return "-";
  if (ms < 1000) return `${ms}ms`;
  if (ms < 60_000) return `${(ms / 1000).toFixed(1)}s`;
  const mins = Math.floor(ms / 60_000);
  const secs = Math.floor((ms % 60_000) / 1000);
  return `${mins}m ${secs}s`;
}

/** Locale date-time, or `-` for an empty ISO string. */
export function formatDate(iso: string): string {
  if (!iso) return "-";
  return new Date(iso).toLocaleString();
}
