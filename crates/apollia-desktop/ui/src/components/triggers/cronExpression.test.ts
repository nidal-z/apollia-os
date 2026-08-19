import { describe, it, expect } from "vitest";
import {
  WEEKLY_NO_DAY_ERROR_KEY,
  buildCronExpression,
  shiftMinutes,
  utcToLocal,
  type CronDraft,
} from "./cronExpression";

/** Mon-Fri, the state the builder opens on. */
const WEEKDAYS = [true, true, true, true, true, false, false];
const NO_DAY = [false, false, false, false, false, false, false];

function draft(over: Partial<CronDraft> = {}): CronDraft {
  return {
    preset: "weekly",
    dailyTime: "08:00",
    weeklyTime: "08:00",
    weeklyDays: [...WEEKDAYS],
    rawCron: "",
    ...over,
  };
}

describe("buildCronExpression, weekly preset", () => {
  it("emits nothing and names the reason when no day is ticked", () => {
    // GIVEN a weekly schedule whose seven day chips are all off
    const d = draft({ weeklyDays: [...NO_DAY] });

    // WHEN the expression is built
    const result = buildCronExpression(d, 0);

    // THEN no expression is emitted, and the caller is handed the reason.
    // Before this lot the builder fell back to "0" and emitted "0 8 * * 0",
    // a Sunday schedule nobody chose.
    expect(result.expr).toBe("");
    expect(result.errorKey).toBe(WEEKLY_NO_DAY_ERROR_KEY);
  });

  it("emits nothing whatever the offset, so no timezone hides the empty day set", () => {
    // GIVEN no day ticked, and a zone whose conversion crosses midnight
    const d = draft({ weeklyDays: [...NO_DAY], weeklyTime: "00:30" });

    // WHEN the expression is built west of UTC and east of UTC
    const west = buildCronExpression(d, 300);
    const east = buildCronExpression(d, -600);

    // THEN neither produces an expression
    expect(west.expr).toBe("");
    expect(east.expr).toBe("");
  });

  it("keeps the ticked days when at least one is on", () => {
    // GIVEN Monday and Wednesday ticked, at 08:00 in UTC
    const d = draft({ weeklyDays: [true, false, true, false, false, false, false] });

    // WHEN the expression is built
    const result = buildCronExpression(d, 0);

    // THEN both days travel, and no reason is reported
    expect(result.expr).toBe("0 8 * * 1,3");
    expect(result.errorKey).toBeNull();
  });

  it("shifts the weekday when the conversion to UTC crosses midnight", () => {
    // GIVEN Monday at 23:30 local, five hours west of UTC
    const d = draft({
      weeklyDays: [true, false, false, false, false, false, false],
      weeklyTime: "23:30",
    });

    // WHEN the expression is built
    const result = buildCronExpression(d, 300);

    // THEN it fires Tuesday 04:30 UTC
    expect(result.expr).toBe("30 4 * * 2");
  });

  it("wraps Sunday to Saturday when the conversion moves a day back", () => {
    // GIVEN Sunday at 00:30 local, ten hours east of UTC
    const d = draft({
      weeklyDays: [false, false, false, false, false, false, true],
      weeklyTime: "00:30",
    });

    // WHEN the expression is built
    const result = buildCronExpression(d, -600);

    // THEN Sunday 0 becomes Saturday 6
    expect(result.expr).toBe("30 14 * * 6");
  });
});

describe("buildCronExpression, other presets", () => {
  it("emits the fixed expressions unchanged", () => {
    // GIVEN the three fixed-interval presets
    // WHEN each is built
    // THEN the expression is the literal the preset stands for
    expect(buildCronExpression(draft({ preset: "15m" }), 0).expr).toBe("*/15 * * * *");
    expect(buildCronExpression(draft({ preset: "30m" }), 0).expr).toBe("*/30 * * * *");
    expect(buildCronExpression(draft({ preset: "hourly" }), 0).expr).toBe("0 * * * *");
  });

  it("converts the daily time to UTC and ignores the day chips", () => {
    // GIVEN a daily schedule at 08:00 local, five hours west of UTC, no day ticked
    const d = draft({ preset: "daily", dailyTime: "08:00", weeklyDays: [...NO_DAY] });

    // WHEN the expression is built
    const result = buildCronExpression(d, 300);

    // THEN the daily path is untouched by the day chips
    expect(result.expr).toBe("0 13 * * *");
    expect(result.errorKey).toBeNull();
  });

  it("hands back the raw expression on the custom preset", () => {
    // GIVEN a hand-typed expression
    const d = draft({ preset: "custom", rawCron: "*/7 3 * * 5" });

    // WHEN the expression is built
    const result = buildCronExpression(d, 300);

    // THEN it travels verbatim, no conversion applied
    expect(result.expr).toBe("*/7 3 * * 5");
  });
});

describe("shiftMinutes", () => {
  it("reports the day it landed on in both directions", () => {
    // GIVEN times that cross midnight forwards and backwards
    // WHEN they are shifted
    // THEN the day delta carries the crossing
    expect(shiftMinutes(23, 30, 60)).toEqual({ hh: 0, mm: 30, dayDelta: 1 });
    expect(shiftMinutes(0, 30, -60)).toEqual({ hh: 23, mm: 30, dayDelta: -1 });
    expect(shiftMinutes(8, 0, 0)).toEqual({ hh: 8, mm: 0, dayDelta: 0 });
  });

  it("is the inverse of itself through utcToLocal", () => {
    // GIVEN 13:00 UTC read back five hours west of UTC
    // WHEN converted to local
    const local = utcToLocal(13, 0, 300);

    // THEN the operator reads the 08:00 they typed
    expect(local).toEqual({ hh: 8, mm: 0, dayDelta: 0 });
  });
});
