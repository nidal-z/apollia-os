import { describe, test, expect, vi, beforeEach, afterEach } from "vitest";
import { flushSync, mount, unmount } from "svelte";
import { locale, waitLocale } from "svelte-i18n";

const invoke = vi.hoisted(() => vi.fn(async (..._args: unknown[]): Promise<unknown> => null));
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(async () => null) }));

import "$lib/i18n";
import { developerMode } from "$lib/stores/developerMode";
import { connectionsView } from "$lib/stores/connectionsView";
import { brandLogoPath } from "$lib/connections/brandLogo";
import type { Connection, ConnectionTool } from "$lib/types";
import Connections from "../../../routes/Connections.svelte";

// The page is mounted whole, its commands answered here: the summary, the
// order of the tiles, one tile of each kind, the filter on a state and the
// empty page are held on what the user sees.

const HOUR = 3_600_000;

function connection(over: Partial<Connection>): Connection {
  return {
    id: "mcp:notion",
    kind: "mcp",
    label: "Notion",
    account: null,
    provenance: "selection",
    remote: true,
    official: true,
    state: "connected",
    status: "working",
    reason: null,
    checkedAt: new Date(Date.now() - 2 * HOUR).toISOString(),
    package: null,
    tools: [],
    ...over,
  } as Connection;
}

const tool = (id: string): ConnectionTool => ({ id, family: "read", familySource: "selection", prechecked: true });

const FIVE: Connection[] = [
  connection({ tools: [tool("mcp:notion/search"), tool("mcp:notion/create_page")] }),
  connection({
    id: "native:google:ada@example.com",
    kind: "native",
    label: "Google",
    account: "ada@example.com",
    status: "needs_repair",
    reason: { key: "sign_in_again" },
  }),
  connection({
    id: "native:microsoft:ada@example.com",
    kind: "native",
    label: "Microsoft",
    account: "ada@example.com",
    status: "not_checked",
    reason: { key: "not_checked" },
    checkedAt: null,
  }),
  connection({
    id: "folder:demo",
    kind: "folder",
    label: "Demo",
    account: null,
    path: "/Users/ada/Demo",
    provenance: "custom",
    remote: false,
    official: false,
    status: "working",
  }),
  connection({
    id: "imap:ada@example.com",
    kind: "imap",
    label: "Fastmail",
    account: "ada@example.com",
    provenance: "custom",
    status: "paused",
  }),
];

let listed: Connection[] = [];
let enrichments: unknown[] = [];
let target: HTMLElement;
let page: ReturnType<typeof mount> | null = null;

async function settle(rounds = 30): Promise<void> {
  for (let i = 0; i < rounds; i++) {
    await new Promise((resolve) => setTimeout(resolve, 0));
    flushSync();
  }
}

function byTestid(id: string): HTMLElement | null {
  return target.querySelector(`[data-testid="${CSS.escape(id)}"]`);
}

function tileIds(): string[] {
  return [...target.querySelectorAll('[data-testid^="sidebar-mcp-"], [data-testid^="sidebar-connection-"]')].map(
    (el) => el.getAttribute("data-testid") ?? "",
  );
}

beforeEach(async () => {
  locale.set("en");
  await waitLocale("en");
  developerMode.set(false);
  connectionsView.reset();
  invoke.mockReset();
  enrichments = [];
  invoke.mockImplementation(async (command: unknown, args: unknown) => {
    const name = String(command);
    const a = (args ?? {}) as Record<string, unknown>;
    if (name === "list_connections") return listed;
    if (name === "list_mcp_enrichments") return enrichments;
    if (name === "get_connection_agents") {
      const holders: Record<string, string[]> = {
        "mcp:notion": ["apollia:chat", "writer", "planner"],
        "folder:demo": ["writer"],
      };
      const ids = holders[String(a.connectionId)] ?? [];
      return {
        connection: a.connectionId,
        agents: ids.map((id) => ({ id, name: id, enabled: true, byDefault: false, tools: [] })),
        sharedWith: [],
      };
    }
    if (name === "get_connection_activity") {
      const used = a.toolPrefix === "mcp:notion/" ? new Date(Date.now() - 3 * HOUR).toISOString() : null;
      return { toolPrefix: a.toolPrefix, since: a.since, until: null, actions: used ? 4 : 0, lastUsedAt: used, agents: [] };
    }
    if (/(^|_)list(_|$)/.test(name) || name.startsWith("fetch_")) return [];
    return null;
  });
  target = document.createElement("div");
  document.body.appendChild(target);
});

afterEach(() => {
  if (page) unmount(page);
  page = null;
  target.remove();
});

async function mountPage(): Promise<void> {
  page = mount(Connections, { target, props: {} });
  await settle();
}

describe("the summary over the grid", () => {
  test("counts what is connected, what works, what asks for an action, and the agents", async () => {
    // GIVEN five connections, two working, three agents among their holders
    listed = FIVE;

    // WHEN the page is shown
    await mountPage();

    // THEN the summary reads five connected, two working, three to act on, three agents
    expect(byTestid("connections-summary-all")?.textContent?.trim()).toBe("5 connected");
    expect(byTestid("connections-summary-working")?.textContent?.trim()).toBe("2 working");
    expect(byTestid("connections-summary-action")?.textContent?.trim()).toBe("3 need attention");
    expect(byTestid("connections-summary-agents")?.textContent?.trim()).toBe("3 agents use them");
  });

  test("a count filters the grid on its state, and chosen again gives every state back", async () => {
    // GIVEN the five connections on the page
    listed = FIVE;
    await mountPage();

    // WHEN Needs attention is chosen
    byTestid("connections-summary-action")?.click();
    await settle();

    // THEN only the three that ask for an action are left, and the count is pressed
    expect(tileIds()).toEqual([
      "sidebar-connection-imap:ada@example.com",
      "sidebar-connection-native:google:ada@example.com",
      "sidebar-connection-native:microsoft:ada@example.com",
    ]);
    expect(byTestid("connections-summary-action")?.getAttribute("aria-pressed")).toBe("true");
    expect(byTestid("connections-builtin-section")).toBeNull();

    // AND Working leaves the two that work
    byTestid("connections-summary-working")?.click();
    await settle();
    expect(tileIds()).toEqual(["sidebar-connection-folder:demo", "sidebar-mcp-notion"]);

    // AND choosing Working again gives the five back
    byTestid("connections-summary-working")?.click();
    await settle();
    expect(tileIds()).toHaveLength(5);
    expect(byTestid("connections-builtin-section")).not.toBeNull();
  });
});

describe("the tiles", () => {
  test("what asks for an action leads, by name, then the rest by name", async () => {
    // GIVEN the five connections, listed in another order
    listed = [...FIVE].reverse();

    // WHEN the page is shown
    await mountPage();

    // THEN the paused mailbox, the Google account to repair and the Microsoft
    // account to check lead by name, then the folder and the service
    expect(tileIds()).toEqual([
      "sidebar-connection-imap:ada@example.com",
      "sidebar-connection-native:google:ada@example.com",
      "sidebar-connection-native:microsoft:ada@example.com",
      "sidebar-connection-folder:demo",
      "sidebar-mcp-notion",
    ]);
  });

  test("each kind says its name, where it acts, its state, what it brings, who uses it and its action", async () => {
    // GIVEN one connection of each kind
    listed = FIVE;

    // WHEN the page is shown
    await mountPage();

    // THEN the service names its tools, its three agents and its last use
    expect(byTestid("connection-tile-brings-mcp:notion")?.textContent).toContain("2 tools");
    expect(byTestid("connection-tile-use-mcp:notion")?.textContent).toMatch(/3 agents\s*·\s*Last used 3 h ago/);
    expect(byTestid("connection-state-mcp:notion")?.dataset.status).toBe("working");
    // AND the Google account shows the Google logo, its address, Sign in again
    expect(byTestid("connection-logo-native:google:ada@example.com")?.querySelector("path")?.getAttribute("d")).toBe(
      brandLogoPath("google"),
    );
    expect(byTestid("connection-tile-where-native:google:ada@example.com")?.textContent?.trim()).toBe("ada@example.com");
    expect(byTestid("connection-tile-repair-native:google:ada@example.com")?.dataset.repair).toBe("sign_in");
    // AND the Microsoft account shows the Microsoft logo, says its services
    // in words, and offers Check in its foot
    expect(byTestid("connection-logo-native:microsoft:ada@example.com")?.querySelector("path")?.getAttribute("d")).toBe(
      brandLogoPath("microsoft"),
    );
    expect(byTestid("connection-kind-icon-native:microsoft:ada@example.com")).toBeNull();
    expect(byTestid("connection-tile-brings-native:microsoft:ada@example.com")?.textContent).toContain(
      "Mail · Calendar · OneDrive",
    );
    const check = byTestid("connection-state-native:microsoft:ada@example.com-check");
    expect(check).not.toBeNull();
    // the one action of a tile never checked sits in its foot, not in its state line
    expect(byTestid("connection-state-native:microsoft:ada@example.com")?.contains(check ?? null)).toBe(false);
    // AND the folder shows its path, its files and its one agent
    expect(byTestid("connection-tile-where-folder:demo")?.textContent?.trim()).toBe("/Users/ada/Demo");
    expect(byTestid("connection-tile-brings-folder:demo")?.textContent).toContain("Files");
    expect(byTestid("connection-tile-use-folder:demo")?.textContent).toContain("1 agent");
    // AND the mailbox, paused, says its mail; no technical kind is shown
    expect(byTestid("connection-tile-brings-imap:ada@example.com")?.textContent).toContain("Mail");
    expect(byTestid("connection-state-imap:ada@example.com")?.dataset.status).toBe("paused");
    expect(target.querySelector('[data-testid^="connection-provenance-"]')).toBeNull();
    expect(target.textContent).not.toMatch(/MCP · |folder · /);
  });

  test("the technical kind and origin are shown in developer mode only", async () => {
    // GIVEN the developer mode on
    listed = FIVE;
    developerMode.set(true);

    // WHEN the page is shown
    await mountPage();

    // THEN each tile gives its kind and origin
    expect(byTestid("connection-provenance-folder:demo")?.textContent).toContain("folder · custom");
    expect(byTestid("connection-provenance-mcp:notion")?.textContent).toContain("MCP · selection");
  });

  test("a service of the selection is named by its label, never by the name of its server", async () => {
    // GIVEN a service whose server carries a technical name, added from the Notion entry
    listed = [connection({ id: "mcp:com-notion-mcp", label: "com-notion-mcp" })];
    enrichments = [
      {
        package_identifier: "@notionhq/notion-mcp-server",
        enrichment: {
          operator_label: { en: "Notion" },
          category: "productivity",
          icon_name: "notion",
          registry_names: ["com.notion/mcp"],
          trust_level: "verified_official",
          auth_help_url: null,
          auth_help_text: null,
          auth_help_i18n_key: null,
          oauth_pre_registered_client_id_env: null,
        },
      },
    ];

    // WHEN the page is shown
    await mountPage();

    // THEN its tile reads Notion, with the Notion logo
    expect(byTestid("connection-tile-name-mcp:com-notion-mcp")?.textContent?.trim()).toBe("Notion");
    expect(byTestid("connection-logo-mcp:com-notion-mcp")).not.toBeNull();
  });

  test("a tile opens the card of its connection, and its agents and activity are read once each", async () => {
    // GIVEN the page on its five connections
    listed = FIVE;
    await mountPage();
    const agentReads = invoke.mock.calls.filter(([name]) => name === "get_connection_agents").length;

    // WHEN the tile of the folder is chosen
    byTestid("sidebar-connection-folder:demo")?.click();
    await settle();

    // THEN its card opens beside the grid, and the grid read one agents
    // command per connection, five in all, before the card read its own
    expect(byTestid("connector-detail")).not.toBeNull();
    expect(byTestid("connection-detail-title")?.textContent).toContain("Demo");
    expect(agentReads).toBe(5);
  });
});

describe("the empty page", () => {
  test("invites to add a connection, with three services of the selection", async () => {
    // GIVEN no connection, and the selection read
    listed = [];
    const entry = (identifier: string, label: string, icon: string) => ({
      package_identifier: identifier,
      enrichment: {
        operator_label: { en: label },
        category: "productivity",
        icon_name: icon,
        trust_level: "verified_official",
        auth_help_url: null,
        auth_help_text: null,
        auth_help_i18n_key: null,
        oauth_pre_registered_client_id_env: null,
      },
    });
    enrichments = [
      entry("com.notion/mcp", "Notion", "notion"),
      entry("com.slack/mcp", "Slack", "slack"),
      entry("io.github.github/github-mcp-server", "GitHub", "github"),
      entry("app.linear/mcp", "Linear", "linear"),
      entry("com.stripe/mcp", "Stripe", "stripe"),
    ];

    // WHEN the page is shown
    await mountPage();

    // THEN the invitation offers Notion, GitHub and Linear, and the add action;
    // no summary is drawn over an empty grid
    expect(byTestid("connections-sidebar-empty")).not.toBeNull();
    const offered = [...target.querySelectorAll('[data-testid^="connections-empty-suggestion-"]')].map((el) =>
      el.textContent?.trim(),
    );
    expect(offered).toEqual(["Notion", "GitHub", "Linear"]);
    expect(byTestid("connections-empty-add")).not.toBeNull();
    expect(byTestid("connections-summary")).toBeNull();
  });
});
