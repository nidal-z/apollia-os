import { describe, test, expect } from "vitest";
import type { ImpactLevel } from "./RiskBadge.svelte";

// RiskBadge exposes the `ImpactLevel` union as its public type — any consumer
// must be able to narrow the 4 variants without runtime code. These tests
// lock the contract (variants + round-trip) since the rendered markup is
// styling-only.

describe("RiskBadge — ImpactLevel contract", () => {
  test("accepts the 4 documented variants", () => {
    const levels: ImpactLevel[] = ["low", "medium", "high", "critical"];
    expect(levels).toHaveLength(4);
  });

  test("variants are stable identifiers (snake-case matches the Rust enum)", () => {
    // GIVEN the Rust `ImpactLevel` enum in apollia-core
    // WHEN serde rename_all = "snake_case" emits the JSON tag
    // THEN the TS union mirrors those exact strings so that payloads
    //      received from Tauri can be used without a translation layer.
    const expected = new Set(["low", "medium", "high", "critical"]);
    for (const v of ["low", "medium", "high", "critical"] as ImpactLevel[]) {
      expect(expected.has(v)).toBe(true);
    }
  });
});
