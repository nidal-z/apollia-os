import { beforeEach, describe, expect, test, vi } from "vitest";
import type { Connection } from "$lib/types";

const getAgentDefinition = vi.fn();
const setAgentOverride = vi.fn();
const grantConnection = vi.fn();

vi.mock("$lib/ipc/agentSheet", () => ({ getAgentDefinition }));
vi.mock("$lib/ipc/agentAdjust", () => ({ setAgentOverride }));
vi.mock("$lib/ipc/connections", () => ({ listConnections: vi.fn(), grantConnection }));

const { accessWithTools, connectionTools, grantConnectionToApollia } = await import("./apolliaGrant");

function browser(tools: string[]): Connection {
  return {
    id: "mcp:navigateur",
    kind: "mcp",
    label: "navigateur",
    account: null,
    path: null,
    provenance: "custom",
    remote: false,
    official: false,
    state: "connected",
    tools: tools.map((id) => ({ id, family: "unclassified", prechecked: false })),
  } as unknown as Connection;
}

describe("granting a connection just added to the agent of the chat", () => {
  beforeEach(() => {
    getAgentDefinition.mockReset();
    setAgentOverride.mockReset();
    grantConnection.mockReset();
  });

  test("every tool of the connection is given, after what the agent held and once", () => {
    // GIVEN an agent that reads the web and already holds one tool of the browser
    const held = ["web_search", "mcp:navigateur/browser_navigate"];
    const tools = connectionTools(
      [browser(["mcp:navigateur/browser_navigate", "mcp:navigateur/browser_click"])],
      "mcp:navigateur",
    );

    // WHEN the browser is granted, twice
    const once = accessWithTools(held, tools);
    const twice = accessWithTools(once, tools);

    // THEN the missing tool is added at the end, whatever its family, and nothing is added again
    expect(once).toEqual([
      "web_search",
      "mcp:navigateur/browser_navigate",
      "mcp:navigateur/browser_click",
    ]);
    expect(twice).toEqual(once);
  });

  test("a server that has not indexed its tools yet is read again until it has", async () => {
    // GIVEN a server whose tools appear on the second read, and an agent holding one tool
    const list = vi
      .fn()
      .mockResolvedValueOnce([browser([])])
      .mockResolvedValueOnce([browser(["mcp:navigateur/browser_navigate"])]);
    getAgentDefinition.mockResolvedValue({ access: ["web_search"], overridden: ["access"] });

    // WHEN the connection is granted to Apollia
    const added = await grantConnectionToApollia("navigateur", { list, waitMs: 0 });

    // THEN its tool is granted to Apollia by the runtime
    expect(added).toBe(1);
    expect(list).toHaveBeenCalledTimes(2);
    expect(grantConnection).toHaveBeenCalledWith("mcp:navigateur", ["apollia:chat"], [
      "mcp:navigateur/browser_navigate",
    ]);
  });

  test("an Apollia on the default access is not given its default as a fixed list", async () => {
    // GIVEN an Apollia nobody chose an access for, whose card lists the reading family
    const list = vi.fn().mockResolvedValue([browser(["mcp:navigateur/browser_click"])]);
    getAgentDefinition.mockResolvedValue({ access: ["web_search", "file_read"], overridden: [] });

    // WHEN the browser is granted to it
    const added = await grantConnectionToApollia("navigateur", { list, waitMs: 0 });

    // THEN only the tool of the browser goes to the runtime, and no access list is written
    expect(added).toBe(1);
    expect(grantConnection).toHaveBeenCalledWith("mcp:navigateur", ["apollia:chat"], [
      "mcp:navigateur/browser_click",
    ]);
    expect(setAgentOverride).not.toHaveBeenCalled();
  });

  test("a connection that never offers a tool writes nothing", async () => {
    // GIVEN a server that never indexes a tool
    const list = vi.fn().mockResolvedValue([browser([])]);

    // WHEN the connection is granted to Apollia
    const added = await grantConnectionToApollia("navigateur", { list, attempts: 3, waitMs: 0 });

    // THEN the accesses of Apollia are neither read nor written
    expect(added).toBe(0);
    expect(list).toHaveBeenCalledTimes(3);
    expect(getAgentDefinition).not.toHaveBeenCalled();
    expect(grantConnection).not.toHaveBeenCalled();
  });
});
