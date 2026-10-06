import { describe, test, expect, beforeAll } from "vitest";
import { locale, waitLocale } from "svelte-i18n";
import "./i18n/index";
import { formatCost } from "./format";

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
