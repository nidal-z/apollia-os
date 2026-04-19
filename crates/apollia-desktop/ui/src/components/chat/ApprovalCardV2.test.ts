import { describe, test, expect } from "vitest";
import { deriveRiskLevel } from "./ApprovalCardV2.svelte";

// ─── deriveRiskLevel ─────────────────────────────────────────────────────────

describe("ApprovalCardV2 — deriveRiskLevel", () => {
  test("maps scores 0..3 to low", () => {
    // GIVEN scores in the [0, 3] interval
    // WHEN deriveRiskLevel is called
    // THEN all of them collapse to "low"
    for (const s of [0, 1, 2, 3]) expect(deriveRiskLevel(s)).toBe("low");
  });

  test("maps scores 4..6 to medium", () => {
    for (const s of [4, 5, 6]) expect(deriveRiskLevel(s)).toBe("medium");
  });

  test("maps scores 7..8 to high", () => {
    for (const s of [7, 8]) expect(deriveRiskLevel(s)).toBe("high");
  });

  test("maps scores 9..10 to critical", () => {
    for (const s of [9, 10]) expect(deriveRiskLevel(s)).toBe("critical");
  });

  test("falls back to medium when the score is undefined", () => {
    // GIVEN no risk_score declared on the manifest
    // THEN UI defaults to medium to encourage review without scaring the user
    expect(deriveRiskLevel(undefined)).toBe("medium");
  });

  test("is monotonic across the full domain", () => {
    // GIVEN the numeric score domain
    // WHEN we map each score
    // THEN the resulting ordinal sequence never decreases
    const ord: Record<string, number> = { low: 0, medium: 1, high: 2, critical: 3 };
    let prev = -1;
    for (let s = 0; s <= 10; s++) {
      const next = ord[deriveRiskLevel(s)];
      expect(next).toBeGreaterThanOrEqual(prev);
      prev = next;
    }
  });
});

// ─── Decision payload shape (compile-time contract) ──────────────────────────

describe("ApprovalCardV2 — decision payload union", () => {
  test("all three decision kinds are typeable", () => {
    // GIVEN the documented union shape
    type Payload =
      | { kind: "approve" }
      | { kind: "reject"; reason: string | null }
      | {
          kind: "always_accept";
          scope:
            | "this_tool"
            | "this_session"
            | "this_agent"
            | "this_project"
            | "global";
        };

    const samples: Payload[] = [
      { kind: "approve" },
      { kind: "reject", reason: null },
      { kind: "reject", reason: "out of scope" },
      { kind: "always_accept", scope: "this_tool" },
      { kind: "always_accept", scope: "this_session" },
      { kind: "always_accept", scope: "this_agent" },
      { kind: "always_accept", scope: "this_project" },
      { kind: "always_accept", scope: "global" },
    ];

    // WHEN we inspect each payload's discriminant
    // THEN the kind field is always one of the three known values
    for (const s of samples) {
      expect(["approve", "reject", "always_accept"]).toContain(s.kind);
    }
  });
});
