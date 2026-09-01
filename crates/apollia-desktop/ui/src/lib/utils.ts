import { clsx, type ClassValue } from "clsx";
import { extendTailwindMerge } from "tailwind-merge";
import { get } from "svelte/store";
import { t } from "svelte-i18n";

/**
 * The font-size keys `tailwind.config.ts` adds on top of Tailwind's own.
 *
 * tailwind-merge resolves conflicts from a table of the class groups it ships
 * with, and `text-*` is ambiguous: it is a font size for the keys Tailwind
 * declares and a colour for everything else. A key this list does not name
 * therefore lands in the colour group, and a colour class written after it in
 * the same `cn()` call removes it as a conflict.
 *
 * That is not theoretical. Measured on the tree, `cn("text-overline",
 * "text-info")` returned `text-info` alone, so every `Badge` shipped with no
 * size class at all and rendered at the 16px it inherited, whatever the scale
 * said. Four rounds of adjusting the scale could not move it, because the size
 * never reached the element. `text-xs` survived the same call, being a key
 * tailwind-merge already knows, which is what made the defect look like a
 * question of tiers.
 *
 * `src/lib/utils.tailwind-merge.test.ts` reads the keys back out of the config
 * and fails when one of them is missing here.
 */
export const CUSTOM_FONT_SIZE_TIERS = [
  "display-xl",
  "display-lg",
  "display-md",
  "display-sm",
  "heading-lg",
  "heading-md",
  "heading-sm",
  "body-lg",
  "body-md",
  "body-sm",
  "body-xs",
  "label-md",
  "label-sm",
  "caption",
  "caption-lg",
  "overline",
  "code-sm",
  "micro-lg",
  "micro",
  "micro-sm",
  "micro-xs",
] as const;

const twMerge = extendTailwindMerge({
  extend: {
    classGroups: {
      "font-size": [{ text: [...CUSTOM_FONT_SIZE_TIERS] }],
    },
  },
});

/** Merge Tailwind classes with clsx and tailwind-merge (shadcn-svelte pattern). */
export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}

/**
 * Parse a timestamp to epoch milliseconds, treating timezone-less strings as
 * UTC.
 *
 * SQLite `CURRENT_TIMESTAMP` columns reach the UI as `"YYYY-MM-DD HH:MM:SS"`
 * with no timezone marker. `new Date(...)` would read those as local time and
 * skew every relative-time display by the local UTC offset. Strings that
 * already carry a `Z` suffix or a numeric offset are parsed as-is.
 */
export function parseTimestampMs(iso: string): number {
  const s = iso.trim();
  const hasTz = /[zZ]$|[+-]\d{2}:?\d{2}$/.test(s);
  if (hasTz) return new Date(s).getTime();
  // A time component without a timezone means the wire value is UTC.
  const normalized = s.includes(":") ? `${s.replace(" ", "T")}Z` : s;
  return new Date(normalized).getTime();
}

/**
 * Format an ISO date string as a relative time string (e.g. "30s ago",
 * "5m ago"). Dates older than a week render as a full date in `locale`,
 * the application language fed by the caller (`$locale ?? "en"`).
 */
export function formatRelativeTime(isoDate: string, locale: string): string {
  if (!isoDate) return "-";
  const then = parseTimestampMs(isoDate);
  if (Number.isNaN(then)) return "-";
  const tr = get(t);
  const diffSecs = Math.floor((Date.now() - then) / 1000);
  if (diffSecs < 0) return tr("common.relative.just_now");
  if (diffSecs < 60) {
    return tr("common.relative.seconds", { values: { n: diffSecs } });
  }
  const diffMins = Math.floor(diffSecs / 60);
  if (diffMins < 60) {
    return tr("common.relative.minutes", { values: { n: diffMins } });
  }
  const diffHours = Math.floor(diffMins / 60);
  if (diffHours < 24) {
    return tr("common.relative.hours", { values: { n: diffHours } });
  }
  const diffDays = Math.floor(diffHours / 24);
  if (diffDays < 7) {
    return tr("common.relative.days", { values: { n: diffDays } });
  }
  return new Date(isoDate).toLocaleDateString(locale);
}

/** The one rendering of a millisecond duration: `418ms`, `1.4s`, `1m 30s`. */
export function formatDuration(ms: number | undefined | null): string {
  if (ms === undefined || ms === null) return "-";
  if (ms < 1000) return `${ms}ms`;
  if (ms < 60_000) return `${(ms / 1000).toFixed(1)}s`;
  const mins = Math.floor(ms / 60_000);
  const secs = Math.floor((ms % 60_000) / 1000);
  return `${mins}m ${secs}s`;
}

/**
 * Format a running count of whole seconds as a coarse clock: `45s`, `3m 20s`,
 * `1h 5m`.
 *
 * This is not `formatDuration` under another name. It takes seconds rather
 * than milliseconds, it drops the sub-second precision a live counter cannot
 * carry, and it folds minutes into hours, which a task duration never needs.
 * It exists for elapsed-time readouts that tick while the user watches them.
 */
export function formatElapsedClock(totalSeconds: number): string {
  if (totalSeconds < 60) return `${totalSeconds}s`;
  const mins = Math.floor(totalSeconds / 60);
  const secs = totalSeconds % 60;
  if (mins < 60) return secs === 0 ? `${mins}m` : `${mins}m ${secs}s`;
  const hours = Math.floor(mins / 60);
  const restMins = mins % 60;
  return restMins === 0 ? `${hours}h` : `${hours}h ${restMins}m`;
}
