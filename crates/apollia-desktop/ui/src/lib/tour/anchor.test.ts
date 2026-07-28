import { describe, expect, it } from "vitest";
import { resolveIndex, selectorFor } from "./anchor";
import type { StepAnchor } from "./types";

describe("tour anchor - selectorFor", () => {
  it("matches an exact testid", () => {
    // GIVEN an exact anchor
    const anchor: StepAnchor = { kind: "testid", value: "topbar-search" };
    // WHEN the selector is built
    const selector = selectorFor(anchor);
    // THEN it is an equality match
    expect(selector).toBe('[data-testid="topbar-search"]');
  });

  it("matches a testid prefix", () => {
    // GIVEN a prefix anchor, as approval cards need since their testid carries
    // the tool name
    const anchor: StepAnchor = { kind: "testidPrefix", value: "operator-approval-" };
    // WHEN the selector is built
    const selector = selectorFor(anchor);
    // THEN it is a starts-with match
    expect(selector).toBe('[data-testid^="operator-approval-"]');
  });
});

describe("tour anchor - resolveIndex", () => {
  it("returns a positive index unchanged", () => {
    // GIVEN a forward index over three matches
    // WHEN resolved
    const index = resolveIndex(1, 3);
    // THEN it is used as-is
    expect(index).toBe(1);
  });

  it("counts a negative index from the end", () => {
    // GIVEN -1 over three matches, the convention the automation scripts use
    // WHEN resolved
    const index = resolveIndex(-1, 3);
    // THEN it points at the last match
    expect(index).toBe(2);
  });

  it("yields an out-of-range index rather than clamping", () => {
    // GIVEN an index beyond the match count
    // WHEN resolved
    const index = resolveIndex(-5, 2);
    // THEN it stays out of range so the caller's `item()` returns null, instead
    // of silently anchoring on the wrong element
    expect(index).toBe(-3);
  });
});
