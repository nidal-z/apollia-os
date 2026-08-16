import { describe, test, expect, beforeAll, afterAll } from "vitest";

// The component's import graph reaches the theme store, which reads
// localStorage at import time; vitest runs in `node`, so stub it first.
if (!("localStorage" in globalThis)) {
  Object.defineProperty(globalThis, "localStorage", {
    value: { getItem: (_key: string) => null, setItem: () => {}, removeItem: () => {} },
  });
}
const { formatDate } = await import("./PermissionRuleRow.svelte");

// Rule expiration and creation dates render as calendar dates. These tests run
// under a timezone shifted from UTC so a UTC rendering cannot pass by
// coincidence.

const SAVED_TZ = process.env.TZ;

beforeAll(() => {
  process.env.TZ = "Europe/Paris";
});

afterAll(() => {
  if (SAVED_TZ === undefined) delete process.env.TZ;
  else process.env.TZ = SAVED_TZ;
});

describe("PermissionRuleRow - formatDate", () => {
  test("a rule expiring today is not announced for yesterday", () => {
    // GIVEN an expiration recorded at 22:00 UTC on the 9th, which is already
    // the 10th in Europe/Paris (UTC+2 in August)
    const expiresAt = "2026-08-09T22:00:00+00:00";

    // WHEN formatting it under the English catalogue
    const rendered = formatDate(expiresAt, "en");

    // THEN the date is the machine's local calendar day
    expect(rendered).toBe("08/10/2026");
  });

  test("the same instant follows the French date order under the French catalogue", () => {
    // GIVEN the same wire instant
    const expiresAt = "2026-08-09T22:00:00+00:00";

    // WHEN formatting it under the French catalogue
    const rendered = formatDate(expiresAt, "fr");

    // THEN day and month swap to the locale's order, on the local day
    expect(rendered).toBe("10/08/2026");
  });

  test("an unparsable timestamp passes through verbatim", () => {
    // GIVEN a value the Date constructor rejects
    // WHEN formatting it
    // THEN the raw value is shown rather than an Invalid Date artifact
    expect(formatDate("never", "en")).toBe("never");
  });
});
