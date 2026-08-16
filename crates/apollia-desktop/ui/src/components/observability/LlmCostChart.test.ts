import { describe, test, expect, beforeAll, afterAll } from "vitest";

// The component's import graph reaches the theme store, which reads
// localStorage at import time; vitest runs in `node`, so stub it first.
if (!("localStorage" in globalThis)) {
  Object.defineProperty(globalThis, "localStorage", {
    value: { getItem: (_key: string) => null, setItem: () => {}, removeItem: () => {} },
  });
}
const { buildDateAxis } = await import("./LlmCostChart.svelte");

// The cost chart's X axis is a run of local calendar days ending today. These
// tests run under a timezone shifted from UTC so a UTC axis cannot pass by
// coincidence.

const SAVED_TZ = process.env.TZ;

beforeAll(() => {
  process.env.TZ = "Europe/Paris";
});

afterAll(() => {
  if (SAVED_TZ === undefined) delete process.env.TZ;
  else process.env.TZ = SAVED_TZ;
});

describe("LlmCostChart - buildDateAxis", () => {
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
