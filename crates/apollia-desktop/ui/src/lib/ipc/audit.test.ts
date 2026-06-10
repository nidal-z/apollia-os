import { vi, describe, test, expect, beforeEach } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

import { invoke } from "@tauri-apps/api/core";
import { getToolAuditTrail, verifyAuditRun } from "./audit";

const mockedInvoke = vi.mocked(invoke);

beforeEach(() => {
  vi.clearAllMocks();
});

describe("audit IPC - getToolAuditTrail", () => {
  test("invokes get_tool_audit_trail with the requested limit", async () => {
    // GIVEN the backend returns two entries
    mockedInvoke.mockResolvedValue([{ id: "a" }, { id: "b" }]);

    // WHEN reading the trail with a limit
    const entries = await getToolAuditTrail(25);

    // THEN the typed command and args are used and the value flows through
    expect(mockedInvoke).toHaveBeenCalledWith("get_tool_audit_trail", { limit: 25 });
    expect(entries).toHaveLength(2);
  });
});

describe("audit IPC - verifyAuditRun", () => {
  test("invokes verify_audit_run with the run id and returns the verdict", async () => {
    // GIVEN the backend reports an intact chain
    mockedInvoke.mockResolvedValue({ ok: true, broken_at: null, message: "" });

    // WHEN verifying a run
    const result = await verifyAuditRun("run-abc");

    // THEN the command receives the camelCase runId arg and the verdict returns
    expect(mockedInvoke).toHaveBeenCalledWith("verify_audit_run", { runId: "run-abc" });
    expect(result.ok).toBe(true);
  });

  test("propagates a rejection for an unknown run", async () => {
    // GIVEN the backend rejects (404 mapped to an error string)
    mockedInvoke.mockRejectedValue("API error (404): not_found");

    // WHEN verifying an unknown run
    // THEN the rejection surfaces to the caller (no silent ok verdict)
    await expect(verifyAuditRun("run-unknown")).rejects.toBe(
      "API error (404): not_found",
    );
  });
});
