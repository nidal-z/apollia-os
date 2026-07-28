import { describe, it, expect } from "vitest";
import { contentIn, contentOut } from "./routeTransition";
import { duration } from "./motion";

describe("routeTransition", () => {
  it("contentIn rises 6px and fades in on the base duration", () => {
    // GIVEN the route-content enter preset
    const params = contentIn();
    // THEN it flies a short distance with a fade
    expect(params.y).toBe(6);
    expect(params.opacity).toBe(0);
    expect(params.duration).toBe(duration.base);
    expect(typeof params.easing).toBe("function");
  });

  it("contentOut is shorter than contentIn to keep swaps snappy", () => {
    // GIVEN the leave preset
    // THEN it resolves on the fast duration
    expect(contentOut().duration).toBe(duration.fast);
  });
});
