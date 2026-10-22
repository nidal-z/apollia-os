import { describe, test, expect } from "vitest";
import { connectionOffers, proposeConnection, withProposedTools } from "$lib/chat/connectionOffer";
import type { Connection } from "$lib/types";

function result(matches: unknown[]): string {
  return JSON.stringify({ matches, total: matches.length });
}

describe("connectionOffers", () => {
  test("each entry is offered by where it stands, an entry already granted offering nothing", () => {
    // GIVEN a lookup that found one entry of each status
    const output = result([
      { entry: "@notionhq/notion-mcp-server", label: "Notion", status: "not_connected" },
      { entry: "com.linear/mcp", label: "Linear", status: "connected_not_granted", connection: "mcp:linear" },
      { entry: "com.github/mcp", label: "GitHub", status: "granted", connection: "mcp:github" },
    ]);

    // WHEN its offers are read
    const offers = connectionOffers(output);

    // THEN the first is to add, the second to grant, the third offers nothing
    expect(offers).toEqual([
      { kind: "add", entry: "@notionhq/notion-mcp-server", label: "Notion" },
      { kind: "grant", entry: "com.linear/mcp", label: "Linear", connection: "mcp:linear" },
    ]);
  });

  test("an output that is not the tool's result offers nothing", () => {
    // GIVEN outputs that are no lookup result
    const outputs = [null, "not json", "[]", JSON.stringify({ matches: "none" })];

    // WHEN their offers are read
    const offers = outputs.map(connectionOffers);

    // THEN none offers anything
    expect(offers).toEqual([[], [], [], []]);
  });
});

describe("withProposedTools", () => {
  test("the tools are added to what the conversation proposes, and switched back on", () => {
    // GIVEN settings proposing one access and switching off a tool of the connection
    const settings = {
      temperature: 0.4,
      proposed_accesses: ["file_write"],
      disabled_accesses: ["mcp:notion/search"],
    };

    // WHEN the tools of the connection are proposed
    const next = withProposedTools(settings, ["mcp:notion/search", "file_write"]);

    // THEN each is proposed once, none is switched off, and the rest is kept
    expect(next).toEqual({ temperature: 0.4, proposed_accesses: ["file_write", "mcp:notion/search"] });
  });
});

describe("proposeConnection", () => {
  test("a connection with no tool yet is read again, and nothing is written while it has none", async () => {
    // GIVEN a connection that never lists a tool
    let reads = 0;
    const written: unknown[] = [];
    const deps = {
      list: async (): Promise<Connection[]> => {
        reads += 1;
        return [{ id: "mcp:notion", tools: [] } as unknown as Connection];
      },
      read: async () => ({}),
      write: async (_id: string, settings: object) => {
        written.push(settings);
        return settings;
      },
      attempts: 3,
      waitMs: 0,
    };

    // WHEN it is proposed to a conversation
    const proposed = await proposeConnection("session-1", "mcp:notion", deps);

    // THEN it was read three times, and the settings were left alone
    expect(proposed).toBe(0);
    expect(reads).toBe(3);
    expect(written).toEqual([]);
  });
});
