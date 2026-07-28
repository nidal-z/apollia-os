import { describe, it, expect } from "vitest";
import { humanize, permissionErrorHumanize } from "./humanize";

// Identity translator: returns the i18n key so tests can assert routing
// without loading svelte-i18n.
const id = (key: string): string => key;

describe("humanize", () => {
  it("always returns a HumanizedError, falling back to generic", () => {
    // GIVEN an unrecognized error
    const h = humanize("some totally unknown weirdo error", id);
    // THEN it never returns undefined and uses the generic category
    expect(h.category).toBe("generic");
    expect(h.code).toBeUndefined();
    expect(h.title).toBe("errors.generic.title");
    expect(h.suggested_action).toBe("errors.generic.suggested_action");
  });

  it("routes categories from the raw text", () => {
    expect(humanize("HTTP 429 Too Many Requests", id).category).toBe("rate-limit");
    expect(humanize("404 not found", id).category).toBe("not-found");
    expect(humanize("409 conflict: already exists", id).category).toBe("conflict");
    expect(humanize("401 unauthorized", id).category).toBe("auth");
    expect(humanize("400 invalid payload", id).category).toBe("validation");
    expect(humanize("internal server error", id).category).toBe("ipc");
    expect(humanize("EACCES: permission denied", id).category).toBe("permission");
  });

  it("resolves display strings from the category i18n stem", () => {
    // GIVEN a permission error
    const h = humanize("permission denied by policy", id);
    // THEN the strings resolve from errors.permission.*
    expect(h.title).toBe("errors.permission.title");
    expect(h.friendly_message).toBe("errors.permission.friendly_message");
    expect(h.code).toBe("POLICY_DENIED");
  });

  it("preserves the raw text as detail", () => {
    expect(humanize("boom", id).detail).toBe("boom");
    expect(humanize("", id).detail).toBeUndefined();
  });

  it("accepts Error instances and nullish input", () => {
    expect(humanize(new Error("EPERM: operation not permitted"), id).category).toBe(
      "permission",
    );
    expect(humanize(null, id).category).toBe("generic");
    expect(humanize(undefined, id).detail).toBeUndefined();
  });
});

describe("permissionErrorHumanize (backward-compat shim target)", () => {
  it("maps permission codes to legacy English payloads", () => {
    expect(permissionErrorHumanize("EACCES: permission denied")?.code).toBe("EACCES");
    expect(permissionErrorHumanize("permission denied by policy")?.code).toBe(
      "POLICY_DENIED",
    );
    expect(permissionErrorHumanize("StepBudget exceeded")?.code).toBe("BUDGET_EXCEEDED");
  });

  it("returns a complete legacy payload", () => {
    const h = permissionErrorHumanize("EACCES: permission denied");
    expect(h).toBeDefined();
    expect(h!.title).toMatch(/permission/i);
    expect(h!.friendly_message.length).toBeGreaterThan(0);
    expect(h!.suggested_action.length).toBeGreaterThan(0);
  });

  it("returns undefined for empty or unmatched strings", () => {
    expect(permissionErrorHumanize("")).toBeUndefined();
    expect(permissionErrorHumanize(null)).toBeUndefined();
    expect(permissionErrorHumanize(undefined)).toBeUndefined();
    expect(permissionErrorHumanize("some totally unknown weirdo error")).toBeUndefined();
  });
});
