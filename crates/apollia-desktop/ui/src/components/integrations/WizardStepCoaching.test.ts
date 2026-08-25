import { vi, describe, test, expect, beforeEach } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue([]),
}));

import { invoke } from "@tauri-apps/api/core";
import { metaGenerateCapabilitiesCoaching } from "$lib/ipc/connections";

const mockedInvoke = vi.mocked(invoke);

beforeEach(() => {
  vi.clearAllMocks();
  mockedInvoke.mockResolvedValue([]);
});

// ── meta_generate_capabilities_coaching, argument shape ───────────────────────

describe("metaGenerateCapabilitiesCoaching - the shape Tauri reads", () => {
  test("the argument object nests the payload under the key Tauri reads", async () => {
    // GIVEN a server whose title differs from its name
    const serverName = "filesystem";
    const serverTitle = "Local files";

    // WHEN the coaching step asks for its usage examples
    await metaGenerateCapabilitiesCoaching(serverName, serverTitle);

    // THEN the single argument is `request`, because the Rust signature takes
    // one structured argument of that name; a flat object omits it and Tauri
    // rejects the call outright.
    const [command, args] = mockedInvoke.mock.calls[0] as [
      string,
      Record<string, unknown>,
    ];
    expect(command).toBe("meta_generate_capabilities_coaching");
    expect(Object.keys(args)).toEqual(["request"]);
    expect(args.request).toEqual({
      serverName: "filesystem",
      serverTitle: "Local files",
    });
  });

  test("the nested fields are camelCase, as CoachingRequest renames them", async () => {
    // GIVEN a server with no distinct title
    // WHEN its examples are requested
    await metaGenerateCapabilitiesCoaching("notion", null);

    // THEN the nested keys match `#[serde(rename_all = "camelCase")]` on
    // `CoachingRequest`, and the missing title falls back to the name.
    const args = mockedInvoke.mock.calls[0]?.[1] as Record<string, unknown>;
    const request = args.request as Record<string, unknown>;
    expect(Object.keys(request).sort()).toEqual(["serverName", "serverTitle"]);
    expect(request.serverTitle).toBe("notion");
  });

  test("the runtime answer is handed back untouched", async () => {
    // GIVEN the runtime returns one example card
    const card = {
      title: "Explore a folder",
      description: "List the files in an allowed folder.",
      prompt: "List the files in my allowed folder.",
    };
    mockedInvoke.mockResolvedValue([card]);

    // WHEN the coaching step asks for its usage examples
    const examples = await metaGenerateCapabilitiesCoaching("filesystem", null);

    // THEN it receives the cards rather than an error message: this is the
    // surface the wizard renders in place of its raw error line.
    expect(examples).toEqual([card]);
  });
});
