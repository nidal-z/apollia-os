import { vi, describe, test, expect, beforeEach } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue(null),
}));

import { invoke } from "@tauri-apps/api/core";
import { resolveSovereignty } from "$lib/ipc/connections";

const mockedInvoke = vi.mocked(invoke);

function profileWith(sovereignty: string | null) {
  const entries = sovereignty
    ? [{ key: "constraints.sovereignty", value: sovereignty }]
    : [];
  return { schema_entries: entries, extras: [], entries, last_updated_at: null };
}

beforeEach(() => {
  vi.clearAllMocks();
});

describe("resolveSovereignty", () => {
  test("the strictly-local profile maps to the value the command refuses on", async () => {
    // GIVEN a profile whose sovereignty is set to strictly local
    mockedInvoke.mockResolvedValue(profileWith("local-only"));

    // WHEN the sovereignty is resolved for an OAuth flow
    const resolved = await resolveSovereignty();

    // THEN it is the value `ensure_cloud_allowed` rejects
    expect(resolved).toBe("local_only");
  });

  test("the local-preferred profile allows the flow, cloud being its last resort", async () => {
    // GIVEN a profile that prefers local but permits cloud after approval
    mockedInvoke.mockResolvedValue(profileWith("local-preferred"));

    // WHEN the sovereignty is resolved
    const resolved = await resolveSovereignty();

    // THEN the connector flow is allowed to proceed
    expect(resolved).toBe("cloud_allowed");
  });

  test("an unreadable profile fails closed rather than opening a cloud flow", async () => {
    // GIVEN a profile store that cannot be reached
    mockedInvoke.mockRejectedValue(new Error("profile store unavailable"));

    // WHEN the sovereignty is resolved
    const resolved = await resolveSovereignty();

    // THEN the strict value is returned, so a lost store cannot open a flow
    expect(resolved).toBe("local_only");
  });

  test("a profile that never set the field fails closed too", async () => {
    // GIVEN a profile carrying no sovereignty entry at all
    mockedInvoke.mockResolvedValue(profileWith(null));

    // WHEN the sovereignty is resolved
    const resolved = await resolveSovereignty();

    // THEN the absent setting does not read as permission
    expect(resolved).toBe("local_only");
  });
});
