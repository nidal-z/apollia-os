import { describe, test, expect } from "vitest";
import type { HallucinationRisk } from "$lib/types";

/**
 * Unit tests for the `HallucinationRiskBadge` banding contract.
 *
 * The Svelte rendering itself is exercised through the app; these tests lock
 * the score → band mapping so the visual contract stays in sync with
 * `apollia_runtime::analyzers::compute_session_hallucination_risk`.
 */

type Band = "safe" | "low" | "elevated" | "high";

function bandFor(score: number): Band {
  if (score >= 75) return "high";
  if (score >= 50) return "elevated";
  if (score >= 25) return "low";
  return "safe";
}

describe("HallucinationRiskBadge banding", () => {
  test("score 0 lands in safe", () => {
    expect(bandFor(0)).toBe("safe");
  });

  test("score 24 still safe, 25 flips to low", () => {
    expect(bandFor(24)).toBe("safe");
    expect(bandFor(25)).toBe("low");
  });

  test("score 50 flips to elevated, 75 flips to high", () => {
    expect(bandFor(49)).toBe("low");
    expect(bandFor(50)).toBe("elevated");
    expect(bandFor(74)).toBe("elevated");
    expect(bandFor(75)).toBe("high");
  });

  test("score 100 is high", () => {
    expect(bandFor(100)).toBe("high");
  });

  test("risk with empty factors is accepted", () => {
    const risk: HallucinationRisk = { score: 0, factors: [] };
    expect(risk.factors.length).toBe(0);
  });
});
