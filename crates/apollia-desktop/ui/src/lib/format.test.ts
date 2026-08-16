import { describe, test, expect, beforeAll } from "vitest";
import { get } from "svelte/store";
import { locale, waitLocale } from "svelte-i18n";
import "./i18n/index";
import { formatCost, formatDayHeadline } from "./format";

beforeAll(async () => {
  locale.set("en");
  await waitLocale();
});

describe("formatCost", () => {
  test("zero renders $0.00 on every surface, task timeline included", () => {
    // GIVEN a zero USD cost
    const usd = 0;
    // WHEN it is formatted through the single shared formatter
    const rendered = formatCost(usd);
    // THEN it renders the plain form, not the $0.0000 the task timeline used to show
    expect(rendered).toBe("$0.00");
  });

  test("sub-cent costs keep four decimals", () => {
    // GIVEN a sub-cent USD cost
    const usd = 0.0004;
    // WHEN it is formatted
    const rendered = formatCost(usd);
    // THEN the four-decimal form keeps it readable
    expect(rendered).toBe("$0.0004");
  });

  test("ordinary costs round to the cent", () => {
    // GIVEN a cost at and above one cent
    // WHEN each is formatted
    // THEN both round to two decimals
    expect(formatCost(0.01)).toBe("$0.01");
    expect(formatCost(1.5)).toBe("$1.50");
  });
});

describe("formatDayHeadline", () => {
  const day = new Date(2026, 7, 15, 12, 0, 0);

  test("the application language governs the rendered date", async () => {
    // GIVEN the application language is English
    locale.set("en");
    await waitLocale();
    const english = formatDayHeadline(day, get(locale) ?? "en");
    // WHEN the user switches the application language to French
    locale.set("fr");
    await waitLocale();
    const french = formatDayHeadline(day, get(locale) ?? "en");
    // THEN the rendered date follows the switch instead of staying frozen
    expect(english).toContain("SATURDAY");
    expect(french).toContain("SAMEDI");
    expect(english).not.toBe(french);
  });

  test("an invalid date degrades to an empty headline", () => {
    // GIVEN a date the formatter cannot render
    const invalid = new Date(NaN);
    // WHEN it is formatted
    const rendered = formatDayHeadline(invalid, "en");
    // THEN the headline is empty rather than throwing
    expect(rendered).toBe("");
  });
});
