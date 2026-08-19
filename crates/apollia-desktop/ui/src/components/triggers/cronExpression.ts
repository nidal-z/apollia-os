/**
 * Cron expression built by the presets of `CronBuilder`.
 *
 * Pure: no i18n, no IPC, no clock. The timezone offset is an argument, so the
 * component passes the operator's offset while a test pins a zone and reads a
 * stable expression. Validation returns a translation key and the caller
 * resolves it, the convention `automationDefinition.ts` already follows.
 *
 * The trigger engine evaluates cron expressions in UTC while the pickers show
 * local wall-clock time, so every build converts local to UTC and shifts the
 * weekday when the conversion crosses midnight. DST is approximated with the
 * offset handed in: a recurring schedule cannot encode a per-occurrence offset.
 */

/** Presets the builder can emit. */
export type CronPreset = "15m" | "30m" | "hourly" | "daily" | "weekly" | "custom";

/** Cron day-of-week for the Mon-first chip row: Mon 1 ... Sat 6, Sun 0. */
export const DAYS_CRON: readonly number[] = [1, 2, 3, 4, 5, 6, 0];

/** A wall-clock time plus the day it moved to when the shift crossed midnight. */
export interface ShiftedTime {
  hh: number;
  mm: number;
  dayDelta: number;
}

/** Moves `hh:mm` by `delta` minutes, reporting the day it landed on. */
export function shiftMinutes(hh: number, mm: number, delta: number): ShiftedTime {
  let total = hh * 60 + mm + delta;
  let dayDelta = 0;
  while (total < 0) {
    total += 1440;
    dayDelta -= 1;
  }
  while (total >= 1440) {
    total -= 1440;
    dayDelta += 1;
  }
  return { hh: Math.floor(total / 60), mm: total % 60, dayDelta };
}

/**
 * @param offsetMinutes minutes to add to local time to reach UTC, the sign
 * `Date.prototype.getTimezoneOffset` uses.
 */
function localToUtc(hh: number, mm: number, offsetMinutes: number): ShiftedTime {
  return shiftMinutes(hh, mm, offsetMinutes);
}

/**
 * Inverse of `localToUtc`, same sign convention for `offsetMinutes`. Exported
 * because the builder reads a stored expression back into its pickers.
 */
export function utcToLocal(hh: number, mm: number, offsetMinutes: number): ShiftedTime {
  return shiftMinutes(hh, mm, -offsetMinutes);
}

/** Everything the builder holds, flattened so the build stays pure. */
export interface CronDraft {
  preset: CronPreset;
  /** `HH:MM` local, used by the `daily` preset. */
  dailyTime: string;
  /** `HH:MM` local, used by the `weekly` preset. */
  weeklyTime: string;
  /** One flag per chip, Monday first, same order as {@link DAYS_CRON}. */
  weeklyDays: boolean[];
  /** Expression typed by hand, used by the `custom` preset. */
  rawCron: string;
}

/** Either the expression to emit, or the reason none could be built. */
export interface CronBuildResult {
  /** Expression to emit, empty when the draft cannot produce one. */
  expr: string;
  /** Translation key naming why nothing was built, `null` when `expr` stands. */
  errorKey: string | null;
}

/**
 * Reason rendered when the weekly preset has no day ticked.
 *
 * A weekly schedule without a day has no correct expression: the builder used
 * to fall back to `0`, which is Sunday, so an operator who unticked every chip
 * got an automation firing on a day nobody chose, with nothing said.
 *
 * The key names the missing day rather than a missing schedule:
 * `field_schedule_required` reads "the cron expression is required", which is
 * false here, the operator has a schedule and is missing a day.
 */
export const WEEKLY_NO_DAY_ERROR_KEY = "triggers.cron_weekly_no_day";

function built(expr: string): CronBuildResult {
  return { expr, errorKey: null };
}

/** Reads an `HH:MM` picker value. */
function readTime(value: string): { hh: number; mm: number } {
  const [hh, mm] = value.split(":").map(Number);
  return { hh, mm };
}

/** Builds the expression a draft stands for, in UTC. */
export function buildCronExpression(draft: CronDraft, offsetMinutes: number): CronBuildResult {
  switch (draft.preset) {
    case "15m":
      return built("*/15 * * * *");
    case "30m":
      return built("*/30 * * * *");
    case "hourly":
      return built("0 * * * *");
    case "daily": {
      const local = readTime(draft.dailyTime);
      const u = localToUtc(local.hh, local.mm, offsetMinutes);
      return built(`${u.mm} ${u.hh} * * *`);
    }
    case "weekly": {
      const local = readTime(draft.weeklyTime);
      const u = localToUtc(local.hh, local.mm, offsetMinutes);
      const activeDays = DAYS_CRON.filter((_, i) => draft.weeklyDays[i]).map(
        (d) => (((d + u.dayDelta) % 7) + 7) % 7,
      );
      if (activeDays.length === 0) {
        return { expr: "", errorKey: WEEKLY_NO_DAY_ERROR_KEY };
      }
      return built(`${u.mm} ${u.hh} * * ${activeDays.join(",")}`);
    }
    case "custom":
      return built(draft.rawCron);
  }
}
