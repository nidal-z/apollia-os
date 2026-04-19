import { describe, it, expect } from "vitest";
import { shouldVirtualize, VIRTUALIZATION_THRESHOLD } from "./virtualList";

describe("shouldVirtualize", () => {
  // GIVEN a short conversation
  // WHEN under the threshold
  // THEN virtualization is off (direct render path)
  it("returns false below threshold", () => {
    expect(shouldVirtualize(0)).toBe(false);
    expect(shouldVirtualize(VIRTUALIZATION_THRESHOLD)).toBe(false);
    expect(shouldVirtualize(VIRTUALIZATION_THRESHOLD - 1)).toBe(false);
  });

  // GIVEN a long conversation
  // WHEN count exceeds threshold
  // THEN virtualization kicks in
  it("returns true above threshold", () => {
    expect(shouldVirtualize(VIRTUALIZATION_THRESHOLD + 1)).toBe(true);
    expect(shouldVirtualize(1000)).toBe(true);
  });
});
