import { describe, test, expect, beforeAll, afterAll } from "vitest";

// The component's import graph reaches the theme store, which reads
// localStorage at import time; vitest runs in `node`, so stub it first.
if (!("localStorage" in globalThis)) {
  Object.defineProperty(globalThis, "localStorage", {
    value: { getItem: (_key: string) => null, setItem: () => {}, removeItem: () => {} },
  });
}
const { formatTime } = await import("./AuditSection.svelte");

// The audit log renders wall-clock times. These tests run under a timezone
// shifted from UTC so a UTC rendering cannot pass by coincidence.

const SAVED_TZ = process.env.TZ;

beforeAll(() => {
  process.env.TZ = "Europe/Paris";
});

afterAll(() => {
  if (SAVED_TZ === undefined) delete process.env.TZ;
  else process.env.TZ = SAVED_TZ;
});

describe("AuditSection - formatTime", () => {
  test("a decision taken at 01:30 local is logged at 01:30, not 23:30 the day before", () => {
    // GIVEN a decision recorded on the wire at 23:30 UTC, which is 01:30
    // in Europe/Paris (UTC+2 in August)
    const decidedAt = "2026-08-14T23:30:00+00:00";

    // WHEN formatting it for the audit column
    const rendered = formatTime(decidedAt, "fr");

    // THEN the hour is the machine's wall clock, not the UTC hour
    expect(rendered).toBe("01:30");
  });

  test("the 24h reading holds for the English locale too", () => {
    // GIVEN an afternoon decision, 14:05 UTC = 16:05 in Paris
    const decidedAt = "2026-08-14T14:05:00+00:00";

    // WHEN formatting it under the English catalogue
    const rendered = formatTime(decidedAt, "en");

    // THEN the fixed-width column keeps its h23 shape in every locale
    expect(rendered).toBe("16:05");
  });

  test("an unparsable timestamp passes through verbatim", () => {
    // GIVEN a value the Date constructor rejects
    // WHEN formatting it
    // THEN the raw value is shown rather than an Invalid Date artifact
    expect(formatTime("not-a-date", "en")).toBe("not-a-date");
  });
});
