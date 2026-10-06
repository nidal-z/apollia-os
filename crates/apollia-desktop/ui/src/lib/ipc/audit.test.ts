import { vi, describe, test, expect, beforeEach } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

import { invoke } from "@tauri-apps/api/core";
import { getToolAuditTrail } from "./audit";

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
