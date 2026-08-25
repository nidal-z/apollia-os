/**
 * Human-friendly labels for automation schedules.
 *
 * Operator vocabulary - never exposes cron digits, payload, or on-busy.
 * Every label is a catalogue key under `automations.humanize.*` plus its
 * interpolation values ([`ScheduleLabel`]); the caller renders it through
 * `$t(label.key, { values: label.values })`. Falls back to the
 * custom-schedule key with the raw config moved to a tooltip by the caller.
 *
 * The `locale` argument only drives locale-specific *formatting* of the
 * interpolated values (clock style, day names); the copy itself lives in
 * `en.json` / `fr.json`.
 */

export type ScheduleKind = "cron" | "interval" | "file_watch" | "webhook" | "oneshot";
export type Locale = "en" | "fr";

/** A catalogue key plus the values its message interpolates. */
export interface ScheduleLabel {
  key: string;
  values?: Record<string, string | number>;
}

const BASE = "automations.humanize";

function label(
  suffix: string,
  values?: Record<string, string | number>,
): ScheduleLabel {
  return values ? { key: `${BASE}.${suffix}`, values } : { key: `${BASE}.${suffix}` };
}

const CRON_EXACT: Record<string, string> = {
  "* * * * *": "cron_every_minute",
  "0 * * * *": "cron_hourly",
  "0 0 * * *": "cron_daily_midnight",
  "0 12 * * *": "cron_daily_noon",
  "0 0 * * 0": "cron_sunday_midnight",
  "0 0 * * 1": "cron_monday_midnight",
  "0 0 1 * *": "cron_first_of_month",
};

/** Day-of-week catalogue key suffixes, indexed by cron day number. */
const DOW_KEYS = [
  "dow_sunday",
  "dow_monday",
  "dow_tuesday",
  "dow_wednesday",
  "dow_thursday",
  "dow_friday",
  "dow_saturday",
] as const;

const INTERVAL_RE = /^(\d+)\s*([smhd])?$/;
const STEP_RE = /^\*\/\d+$/;
const ONE_OR_TWO_DIGITS = /^\d{1,2}$/;
const DOW_SINGLE = /^[0-6]$/;

function pad(n: string | number): string {
  return String(n).padStart(2, "0");
}

/**
 * Drops the leading seconds field from a 6-field cron expression when that
 * field is `0`, which is the form the runtime persists for a 5-field
 * expression. Display helpers and the schedule builder reason in 5 fields.
 */
export function stripSecondsField(expr: string): string {
  const parts = expr.trim().split(/\s+/);
  return parts.length === 6 && parts[0] === "0" ? parts.slice(1).join(" ") : expr;
}

/**
 * Formats a hour/minute pair in the operator vocabulary of the locale:
 * `8h` / `8h30` in French (minutes dropped when zero), `08:00` / `08:30`
 * elsewhere. Formatting, not copy: the sentence around it is a catalogue key.
 */
function clock(hour: number, minute: number, loc: Locale): string {
  if (loc === "fr") {
    return minute === 0 ? `${hour}h` : `${hour}h${pad(minute)}`;
  }
  return `${pad(hour)}:${pad(minute)}`;
}

/**
 * Recognises `*\/N * * * *` (every N minutes) and returns the label,
 * or null when the expression doesn't match.
 */
function matchEveryNMinutes(parts: readonly string[]): ScheduleLabel | null {
  const [minute, hour, dayOfMonth, month, dayOfWeek] = parts;
  if (!STEP_RE.test(minute) || hour !== "*" || dayOfMonth !== "*" || month !== "*" || dayOfWeek !== "*") {
    return null;
  }
  const n = Number.parseInt(minute.slice(2), 10);
  if (n <= 0) return null;
  return label("every_n_minutes", { n });
}

/**
 * Recognises `0 *\/N * * *` (every N hours) and returns the label,
 * or null when the expression doesn't match.
 */
function matchEveryNHours(parts: readonly string[]): ScheduleLabel | null {
  const [minute, hour, dayOfMonth, month, dayOfWeek] = parts;
  if (minute !== "0" || !STEP_RE.test(hour) || dayOfMonth !== "*" || month !== "*" || dayOfWeek !== "*") {
    return null;
  }
  const n = Number.parseInt(hour.slice(2), 10);
  if (n <= 0) return null;
  return label("every_n_hours", { n });
}

/**
 * Recognises `M H * * <dow-spec>` patterns: weekdays (`1-5`), weekends
 * (`0,6` / `6,0`), single day-of-week (`0..6`), or all days (`*`).
 */
function matchDailyWithDow(
  parts: readonly string[],
  loc: Locale,
): ScheduleLabel | null {
  const [minute, hour, dayOfMonth, month, dayOfWeek] = parts;
  if (!ONE_OR_TWO_DIGITS.test(minute) || !ONE_OR_TWO_DIGITS.test(hour)) return null;
  if (dayOfMonth !== "*" || month !== "*") return null;

  const hRaw = Number.parseInt(hour, 10);
  const mRaw = Number.parseInt(minute, 10);
  const time = clock(hRaw, mRaw, loc);

  if (dayOfWeek === "1-5") return label("weekday_at", { time });
  if (dayOfWeek === "0,6" || dayOfWeek === "6,0") return label("weekend_at", { time });
  if (DOW_SINGLE.test(dayOfWeek)) {
    const d = Number.parseInt(dayOfWeek, 10);
    return { key: `${BASE}.${DOW_KEYS[d]}`, values: { time } };
  }
  if (dayOfWeek === "*") return label("daily_at", { time });
  return null;
}

/**
 * Recognises `0 H D * *` (monthly on the Dth at H:00).
 */
function matchMonthly(parts: readonly string[], loc: Locale): ScheduleLabel | null {
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
  return label("monthly_at", { day: d, time: clock(hRaw, 0, loc) });
}

function humanizeCron(expr: string, loc: Locale): ScheduleLabel | null {
  const trimmed = stripSecondsField(expr).trim();
  const exact = CRON_EXACT[trimmed];
  if (exact) return label(exact);

  const parts = trimmed.split(/\s+/);
  if (parts.length < 5) return null;

  return (
    matchEveryNMinutes(parts) ??
    matchEveryNHours(parts) ??
    matchDailyWithDow(parts, loc) ??
    matchMonthly(parts, loc)
  );
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

/** Catalogue key suffix per interval bucket; ICU plural rules do the rest. */
const INTERVAL_KEYS: Record<"d" | "h" | "m" | "s", string> = {
  d: "every_n_days",
  h: "every_n_hours",
  m: "every_n_minutes",
  s: "every_n_seconds",
};

function humanizeInterval(config: string): ScheduleLabel | null {
  const seconds = intervalToSeconds(config);
  if (seconds === null) return null;
  const { count, unit } = pickIntervalBucket(seconds);
  return label(INTERVAL_KEYS[unit], { n: count });
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
 * Builds the "custom schedule" fallback label.
 */
function customScheduleLabel(): { label: ScheduleLabel; isCustom: true } {
  return { label: label("custom_schedule"), isCustom: true };
}

/**
 * Builds the webhook label, with or without an explicit source.
 */
function webhookLabel(source: string | null): ScheduleLabel {
  return source ? label("webhook_from", { source }) : label("webhook");
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
): { label: ScheduleLabel; isCustom: boolean } {
  const loc: Locale = locale.startsWith("fr") ? "fr" : "en";

  switch (kind) {
    case "cron": {
      const cronLabel = humanizeCron(config, loc);
      return cronLabel ? { label: cronLabel, isCustom: false } : customScheduleLabel();
    }
    case "interval": {
      const intervalLabel = humanizeInterval(config);
      return intervalLabel
        ? { label: intervalLabel, isCustom: false }
        : customScheduleLabel();
    }
    case "file_watch": {
      const path = config.trim();
      return {
        label: path
          ? label("file_watch_changes", { path })
          : label("file_watch_changes_default"),
        isCustom: false,
      };
    }
    case "webhook":
      return {
        label: webhookLabel(extractWebhookSource(config)),
        isCustom: false,
      };
    case "oneshot":
      return { label: label("oneshot"), isCustom: false };
    default:
      return customScheduleLabel();
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
  const parts = stripSecondsField(config).trim().split(/\s+/);
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
 * fall back to the "next run scheduled" label.
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
 * Renders the "next run" countdown label for the given diff (ms).
 * Returns null when the diff doesn't match this bucket so the caller can
 * fall through to the next granularity.
 */
function formatMinutesBucket(diffMs: number): ScheduleLabel | null {
  const minutes = Math.floor(diffMs / 60_000);
  if (minutes < 1) return label("next_run_under_minute");
  if (minutes < 60) return label("next_run_minutes", { n: minutes });
  return null;
}

/**
 * Builds a natural-language countdown label: "Next run in 2h34m",
 * "Next run in 3 days". Past dates yield the "overdue" key.
 */
export function formatNextRun(
  next: Date | null,
  _locale: string,
  now: Date = new Date(),
): ScheduleLabel {
  if (!next) return label("next_run_scheduled");
  const diffMs = next.getTime() - now.getTime();
  if (diffMs <= 0) return label("overdue");
  const minutesLabel = formatMinutesBucket(diffMs);
  if (minutesLabel) return minutesLabel;

  const minutes = Math.floor(diffMs / 60_000);
  const hours = Math.floor(minutes / 60);
  const remMin = minutes % 60;
  if (hours < 24) {
    return label("next_run_hours", { time: `${hours}h${pad(remMin)}` });
  }
  const days = Math.floor(hours / 24);
  return label("next_run_days", { n: days });
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
