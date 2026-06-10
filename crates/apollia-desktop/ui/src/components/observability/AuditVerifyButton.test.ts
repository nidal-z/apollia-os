import { describe, test, expect, vi } from "vitest";
import {
  performVerify,
  verifyErrorMessage,
  type VerifyState,
} from "./AuditVerifyButton.svelte";
import type { AuditVerifyResult } from "$lib/ipc/audit";

// DOM rendering is exercised by the Playwright E2E layer (vitest runs in `node`).
// These tests lock the verify orchestration: verdict handling, error mapping,
// and single-flight (double-click) protection.

function freshState(): VerifyState {
  return { loading: false, result: null, error: null };
}

const okResult: AuditVerifyResult = { ok: true, broken_at: null, message: "" };
const failResult: AuditVerifyResult = {
  ok: false,
  broken_at: "7",
  message: "hash mismatch",
};

describe("AuditVerifyButton - performVerify", () => {
  test("stores the ok verdict and clears loading", async () => {
    // GIVEN a fresh state and a verify that resolves intact
    const state = freshState();

    // WHEN running the verification
    await performVerify(state, () => Promise.resolve(okResult));

    // THEN the verdict is stored, no error, not loading
    expect(state.result).toEqual(okResult);
    expect(state.error).toBeNull();
    expect(state.loading).toBe(false);
  });

  test("stores the corrupted verdict with its broken link", async () => {
    // GIVEN a verify that resolves corrupted
    const state = freshState();

    // WHEN running the verification
    await performVerify(state, () => Promise.resolve(failResult));

    // THEN the failing verdict and broken link are stored
    expect(state.result?.ok).toBe(false);
    expect(state.result?.broken_at).toBe("7");
  });

  test("maps a rejection to an error message without a verdict", async () => {
    // GIVEN a verify that rejects (unknown run)
    const state = freshState();

    // WHEN running the verification
    await performVerify(state, () => Promise.reject(new Error("run not found")));

    // THEN the error is surfaced and no verdict is set
    expect(state.error).toBe("run not found");
    expect(state.result).toBeNull();
    expect(state.loading).toBe(false);
  });

  test("ignores a second call while a verification is in flight", async () => {
    // GIVEN a verify in progress (loading already true)
    const state: VerifyState = { loading: true, result: null, error: null };
    const verify = vi.fn(() => Promise.resolve(okResult));

    // WHEN a second click fires during loading
    await performVerify(state, verify);

    // THEN it is a no-op: the verify function is never invoked
    expect(verify).not.toHaveBeenCalled();
  });
});

describe("AuditVerifyButton - verifyErrorMessage", () => {
  test("unwraps an Error to its message", () => {
    expect(verifyErrorMessage(new Error("boom"))).toBe("boom");
  });

  test("stringifies a non-Error rejection", () => {
    expect(verifyErrorMessage("API error (404): not_found")).toBe(
      "API error (404): not_found",
    );
  });
});
