import { describe, test, expect, beforeAll, afterAll } from "vitest";

// The component's import graph reaches the theme store, which reads
// localStorage at import time; vitest runs in `node`, so stub it first.
if (!("localStorage" in globalThis)) {
  Object.defineProperty(globalThis, "localStorage", {
    value: { getItem: (_key: string) => null, setItem: () => {}, removeItem: () => {} },
  });
}
const { buildDateAxis, summarizeWindow } = await import("./LlmCostChart.svelte");

const SAVED_TZ = process.env.TZ;

// The cost chart's X axis is a run of local calendar days ending today. This
// block pins one timezone shifted from UTC so a UTC axis cannot pass by
// coincidence. The fold block below deliberately does not: it runs under the
// timezone of the machine, and is meant to be replayed under both signs of
// offset (TZ=America/Los_Angeles and TZ=Europe/Paris).
describe("LlmCostChart - buildDateAxis", () => {
  beforeAll(() => {
    process.env.TZ = "Europe/Paris";
  });

  afterAll(() => {
    if (SAVED_TZ === undefined) delete process.env.TZ;
    else process.env.TZ = SAVED_TZ;
  });

  test("just past local midnight the axis already ends on the new local day", () => {
    // GIVEN a session at 00:30 local on 2026-08-15 in Europe/Paris, an instant
    // whose UTC date is still 2026-08-14
    const now = new Date("2026-08-14T22:30:00Z");

    // WHEN building a 7-day axis
    const axis = buildDateAxis(7, now);

    // THEN it ends on the local day, spans 7 days, and starts 6 days earlier
    expect(axis[axis.length - 1]).toBe("2026-08-15");
    expect(axis).toHaveLength(7);
    expect(axis[0]).toBe("2026-08-09");
  });

  test("in the afternoon the local and UTC days agree and the axis is contiguous", () => {
    // GIVEN a session at 16:00 local on 2026-08-15 (14:00 UTC)
    const now = new Date("2026-08-15T14:00:00Z");

    // WHEN building a 3-day axis
    const axis = buildDateAxis(3, now);

    // THEN the days are consecutive local dates ending today
    expect(axis).toEqual(["2026-08-13", "2026-08-14", "2026-08-15"]);
  });
});

// The two instants below straddle local midnight, one on each side of UTC:
// 2026-08-16T01:00:00Z is 18:00 on 2026-08-15 in America/Los_Angeles, and
// 2026-08-14T23:00:00Z is 01:00 on 2026-08-15 in Europe/Paris. Which of the
// two is the edge case depends on the sign of the machine's offset, so the
// tests pick it from the ambient timezone rather than pinning one.
const WEST_OF_UTC_EDGE = "2026-08-16T01:00:00Z";
const EAST_OF_UTC_EDGE = "2026-08-14T23:00:00Z";

/** The imposed edge instant that straddles local midnight here. */
function edgeInstant(): Date {
  // getTimezoneOffset is the minutes to add to local time to reach UTC, so it
  // is positive west of UTC and negative east of it.
  const west = new Date(WEST_OF_UTC_EDGE);
  return west.getTimezoneOffset() > 0 ? west : new Date(EAST_OF_UTC_EDGE);
}

function sumOfBars(days: { total: number }[]): number {
  return days.reduce((acc, day) => acc + day.total, 0);
}

describe("LlmCostChart - summarizeWindow", () => {
  test("a call made at the edge of local midnight is carried by the bar of its local day", () => {
    // GIVEN the edge instant for this machine's offset, and the daily rows the
    // runtime returns for a call made then plus one made four days earlier
    const now = edgeInstant();
    const axis = buildDateAxis(7, now);
    const localDay = axis[axis.length - 1];
    const rows = [
      { date: localDay, backend: "anthropic", cost_usd: 0.5 },
      { date: axis[3], backend: "openai", cost_usd: 0.25 },
    ];

    // WHEN folding the window onto the axis the chart draws
    const summary = summarizeWindow(rows, axis);

    // THEN the call sits on the bar of the day the operator made it, and the
    // Total tile is exactly the sum of the bars under it
    expect(summary.days[summary.days.length - 1].total).toBeCloseTo(0.5, 10);
    expect(summary.days[3].total).toBeCloseTo(0.25, 10);
    expect(summary.total).toBeCloseTo(0.75, 10);
    expect(summary.total).toBeCloseTo(sumOfBars(summary.days), 10);
  });

  test("a row outside the axis is in no bar and in no total", () => {
    // GIVEN a row dated the day before the axis opens, which a window counted
    // in 24-hour slices from now reaches into
    const now = edgeInstant();
    const axis = buildDateAxis(7, now);
    const dayBeforeAxis = buildDateAxis(8, now)[0];
    const rows = [
      { date: axis[6], backend: "anthropic", cost_usd: 1 },
      { date: dayBeforeAxis, backend: "anthropic", cost_usd: 5 },
    ];

    // WHEN folding
    const summary = summarizeWindow(rows, axis);

    // THEN the tile shows what the bars show, and the legend pill agrees
    expect(summary.total).toBeCloseTo(1, 10);
    expect(summary.total).toBeCloseTo(sumOfBars(summary.days), 10);
    expect(summary.byBackend["anthropic"]).toBeCloseTo(1, 10);
  });

  test("the total is the sum of the bars whatever the rows carry", () => {
    // GIVEN rows on several days and several backends, one of them off axis
    const now = edgeInstant();
    const axis = buildDateAxis(14, now);
    const rows = [
      { date: axis[0], backend: "anthropic", cost_usd: 0.12 },
      { date: axis[0], backend: "openai", cost_usd: 0.08 },
      { date: axis[13], backend: "openai", cost_usd: 0.4 },
      { date: buildDateAxis(15, now)[0], backend: "anthropic", cost_usd: 9.99 },
    ];

    // WHEN folding
    const summary = summarizeWindow(rows, axis);

    // THEN every reading of the card comes from that one fold
    expect(summary.total).toBeCloseTo(sumOfBars(summary.days), 10);
    expect(summary.byBackend["anthropic"] + summary.byBackend["openai"]).toBeCloseTo(
      summary.total,
      10,
    );
    expect(summary.backends).toEqual(["anthropic", "openai"]);
    expect(summary.days).toHaveLength(14);
  });
});
