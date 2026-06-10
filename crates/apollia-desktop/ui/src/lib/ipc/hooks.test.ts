import { vi, describe, test, expect, beforeEach } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

import { invoke } from "@tauri-apps/api/core";
import { getActiveHooks } from "./hooks";

const mockedInvoke = vi.mocked(invoke);

beforeEach(() => {
  vi.clearAllMocks();
});

describe("hooks IPC - getActiveHooks", () => {
  test("invokes get_active_hooks and returns the handlers", async () => {
    // GIVEN the runtime reports two handlers
    mockedInvoke.mockResolvedValue([
      { id: 0, type: "command", events: ["pre_tool_use"], timeout_ms: 5000, target: "guard.sh" },
      { id: 1, type: "http", events: ["post_tool_use"], timeout_ms: 3000, target: "https://x" },
    ]);

    // WHEN listing active hooks
    const hooks = await getActiveHooks();

    // THEN the typed command is used (no args) and the handlers flow through
    expect(mockedInvoke).toHaveBeenCalledWith("get_active_hooks");
    expect(hooks).toHaveLength(2);
    expect(hooks[0].events).toContain("pre_tool_use");
  });

  test("an empty configuration returns an empty list (valid state)", async () => {
    // GIVEN no hooks configured
    mockedInvoke.mockResolvedValue([]);

    // WHEN listing active hooks
    const hooks = await getActiveHooks();

    // THEN the empty list is returned without error
    expect(hooks).toEqual([]);
  });
});
