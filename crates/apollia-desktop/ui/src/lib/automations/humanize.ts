/**
 * Human-friendly labels for automation schedules.
 *
 * Operator vocabulary - never exposes cron digits, payload, or on-busy.
 * Returns a complete sentence usable inline in an `AutomationScheduleLabel`.
 * Falls back to "Custom schedule" / "Planification personnalisée" with the
 * raw config moved to a tooltip by the caller.
 */

export type ScheduleKind = "cron" | "interval" | "file_watch" | "webhook" | "oneshot";
export type Locale = "en" | "fr";

const CRON_EXACT: Record<string, Record<Locale, string>> = {
  "* * * * *": { en: "Every minute", fr: "Toutes les minutes" },
  "0 * * * *": { en: "Every hour on the hour", fr: "Toutes les heures pile" },
  "0 0 * * *": { en: "Every day at midnight", fr: "Tous les jours à minuit" },
  "0 12 * * *": { en: "Every day at noon", fr: "Tous les jours à midi" },
  "0 0 * * 0": { en: "Every Sunday at midnight", fr: "Chaque dimanche à minuit" },
  "0 0 * * 1": { en: "Every Monday at midnight", fr: "Chaque lundi à minuit" },
  "0 0 1 * *": { en: "First day of every month", fr: "Le 1er de chaque mois" },
};

const DOW_LABEL_EN = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];
const DOW_LABEL_FR = ["dimanche", "lundi", "mardi", "mercredi", "jeudi", "vendredi", "samedi"];

const INTERVAL_RE = /^(\d+)\s*([smhd])?$/;
const STEP_RE = /^\*\/\d+$/;
const ONE_OR_TWO_DIGITS = /^\d{1,2}$/;
const DOW_SINGLE = /^[0-6]$/;

function pad(n: string | number): string {
  return String(n).padStart(2, "0");
}

/**
 * Formats a hour/minute pair in French operator vocabulary (`8h`, `8h30`).
 * Drops the minutes when they are zero, matching the operator's mental model.
 */
function frenchClock(hour: number, minute: number): string {
  return minute === 0 ? `${hour}h` : `${hour}h${pad(minute)}`;
}

/**
 * Recognises `*\/N * * * *` (every N minutes) and returns the localised
 * label, or null when the expression doesn't match.
 */
function matchEveryNMinutes(
  parts: readonly string[],
  loc: Locale,
): string | null {
  const [minute, hour, dayOfMonth, month, dayOfWeek] = parts;
  if (!STEP_RE.test(minute) || hour !== "*" || dayOfMonth !== "*" || month !== "*" || dayOfWeek !== "*") {
    return null;
  }
  const n = Number.parseInt(minute.slice(2), 10);
  if (n <= 0) return null;
  return loc === "fr" ? `Toutes les ${n} minutes` : `Every ${n} minutes`;
}

/**
 * Recognises `0 *\/N * * *` (every N hours) and returns the localised label,
 * or null when the expression doesn't match.
 */
function matchEveryNHours(parts: readonly string[], loc: Locale): string | null {
  const [minute, hour, dayOfMonth, month, dayOfWeek] = parts;
  if (minute !== "0" || !STEP_RE.test(hour) || dayOfMonth !== "*" || month !== "*" || dayOfWeek !== "*") {
    return null;
  }
  const n = Number.parseInt(hour.slice(2), 10);
  if (n <= 0) return null;
  return loc === "fr" ? `Toutes les ${n} heures` : `Every ${n} hours`;
}

/**
 * Recognises `M H * * <dow-spec>` patterns: weekdays (`1-5`), weekends
 * (`0,6` / `6,0`), single day-of-week (`0..6`), or all days (`*`).
 */
function matchDailyWithDow(parts: readonly string[], loc: Locale): string | null {
  const [minute, hour, dayOfMonth, month, dayOfWeek] = parts;
  if (!ONE_OR_TWO_DIGITS.test(minute) || !ONE_OR_TWO_DIGITS.test(hour)) return null;
  if (dayOfMonth !== "*" || month !== "*") return null;

  const hRaw = Number.parseInt(hour, 10);
  const mRaw = Number.parseInt(minute, 10);

  if (dayOfWeek === "1-5") {
    return loc === "fr"
      ? `Tous les matins à ${frenchClock(hRaw, mRaw)} en semaine`
      : `Every weekday at ${pad(hRaw)}:${pad(mRaw)}`;
  }
  if (dayOfWeek === "0,6" || dayOfWeek === "6,0") {
    return loc === "fr"
      ? `Le week-end à ${frenchClock(hRaw, mRaw)}`
      : `On weekends at ${pad(hRaw)}:${pad(mRaw)}`;
  }
  if (DOW_SINGLE.test(dayOfWeek)) {
    const d = Number.parseInt(dayOfWeek, 10);
    return loc === "fr"
      ? `Chaque ${DOW_LABEL_FR[d]} à ${frenchClock(hRaw, mRaw)}`
      : `Every ${DOW_LABEL_EN[d]} at ${pad(hRaw)}:${pad(mRaw)}`;
  }
  if (dayOfWeek === "*") {
    return loc === "fr"
      ? `Tous les jours à ${frenchClock(hRaw, mRaw)}`
      : `Every day at ${pad(hRaw)}:${pad(mRaw)}`;
  }
  return null;
}

/**
 * Recognises `0 H D * *` (monthly on the Dth at H:00).
 */
function matchMonthly(parts: readonly string[], loc: Locale): string | null {
  const [minute, hour, dayOfMonth, month, dayOfWeek] = parts;
  if (
    minute !== "0" ||
    !ONE_OR_TWO_DIGITS.test(hour) ||
    !ONE_OR_TWO_DIGITS.test(dayOfMonth) ||
    month !== "*" ||
    dayOfWeek !== "*"
  ) {
    return null;
  }
  const hRaw = Number.parseInt(hour, 10);
  const d = Number.parseInt(dayOfMonth, 10);
  return loc === "fr"
    ? `Le ${d} de chaque mois à ${hRaw}h`
    : `Monthly on the ${d}${ordinalSuffix(d)} at ${pad(hRaw)}:00`;
}

function humanizeCron(expr: string, loc: Locale): string | null {
  const trimmed = expr.trim();
  const exact = CRON_EXACT[trimmed];
  if (exact) return exact[loc];

  const parts = trimmed.split(/\s+/);
  if (parts.length < 5) return null;

  return (
    matchEveryNMinutes(parts, loc) ??
    matchEveryNHours(parts, loc) ??
    matchDailyWithDow(parts, loc) ??
    matchMonthly(parts, loc)
  );
}

function ordinalSuffix(n: number): string {
  const s = ["th", "st", "nd", "rd"];
  const v = n % 100;
  return s[(v - 20) % 10] ?? s[v] ?? s[0];
}

/**
 * Converts an interval expression (`30s`, `5m`, `2h`, `1d`) to seconds.
 * Returns null when the expression is malformed.
 */
function intervalToSeconds(config: string): number | null {
  const match = INTERVAL_RE.exec(config.trim().toLowerCase());
  if (!match) return null;
  const value = Number.parseInt(match[1], 10);
  if (!Number.isFinite(value) || value <= 0) return null;
  const unit = match[2] ?? "s";
  if (unit === "m") return value * 60;
  if (unit === "h") return value * 3600;
  if (unit === "d") return value * 86_400;
  return value;
}

/**
 * Picks the largest time-unit bucket the seconds value fits into evenly.
 * Falls back to seconds.
 */
function pickIntervalBucket(
  seconds: number,
): { count: number; unit: "d" | "h" | "m" | "s" } {
  if (seconds >= 86_400 && seconds % 86_400 === 0) {
    return { count: seconds / 86_400, unit: "d" };
  }
  if (seconds >= 3600 && seconds % 3600 === 0) {
    return { count: seconds / 3600, unit: "h" };
  }
  if (seconds >= 60 && seconds % 60 === 0) {
    return { count: seconds / 60, unit: "m" };
  }
  return { count: seconds, unit: "s" };
}

const INTERVAL_LABELS: Record<
  "d" | "h" | "m" | "s",
  Record<Locale, (n: number, plural: string) => string>
> = {
  d: {
    fr: (n, p) => `Tous les ${n} jour${p}`,
    en: (n, p) => `Every ${n} day${p}`,
  },
  h: {
    fr: (n, p) => `Toutes les ${n} heure${p}`,
    en: (n, p) => `Every ${n} hour${p}`,
  },
  m: {
    fr: (n) => `Toutes les ${n} minutes`,
    en: (n) => `Every ${n} minutes`,
  },
  s: {
    fr: (n, p) => `Toutes les ${n} seconde${p}`,
    en: (n, p) => `Every ${n} second${p}`,
  },
};

function humanizeInterval(config: string, loc: Locale): string | null {
  const seconds = intervalToSeconds(config);
  if (seconds === null) return null;
  const { count, unit } = pickIntervalBucket(seconds);
  const plural = count > 1 ? "s" : "";
  return INTERVAL_LABELS[unit][loc](count, plural);
}

function extractWebhookSource(config: string): string | null {
  const trimmed = config.trim();
  if (!trimmed) return null;
  // Accept JSON {"path": "...", "source": "..."} or a plain string.
  try {
    const parsed = JSON.parse(trimmed) as Record<string, unknown>;
    const source = parsed.source ?? parsed.name ?? parsed.path;
    if (typeof source === "string" && source.length > 0) return source;
  } catch {
    // Not JSON - return first whitespace-free token.
  }
  const token = trimmed.split(/\s+/)[0];
  return token.length > 0 ? token : null;
}

/**
 * Builds the localised "custom schedule" fallback label.
 */
function customScheduleLabel(loc: Locale): { label: string; isCustom: true } {
  return {
    label: loc === "fr" ? "Planification personnalisée" : "Custom schedule",
    isCustom: true,
  };
}

/**
 * Builds the localised webhook label, with or without an explicit source.
 */
function webhookLabel(source: string | null, loc: Locale): string {
  if (source) {
    return loc === "fr"
      ? `Quand Apollia reçoit un webhook depuis ${source}`
      : `When Apollia receives a webhook from ${source}`;
  }
  return loc === "fr"
    ? "Quand Apollia reçoit un webhook"
    : "When Apollia receives a webhook";
}

/**
 * Turns a trigger definition into operator-friendly language.
 *
 * Returns `{ label, isCustom }` - when `isCustom` is `true`, the caller
 * should surface the raw expression through a tooltip.
 */
export function humanizeSchedule(
  kind: ScheduleKind,
  config: string,
  locale: string,
): { label: string; isCustom: boolean } {
  const loc: Locale = locale.startsWith("fr") ? "fr" : "en";

  switch (kind) {
    case "cron": {
      const label = humanizeCron(config, loc);
      return label ? { label, isCustom: false } : customScheduleLabel(loc);
    }
    case "interval": {
      const label = humanizeInterval(config, loc);
      return label ? { label, isCustom: false } : customScheduleLabel(loc);
    }
    case "file_watch": {
      const path = config.trim() || (loc === "fr" ? "un chemin" : "a path");
      return {
        label: loc === "fr" ? `Quand ${path} change` : `When ${path} changes`,
        isCustom: false,
      };
    }
    case "webhook":
      return {
        label: webhookLabel(extractWebhookSource(config), loc),
        isCustom: false,
      };
    case "oneshot":
      return {
        label: loc === "fr" ? "Une seule fois" : "One time only",
        isCustom: false,
      };
    default:
      return customScheduleLabel(loc);
  }
}

/**
 * Converts an interval expression to milliseconds. Returns null when the
 * expression is malformed.
 */
function intervalToMs(config: string): number | null {
  const seconds = intervalToSeconds(config);
  return seconds === null ? null : seconds * 1000;
}

/**
 * Estimates the next run from a simple daily/weekday cron pattern.
 * Returns null for any expression not also covered by `humanizeCron`.
 */
function estimateCronNextRun(config: string, now: Date): Date | null {
  const parts = config.trim().split(/\s+/);
  if (parts.length < 5) return null;
  const [minuteRaw, hourRaw, dayOfMonth, month, dayOfWeek] = parts;
  if (dayOfMonth !== "*" || month !== "*") return null;
  if (!ONE_OR_TWO_DIGITS.test(minuteRaw) || !ONE_OR_TWO_DIGITS.test(hourRaw)) return null;
  const minute = Number.parseInt(minuteRaw, 10);
  const hour = Number.parseInt(hourRaw, 10);
  const allowed = parseDowRange(dayOfWeek);
  if (!allowed) return null;

  const candidate = new Date(now);
  candidate.setSeconds(0, 0);
  candidate.setHours(hour, minute, 0, 0);
  for (let i = 0; i < 8; i++) {
    if (candidate.getTime() > now.getTime() && allowed.has(candidate.getDay())) {
      return candidate;
    }
    candidate.setDate(candidate.getDate() + 1);
  }
  return null;
}

/**
 * Estimates the next run time from a cron or interval expression.
 *
 * Returns a Date or null. Only handles patterns also covered by
 * `humanizeSchedule()`; complex expressions yield null so the UI can
 * fall back to "Next run scheduled".
 */
export function estimateNextRun(
  kind: ScheduleKind,
  config: string,
  lastFired: Date | null,
  now: Date = new Date(),
): Date | null {
  if (kind === "interval") {
    const ms = intervalToMs(config);
    if (ms === null) return null;
    const base = lastFired ? lastFired.getTime() : now.getTime();
    const next = new Date(base + ms);
    return next.getTime() > now.getTime() ? next : new Date(now.getTime() + ms);
  }

  if (kind === "cron") {
    return estimateCronNextRun(config, now);
  }

  return null;
}

function parseDowRange(spec: string): Set<number> | null {
  if (spec === "*") return new Set([0, 1, 2, 3, 4, 5, 6]);
  if (spec === "1-5") return new Set([1, 2, 3, 4, 5]);
  if (spec === "0,6" || spec === "6,0") return new Set([0, 6]);
  if (DOW_SINGLE.test(spec)) return new Set([Number.parseInt(spec, 10)]);
  return null;
}

/**
 * Renders a localised "next run" countdown for the given diff (ms).
 * Returns null when the diff doesn't match this bucket so the caller can
 * fall through to the next granularity.
 */
function formatMinutesBucket(diffMs: number, loc: Locale): string | null {
  const minutes = Math.floor(diffMs / 60_000);
  if (minutes < 1) {
    return loc === "fr" ? "Prochaine exécution dans <1 min" : "Next run in <1 min";
  }
  if (minutes < 60) {
    return loc === "fr" ? `Prochaine exécution dans ${minutes} min` : `Next run in ${minutes} min`;
  }
  return null;
}

/**
 * Renders a natural-language countdown: "Next run in 2h34m",
 * "Next run tomorrow at 08:00". Past dates render as "Overdue".
 */
export function formatNextRun(next: Date | null, locale: string, now: Date = new Date()): string {
  const loc: Locale = locale.startsWith("fr") ? "fr" : "en";
  if (!next) {
    return loc === "fr" ? "Prochaine exécution planifiée" : "Next run scheduled";
  }
  const diffMs = next.getTime() - now.getTime();
  if (diffMs <= 0) {
    return loc === "fr" ? "En retard" : "Overdue";
  }
  const minutesLabel = formatMinutesBucket(diffMs, loc);
  if (minutesLabel) return minutesLabel;

  const minutes = Math.floor(diffMs / 60_000);
  const hours = Math.floor(minutes / 60);
  const remMin = minutes % 60;
  if (hours < 24) {
    return loc === "fr"
      ? `Prochaine exécution dans ${hours}h${pad(remMin)}`
      : `Next run in ${hours}h${pad(remMin)}`;
  }
  const days = Math.floor(hours / 24);
  const plural = days > 1 ? "s" : "";
  return loc === "fr"
    ? `Prochaine exécution dans ${days} jour${plural}`
    : `Next run in ${days} day${plural}`;
}

/**
 * Returns the success rate percentage (0–100). Falls back to 100 when
 * no runs have been recorded yet.
 */
export function computeSuccessRate(fireCount: number, skipCount: number): number {
  const total = fireCount + skipCount;
  if (total === 0) return 100;
  return Math.round((fireCount / total) * 100);
}
