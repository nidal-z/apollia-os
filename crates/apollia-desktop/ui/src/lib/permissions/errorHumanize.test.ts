import { describe, it, expect } from "vitest";
import { permissionErrorHumanize } from "./errorHumanize";

describe("permissionErrorHumanize", () => {
  it("maps EACCES variants", () => {
    expect(permissionErrorHumanize("EACCES: permission denied")?.code).toBe("EACCES");
    expect(permissionErrorHumanize("permission denied")?.code).toBe("EACCES");
  });

  it("maps EPERM", () => {
    expect(permissionErrorHumanize("EPERM: operation not permitted")?.code).toBe("EPERM");
  });

  it("maps policy-denied", () => {
    expect(permissionErrorHumanize("permission denied by policy")?.code).toBe("POLICY_DENIED");
    expect(permissionErrorHumanize("policy violation on tool X")?.code).toBe("POLICY_DENIED");
  });

  it("maps ENOENT", () => {
    expect(permissionErrorHumanize("ENOENT: no such file or directory")?.code).toBe("ENOENT");
  });

  it("maps EISDIR", () => {
    expect(permissionErrorHumanize("EISDIR: is a directory")?.code).toBe("EISDIR");
  });

  it("maps ENOSPC", () => {
    expect(permissionErrorHumanize("ENOSPC: no space left on device")?.code).toBe("ENOSPC");
  });

  it("maps timeouts", () => {
    expect(permissionErrorHumanize("Operation timed out")?.code).toBe("TIMEOUT");
    expect(permissionErrorHumanize("ETIMEDOUT")?.code).toBe("TIMEOUT");
  });

  it("maps network errors", () => {
    expect(permissionErrorHumanize("ECONNREFUSED")?.code).toBe("NETWORK");
    expect(permissionErrorHumanize("DNS lookup failed")?.code).toBe("NETWORK");
  });

  it("maps budget exceeded", () => {
    expect(permissionErrorHumanize("StepBudget exceeded")?.code).toBe("BUDGET_EXCEEDED");
  });

  it("maps sandbox violation", () => {
    expect(permissionErrorHumanize("sandbox violation")?.code).toBe("SANDBOX_VIOLATION");
  });

  it("returns a complete payload (title, message, suggestion)", () => {
    const h = permissionErrorHumanize("EACCES: permission denied");
    expect(h).toBeDefined();
    expect(h!.title).toMatch(/permission/i);
    expect(h!.friendly_message.length).toBeGreaterThan(0);
    expect(h!.suggested_action.length).toBeGreaterThan(0);
  });

  it("returns undefined for empty or unmatched strings (LLM fallback)", () => {
    expect(permissionErrorHumanize("")).toBeUndefined();
    expect(permissionErrorHumanize(null)).toBeUndefined();
    expect(permissionErrorHumanize(undefined)).toBeUndefined();
    expect(permissionErrorHumanize("some totally unknown weirdo error")).toBeUndefined();
  });
});
