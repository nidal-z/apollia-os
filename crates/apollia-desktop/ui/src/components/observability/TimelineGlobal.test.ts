import { describe, test, expect, beforeAll, afterAll } from "vitest";

// The component's import graph reaches the theme store, which reads
// localStorage at import time; vitest runs in `node`, so stub it first.
if (!("localStorage" in globalThis)) {
  Object.defineProperty(globalThis, "localStorage", {
    value: { getItem: (_key: string) => null, setItem: () => {}, removeItem: () => {} },
  });
}
const { localDayKey, dayKeyOf, dayGroupKind } = await import("./TimelineGlobal.svelte");

// The timeline groups events by day and marks the today/yesterday groups.
// These tests run under a timezone shifted from UTC so a UTC calendar cannot
// pass by coincidence.

const SAVED_TZ = process.env.TZ;

beforeAll(() => {
  process.env.TZ = "Europe/Paris";
});

afterAll(() => {
  if (SAVED_TZ === undefined) delete process.env.TZ;
  else process.env.TZ = SAVED_TZ;
});

describe("TimelineGlobal - day grouping keys", () => {
  test("an event at 01:00 local files under the local day, not the UTC day", () => {
    // GIVEN an event on the wire at 23:00 UTC on the 14th, which is already
    // 01:00 on the 15th in Europe/Paris (UTC+2 in August)
    const timestamp = "2026-08-14T23:00:00Z";

    // WHEN computing its group key
    const key = dayKeyOf(timestamp);

    // THEN the key is the local calendar day
    expect(key).toBe("2026-08-15");
  });

  test("an unparsable timestamp keeps its raw date prefix as key", () => {
    // GIVEN a timestamp the Date constructor rejects
    // WHEN computing its group key
    // THEN the raw prefix is kept rather than a NaN artifact
    expect(dayKeyOf("not-a-timestamp")).toBe("not-a-time");
  });
});

describe("TimelineGlobal - today and yesterday markers", () => {
  // GIVEN a session running at 10:00 local on 2026-08-15 in Europe/Paris
  const now = new Date("2026-08-15T08:00:00Z");

  test("this morning's 01:00 event is marked today, not yesterday", () => {
    // GIVEN the group key of an event at 01:00 local today (23:00 UTC the 14th)
    const key = dayKeyOf("2026-08-14T23:00:00Z");

    // WHEN classifying the group against the local calendar
    const kind = dayGroupKind(key, now);

    // THEN it belongs to today
    expect(kind).toBe("today");
  });

  test("an event from the local day before is marked yesterday", () => {
    // GIVEN the group key of an event at 23:00 local on the 14th (21:00 UTC)
    const key = dayKeyOf("2026-08-14T21:00:00Z");

    // WHEN classifying the group
    const kind = dayGroupKind(key, now);

    // THEN it belongs to yesterday
    expect(kind).toBe("yesterday");
  });

  test("older days fall through to the dated label", () => {
    // GIVEN a group key two local days back
    // WHEN classifying it
    // THEN neither marker claims it
    expect(dayGroupKind("2026-08-13", now)).toBe("other");
  });

  test("the local day key of the session clock is the local calendar day", () => {
    // GIVEN the session clock above, 00:30 local on the 15th expressed in UTC
    const justPastMidnight = new Date("2026-08-14T22:30:00Z");

    // WHEN reading its local day key
    // THEN it is the 15th, where a UTC reading would say the 14th
    expect(localDayKey(justPastMidnight)).toBe("2026-08-15");
  });
});
