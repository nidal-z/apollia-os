// Formatting helpers shared by the Tasks detail header and its panes.

/**
 * Date-time in the application language (`locale` is the caller's
 * `$locale ?? "en"`), or `-` for an empty ISO string.
 */
export function formatDate(iso: string, locale: string): string {
  if (!iso) return "-";
  return new Date(iso).toLocaleString(locale);
}
