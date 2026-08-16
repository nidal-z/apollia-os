import { describe, test, expect } from "vitest";
import { toStringArray, formatDateLocal } from "./SmartOutput.svelte";

describe("toStringArray", () => {
  test("an array of objects serializes readably, never [object Object]", () => {
    // GIVEN a tool output field holding an array of objects
    const value = [{ label: "Revenue", value: 12 }, { label: "Costs" }];
    // WHEN the list renderer coerces it to strings
    const items = toStringArray(value);
    // THEN every item is readable JSON and none collapses to [object Object]
    expect(items).toEqual(['{"label":"Revenue","value":12}', '{"label":"Costs"}']);
    for (const item of items) {
      expect(item).not.toContain("[object Object]");
    }
  });

  test("a lone object serializes readably too", () => {
    // GIVEN a non-array object value
    const value = { status: "done" };
    // WHEN it is coerced
    const items = toStringArray(value);
    // THEN it renders as JSON, not [object Object]
    expect(items).toEqual(['{"status":"done"}']);
  });

  test("strings and primitives pass through unchanged", () => {
    // GIVEN primitive values
    // WHEN they are coerced
    // THEN they keep their plain string form
    expect(toStringArray("alpha")).toEqual(["alpha"]);
    expect(toStringArray([1, "two", true])).toEqual(["1", "two", "true"]);
  });
});

describe("formatDateLocal", () => {
  test("the display locale governs the rendered date", () => {
    // GIVEN one ISO date and two application languages
    // Noon keeps the calendar day stable in every timezone the test runs in.
    const iso = "2026-03-01T12:00:00";
    // WHEN it is formatted under each language
    const english = formatDateLocal(iso, "en");
    const french = formatDateLocal(iso, "fr");
    // THEN the rendering follows the language passed in, not the OS locale
    expect(english).toBe("March 1, 2026");
    expect(french).toBe("1 mars 2026");
  });

  test("an unparseable value returns unchanged", () => {
    // GIVEN a value that is not a date
    const value = "not-a-date";
    // WHEN it is formatted
    const rendered = formatDateLocal(value, "en");
    // THEN the original string comes back
    expect(rendered).toBe("not-a-date");
  });
});
