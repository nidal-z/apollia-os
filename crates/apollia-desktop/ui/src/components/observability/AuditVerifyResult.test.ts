import { describe, test, expect } from "vitest";
import { verifyVerdict } from "./AuditVerifyResult.svelte";
import type { AuditVerifyResult } from "$lib/ipc/audit";

// DOM rendering is exercised by the Playwright E2E layer (vitest runs in `node`).
// These tests lock the verdict mapping that drives icon, color token and testid.

describe("AuditVerifyResult - verifyVerdict", () => {
  test("an intact chain yields the ok verdict", () => {
    // GIVEN a passing result
    const result: AuditVerifyResult = { ok: true, broken_at: null, message: "" };

    // WHEN mapping to a verdict
    // THEN it is "ok" (drives CheckCircle2 + text-success)
    expect(verifyVerdict(result)).toBe("ok");
  });

  test("a corrupted chain yields the fail verdict and exposes the broken link", () => {
    // GIVEN a failing result with a broken link
    const result: AuditVerifyResult = {
      ok: false,
      broken_at: "42",
      message: "hash mismatch",
    };

    // WHEN mapping to a verdict
    // THEN it is "fail" and broken_at is carried for the verify-broken-at line
    expect(verifyVerdict(result)).toBe("fail");
    expect(result.broken_at).toBe("42");
  });
});
