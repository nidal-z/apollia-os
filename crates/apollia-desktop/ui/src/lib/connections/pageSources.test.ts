import { describe, it, expect } from "vitest";
import { applyStateChange, loadPageSources, type PageData, type PageFetchers } from "./pageSources";
import type { Connection, McpServerStatusView } from "$lib/types";

function connection(id: string, over: Partial<Connection> = {}): Connection {
  return {
    id,
    kind: "mcp",
    label: id.slice(4),
    account: null,
    provenance: "selection",
    remote: false,
    official: false,
    state: "connected",
    status: "working",
    reason: null,
    checkedAt: null,
    package: null,
    tools: [],
    ...over,
  };
}

const SERVER = { name: "files" } as McpServerStatusView;

function fetchers(over: Partial<PageFetchers> = {}): PageFetchers {
  return {
    connections: async () => [connection("mcp:files")],
    servers: async () => [SERVER],
    enrichments: async () => [],
    agents: async () => [],
    ...over,
  };
}

describe("loadPageSources", () => {
  it("shows the connections when another source fails, with one banner for that source", async () => {
    // GIVEN the servers cannot be read, every other source answers
    const failing = fetchers({
      servers: async () => {
        throw new Error("runtime unreachable");
      },
    });

    // WHEN the page reads its sources
    const { data, failures } = await loadPageSources(failing);

    // THEN the connections are listed and only the servers carry a failure
    expect(data.connections.map((c) => c.id)).toEqual(["mcp:files"]);
    expect(failures).toEqual([{ source: "services", message: "runtime unreachable" }]);
  });

  it("keeps the list it showed when the connections themselves fail, and names each failing source", async () => {
    // GIVEN a page that already showed a connection and a server, then two sources failing
    const previous: PageData = {
      connections: [connection("mcp:notion")],
      servers: [SERVER],
      enrichments: new Map(),
      agents: [],
    };
    const failing = fetchers({
      connections: async () => {
        throw new Error("timeout");
      },
      agents: () => Promise.reject("agents down"),
    });

    // WHEN the page reads its sources again
    const { data, failures } = await loadPageSources(failing, previous);

    // THEN the list is not emptied, and there is one failure per source, in the order of the page
    expect(data.connections.map((c) => c.id)).toEqual(["mcp:notion"]);
    expect(data.servers).toEqual([SERVER]);
    expect(failures.map((f) => f.source)).toEqual(["connections", "agents"]);
    expect(failures[1].message).toBe("agents down");
  });

  it("reports nothing when every source answers", async () => {
    // GIVEN every source answering
    // WHEN the page reads them
    const { data, failures } = await loadPageSources(
      fetchers({ enrichments: async () => [{ package_identifier: "files", enrichment: {} as never }] }),
    );

    // THEN no banner, and the enrichments are keyed by package
    expect(failures).toEqual([]);
    expect([...data.enrichments.keys()]).toEqual(["files"]);
  });
});

describe("applyStateChange", () => {
  it("puts the connection named in its new state, and leaves the others", () => {
    // GIVEN two working connections
    const list = [connection("mcp:files"), connection("mcp:notion")];

    // WHEN the runtime finds the second needing its account to sign in again
    const next = applyStateChange(list, {
      id: "mcp:notion",
      state: "needs_repair",
      reason: { key: "sign_in_again", detail: "401" },
      checked_at: "2026-10-21T09:00:00Z",
    });

    // THEN only the second changes, with its reason and its date
    expect(next[0]).toEqual(list[0]);
    expect(next[1].status).toBe("needs_repair");
    expect(next[1].reason).toEqual({ key: "sign_in_again", detail: "401" });
    expect(next[1].checkedAt).toBe("2026-10-21T09:00:00Z");
  });
});
