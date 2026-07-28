import { describe, it, expect } from "vitest";
import { listFlip, reorderFlip, rowIn, rowOut } from "./listMotion";
import { duration } from "./motion";

describe("listMotion", () => {
  it("listFlip uses the base duration and an easing function", () => {
    // GIVEN the default list flip preset
    // WHEN it is built
    const params = listFlip();
    // THEN it carries the base duration and a runtime easing function
    expect(params.duration).toBe(duration.base);
    expect(typeof params.easing).toBe("function");
  });

  it("reorderFlip is snappier than listFlip", () => {
    // GIVEN the reorder preset
    // WHEN built
    // THEN it uses the fast duration, shorter than the base flip
    expect(reorderFlip().duration).toBe(duration.fast);
    expect(duration.fast).toBeLessThan(duration.base);
  });

  it("rowIn rises 8px and fades in", () => {
    // GIVEN the row-enter preset
    const params = rowIn();
    // THEN it flies up with a fade
    expect(params.y).toBe(8);
    expect(params.opacity).toBe(0);
    expect(params.duration).toBe(duration.base);
  });

  it("rowOut is shorter than rowIn", () => {
    // GIVEN the row-leave preset
    // THEN removals resolve on the fast duration
    expect(rowOut().duration).toBe(duration.fast);
  });
});
