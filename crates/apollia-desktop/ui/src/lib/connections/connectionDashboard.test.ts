import { describe, expect, it } from "vitest";
import {
  bringsKeys,
  dashboardSummary,
  emptySuggestions,
  loadTileFacts,
  orderedAgents,
  type TileFacts,
  type TileReaders,
} from "./connectionDashboard";
import type { ConnectionActivity, ConnectionAgents } from "$lib/ipc/connections";
import type { ConnectorEnrichmentView } from "$lib/types";

function agentsOf(connection: string, ids: string[]): ConnectionAgents {
  return {
    connection,
    agents: ids.map((id) => ({ id, name: id, enabled: true, byDefault: false, tools: [] })),
    sharedWith: [],
  };
}

function activityOf(prefix: string, lastUsedAt: string | null): ConnectionActivity {
  return { toolPrefix: prefix, since: "", until: null, actions: lastUsedAt ? 1 : 0, lastUsedAt, agents: [] };
}

function enrichment(label: string, icon: string, trust: ConnectorEnrichmentView["trust_level"]): ConnectorEnrichmentView {
  return {
    operator_label: { en: label },
    category: "productivity",
    icon_name: icon,
    trust_level: trust,
    auth_help_url: null,
    auth_help_text: null,
    auth_help_i18n_key: null,
    oauth_pre_registered_client_id_env: null,
  };
}

describe("the summary over the grid", () => {
  it("counts the connections, those that work, those that ask for an action, and the distinct agents", () => {
    // GIVEN five connections, one working, and the agents read for three of them
    const rows = [
      { key: "native:microsoft:ada@example.com", status: "not_checked" as const },
      { key: "mcp:notion", status: "working" as const },
      { key: "folder:demo", status: "needs_repair" as const },
      { key: "imap:ada@example.com", status: "paused" as const },
      { key: "mcp:linear", status: "blocked" as const },
    ];
    const facts = new Map<string, TileFacts>([
      ["mcp:notion", { agents: ["apollia:chat", "writer", "planner"], lastUsedAt: null }],
      ["folder:demo", { agents: ["writer"], lastUsedAt: null }],
      ["mcp:linear", { agents: [], lastUsedAt: null }],
    ]);

    // WHEN the summary is drawn
    const summary = dashboardSummary(rows, facts);

    // THEN five are connected, one works, four ask for an action, and three
    // agents use them, each counted once
    expect(summary).toEqual({ total: 5, working: 1, action: 4, agents: 3 });
  });
});

describe("the facts of the tiles", () => {
  it("reads the agents and the last use of each connection, the activity only where its tools have a prefix", async () => {
    // GIVEN a service, an account and a mailbox, and readers that record what they are asked
    const asked: string[] = [];
    const readers: TileReaders = {
      agents: async (id) => {
        asked.push(`agents ${id}`);
        return agentsOf(id, id === "mcp:notion" ? ["writer"] : []);
      },
      activity: async (prefix, since) => {
        asked.push(`activity ${prefix} ${since}`);
        return activityOf(prefix, prefix === "mcp:notion/" ? "2026-10-28T09:00:00Z" : null);
      },
    };
    const now = Date.parse("2026-10-29T12:00:00Z");

    // WHEN the facts are loaded
    const facts = await loadTileFacts(
      [
        { id: "mcp:notion", kind: "mcp" },
        { id: "native:google:ada@example.com", kind: "native" },
        { id: "imap:ada@example.com", kind: "imap" },
      ],
      readers,
      now,
    );

    // THEN each has its agents and last use, over the seven days before now,
    // and the mailbox, whose tools carry no prefix, had no activity asked
    expect(facts.get("mcp:notion")).toEqual({ agents: ["writer"], lastUsedAt: "2026-10-28T09:00:00Z" });
    expect(facts.get("native:google:ada@example.com")).toEqual({ agents: [], lastUsedAt: null });
    expect(facts.get("imap:ada@example.com")).toEqual({ agents: [], lastUsedAt: null });
    expect(asked).toContain("activity mcp:notion/ 2026-10-22T12:00:00.000Z");
    expect(asked).toContain("activity conn:google:ada@example.com/ 2026-10-22T12:00:00.000Z");
    expect(asked.filter((a) => a.startsWith("activity"))).toHaveLength(2);
  });

  it("reads no more than the bound, and no more than the concurrency at once", async () => {
    // GIVEN thirty services and readers that count the calls in flight
    const connections = Array.from({ length: 30 }, (_, i) => ({ id: `mcp:s${i}`, kind: "mcp" as const }));
    let inFlight = 0;
    let peak = 0;
    let calls = 0;
    const pause = async <T>(value: T): Promise<T> => {
      calls += 1;
      inFlight += 1;
      peak = Math.max(peak, inFlight);
      await new Promise((resolve) => setTimeout(resolve, 1));
      inFlight -= 1;
      return value;
    };
    const readers: TileReaders = {
      agents: (id) => pause(agentsOf(id, [])),
      activity: (prefix) => pause(activityOf(prefix, null)),
    };

    // WHEN the facts are loaded with a bound of ten, three at a time
    const facts = await loadTileFacts(connections, readers, Date.now(), 10, 3);

    // THEN only the first ten are read, two commands each, never more than
    // three connections (six commands) at once
    expect([...facts.keys()]).toHaveLength(10);
    expect(facts.has("mcp:s9")).toBe(true);
    expect(facts.has("mcp:s10")).toBe(false);
    expect(calls).toBe(20);
    expect(peak).toBeLessThanOrEqual(6);
  });

  it("keeps a tile whose reads fail, with nothing said of its agents", async () => {
    // GIVEN readers that fail
    const readers: TileReaders = {
      agents: () => Promise.reject(new Error("runtime unreachable")),
      activity: () => Promise.reject(new Error("journal not configured")),
    };

    // WHEN the facts are loaded
    const facts = await loadTileFacts([{ id: "mcp:notion", kind: "mcp" }], readers, Date.now());

    // THEN the connection has empty facts rather than none
    expect(facts.get("mcp:notion")).toEqual({ agents: [], lastUsedAt: null });
  });
});

describe("what a tile says", () => {
  it("says in words what an account, a mailbox and a folder bring, and leaves a service to its tools", () => {
    // GIVEN one connection of each kind
    // WHEN what each brings is asked
    // THEN an account names its services, a mailbox its mail, a folder its files
    expect(bringsKeys({ kind: "native", key: "native:microsoft:ada@example.com" })).toEqual([
      "connections.tile.brings.mail",
      "connections.tile.brings.calendar",
      "connections.tile.brings.onedrive",
    ]);
    expect(bringsKeys({ kind: "native", key: "native:google:ada@example.com" })).toContain(
      "connections.tile.brings.drive",
    );
    expect(bringsKeys({ kind: "imap", key: "imap:ada@example.com" })).toEqual(["connections.tile.brings.mail"]);
    expect(bringsKeys({ kind: "folder", key: "folder:demo" })).toEqual(["connections.tile.brings.files"]);
    expect(bringsKeys({ kind: "mcp", key: "mcp:notion" })).toEqual([]);
  });

  it("names Apollia first, then the agents by name", () => {
    // GIVEN agents in any order
    // WHEN the tile orders them
    // THEN Apollia leads
    expect(orderedAgents(["writer", "apollia:chat", "planner"])).toEqual(["apollia:chat", "planner", "writer"]);
  });
});

describe("the suggestions of an empty page", () => {
  it("offers the first three official services of the selection that carry their logo", () => {
    // GIVEN the selection in its order, with a community entry and a brand with no logo among them
    const selection = new Map<string, ConnectorEnrichmentView>([
      ["@notionhq/notion-mcp-server", { ...enrichment("Notion", "notion", "verified_official"), registry_names: ["com.notion/mcp"] }],
      ["@modelcontextprotocol/server-brave-search", enrichment("Brave", "brave", "community_verified")],
      ["com.slack/mcp", enrichment("Slack", "slack", "verified_official")],
      ["io.github.github/github-mcp-server", enrichment("GitHub", "github", "verified_official")],
      ["app.linear/mcp", enrichment("Linear", "linear", "verified_official")],
      ["com.stripe/mcp", enrichment("Stripe", "stripe", "verified_official")],
    ]);

    // WHEN the suggestions are chosen
    const offered = emptySuggestions(selection);

    // THEN Notion (by its registry name), GitHub and Linear are offered
    expect(offered.map((s) => s.entryId)).toEqual([
      "com.notion/mcp",
      "io.github.github/github-mcp-server",
      "app.linear/mcp",
    ]);
  });
});
