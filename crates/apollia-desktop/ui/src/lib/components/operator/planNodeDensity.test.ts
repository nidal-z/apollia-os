import { describe, it, expect } from "vitest";
import { nodeFields } from "./planNodeDensity";

describe("nodeFields", () => {
  it("Operator hides every detail field", () => {
    // GIVEN the operator mode
    // WHEN deriving field visibility
    // THEN description, dependencies, hints, rationale and reason are hidden
    const f = nodeFields("operator");
    expect(f.showDescription).toBe(false);
    expect(f.showDependencies).toBe(false);
    expect(f.showHints).toBe(false);
    expect(f.showRationale).toBe(false);
    expect(f.showReason).toBe(false);
  });

  it("Builder exposes every detail field", () => {
    // GIVEN the builder mode
    // WHEN deriving field visibility
    // THEN every field is visible
    const f = nodeFields("builder");
    expect(f.showDescription).toBe(true);
    expect(f.showDependencies).toBe(true);
    expect(f.showHints).toBe(true);
    expect(f.showRationale).toBe(true);
    expect(f.showReason).toBe(true);
  });
});
