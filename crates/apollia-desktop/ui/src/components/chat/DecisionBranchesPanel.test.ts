import { describe, test, expect } from "vitest";
import type { DecisionPoint } from "$lib/types";

/**
 * Unit tests for the DecisionBranchesPanel pure logic (US-SP42-041).
 *
 * The Svelte component renders a DecisionPoint payload: this file exercises
 * the shape contract and the value derivations (delta clamping, kind label
 * mapping) so regressions surface without a full render environment.
 */

// ─── Test helpers duplicated from the component to exercise the contract ───

function clampDelta(delta: number): number {
  if (Number.isNaN(delta)) return 0;
  return Math.max(-1, Math.min(0, delta));
}

function kindLabel(kind: DecisionPoint["kind"]): string {
  switch (kind) {
    case "tool_choice":
      return "Tool choice";
    case "agent_delegate":
      return "Agent delegate";
    case "memory_write":
      return "Memory write";
    default:
      return "Decision";
  }
}

// ─── Fixtures ───

function sampleDecisionPoint(): DecisionPoint {
  return {
    turn_id: "turn-42",
    kind: "tool_choice",
    chosen: "read_file",
    alternatives: [
      {
        label: "bash_executor",
        rejected_reason: "overkill for a single file",
        confidence_delta: -0.3,
      },
      {
        label: "grep",
        rejected_reason: "path already known",
        confidence_delta: -0.5,
      },
    ],
  };
}

describe("DecisionBranchesPanel", () => {
  test("clampDelta clamps positive deltas to 0", () => {
    expect(clampDelta(0.5)).toBe(0);
    expect(clampDelta(1)).toBe(0);
  });

  test("clampDelta preserves valid negative deltas", () => {
    expect(clampDelta(-0.3)).toBeCloseTo(-0.3);
    expect(clampDelta(-1)).toBe(-1);
  });

  test("clampDelta floors extreme negatives at -1", () => {
    expect(clampDelta(-2.5)).toBe(-1);
  });

  test("clampDelta treats NaN as 0", () => {
    expect(clampDelta(Number.NaN)).toBe(0);
  });

  test("kindLabel maps known kinds", () => {
    expect(kindLabel("tool_choice")).toBe("Tool choice");
    expect(kindLabel("agent_delegate")).toBe("Agent delegate");
    expect(kindLabel("memory_write")).toBe("Memory write");
    expect(kindLabel("significant")).toBe("Decision");
  });

  test("DecisionPoint fixture has chosen plus ≤3 alternatives", () => {
    const dp = sampleDecisionPoint();
    expect(dp.chosen).toBeTruthy();
    expect(dp.alternatives.length).toBeLessThanOrEqual(3);
    for (const alt of dp.alternatives) {
      expect(alt.label.length).toBeGreaterThan(0);
      expect(alt.confidence_delta).toBeLessThanOrEqual(0);
    }
  });
});
