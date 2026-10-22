import { describe, test, expect, vi, beforeEach, afterEach } from "vitest";
import { flushSync, mount, unmount } from "svelte";
import { get } from "svelte/store";
import { locale, waitLocale } from "svelte-i18n";

const invoke = vi.hoisted(() => vi.fn(async (..._args: unknown[]): Promise<unknown> => null));
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

import "$lib/i18n";
import { developerMode } from "$lib/stores/developerMode";
import { takeRequestedJournalConnection } from "$lib/stores/journalFocus";
import { currentRoute } from "$lib/stores/navigation";
import type { ConnectionActivity, ConnectionAgents } from "$lib/ipc/connections";
import type { Connection, ConnectionProvenance } from "$lib/types";
import ConnectionCard from "./ConnectionCard.svelte";

const ID = "mcp:acme";

function service(provenance: ConnectionProvenance = "custom"): Connection {
  return {
    id: ID,
    kind: "mcp",
    label: "Acme",
    account: null,
    provenance,
    remote: true,
    official: false,
    state: "connected",
    status: "working",
    reason: null,
    checkedAt: null,
    package: null,
    tools: [
      { id: "mcp:acme/search", family: "read", familySource: "server", prechecked: true },
      { id: "mcp:acme/create_page", family: "unclassified", familySource: "none", prechecked: false },
    ],
  } as Connection;
}

const HOLDERS: ConnectionAgents = {
  connection: ID,
  agents: [{ id: "writer", name: "writer", enabled: true, byDefault: false, tools: ["mcp:acme/search"] }],
  sharedWith: [],
};

const ACTIVITY: ConnectionActivity = {
  toolPrefix: "mcp:acme/",
  since: "2026-10-15T12:00:00Z",
  until: null,
  actions: 5,
  lastUsedAt: new Date(Date.now() - 3 * 3_600_000).toISOString(),
  agents: [
    { agent: "writer", actions: 4, lastUsedAt: new Date(Date.now() - 3 * 3_600_000).toISOString() },
    { agent: "apollia:chat", actions: 1, lastUsedAt: new Date(Date.now() - 5 * 3_600_000).toISOString() },
  ],
};

let target: HTMLElement;
let card: ReturnType<typeof mount> | null = null;

async function settle(): Promise<void> {
  for (let i = 0; i < 8; i++) {
    await Promise.resolve();
    flushSync();
  }
}

function byTestid(id: string): HTMLElement | null {
  return target.querySelector(`[data-testid="${id}"]`);
}

async function click(id: string): Promise<void> {
  const element = byTestid(id);
  if (!element) throw new Error(`no element ${id}`);
  element.click();
  await settle();
}

function callsOf(command: string): unknown[] {
  return invoke.mock.calls.filter(([name]) => name === command).map(([, args]) => args);
}

beforeEach(async () => {
  locale.set("en");
  await waitLocale("en");
  developerMode.set(false);
  takeRequestedJournalConnection();
  currentRoute.set("integrations");
  invoke.mockReset();
  invoke.mockImplementation(async (command: unknown, args: unknown) => {
    switch (command) {
      case "get_connection_agents":
        return HOLDERS;
      case "get_connection_activity":
        return ACTIVITY;
      case "list_agents":
        return [{ id: null, name: "writer" }, { id: null, name: "analyst" }];
      case "grant_connection": {
        const { agents } = args as { agents: string[] };
        return {
          ...HOLDERS,
          agents: [
            ...HOLDERS.agents,
            ...agents.map((id) => ({ id, name: id, enabled: true, byDefault: false, tools: ["mcp:acme/search", "mcp:acme/create_page"] })),
          ],
        };
      }
      case "set_connection_tool_family": {
        const { family } = args as { family: string };
        const updated = service();
        updated.tools = updated.tools.map((t) =>
          t.id === "mcp:acme/create_page" ? { ...t, family: family as never, familySource: "operator" } : t,
        );
        return updated;
      }
      default:
        return null;
    }
  });
  target = document.createElement("div");
  document.body.appendChild(target);
});

afterEach(() => {
  if (card) unmount(card);
  card = null;
  target.remove();
});

async function mountCard(connection: Connection, onupdated: (c: Connection) => void = () => {}): Promise<void> {
  const noop = () => {};
  card = mount(ConnectionCard, {
    target,
    props: { connection, label: "Acme", onupdated, onremoved: noop, onreload: noop, onrepair: noop },
  });
  await settle();
}

describe("the agents of a connection, and Give to an agent", () => {
  test("the card lists the agents the runtime says use it", async () => {
    // GIVEN a service one agent uses
    // WHEN its card opens
    await mountCard(service());

    // THEN the agents of the connection are read for it, and the agent is listed with its tools
    expect(callsOf("get_connection_agents")).toEqual([{ connectionId: ID }]);
    expect(byTestid("connection-agent-writer")?.textContent).toContain("writer");
    expect(byTestid("connection-agent-writer")?.textContent).toContain("1 tool");
  });

  test("Give to an agent grants every tool to the agents checked, in one call", async () => {
    // GIVEN the card of a service one agent uses
    await mountCard(service());

    // WHEN Give to an agent is opened, one agent checked and the grant confirmed
    await click("connection-give-btn");
    expect(byTestid("connection-give-agent-writer")).toBeNull();
    expect(byTestid("connection-give-agent-apollia:chat")).not.toBeNull();
    await click("connection-give-agent-analyst");
    await click("connection-give-confirm");

    // THEN the grant names the connection, the agent and every tool, and the list shows the new agent
    expect(callsOf("grant_connection")).toEqual([
      { connectionId: ID, agents: ["analyst"], tools: ["mcp:acme/search", "mcp:acme/create_page"] },
    ]);
    expect(byTestid("connection-agent-analyst")).not.toBeNull();
    expect(byTestid("connection-give-confirm")).toBeNull();
  });
});

describe("the activity of a connection over seven days", () => {
  test("the last use and the actions per agent are read by the prefix of its tools", async () => {
    // GIVEN a service used by two agents this week
    // WHEN its card opens
    await mountCard(service());

    // THEN the activity is read for its prefix since seven days, and said per agent
    const [args] = callsOf("get_connection_activity") as { toolPrefix: string; since: string }[];
    expect(args?.toolPrefix).toBe("mcp:acme/");
    const days = (Date.now() - Date.parse(args?.since ?? "")) / 86_400_000;
    expect(Math.round(days)).toBe(7);
    expect(byTestid("connection-activity-last")?.textContent).toContain("Last used 3 h ago");
    const rows = target.querySelectorAll('[data-testid="connection-activity-agent"]');
    expect([...rows].map((row) => row.textContent?.replace(/\s+/g, " ").trim())).toEqual([
      "writer 4 actions",
      "Apollia 1 action",
    ]);
  });

  test("See in the journal opens the journal narrowed to the connection", async () => {
    // GIVEN the card of a service
    await mountCard(service());

    // WHEN See in the journal is clicked
    await click("connection-activity-journal");

    // THEN the journal is asked for its prefix and name, and shown
    expect(takeRequestedJournalConnection()).toEqual({ prefix: "mcp:acme/", label: "Acme" });
    expect(get(currentRoute)).toBe("journal");
  });
});

describe("the tools of a connection and their family", () => {
  test("each tool says its family in plain words, with the developer mode off", async () => {
    // GIVEN a service whose server is not running, so its tools come from the list
    // WHEN its card opens
    await mountCard(service());

    // THEN each tool has its family named
    expect(byTestid("tool-family-search")?.textContent?.trim()).toBe("Read");
    expect(byTestid("tool-family-create_page")?.textContent?.trim()).toBe("Unclassified");
  });

  test("Change on a service outside the selection gives the tool the family chosen", async () => {
    // GIVEN the card of a service added by its address
    const onupdated = vi.fn();
    await mountCard(service("custom"), onupdated);

    // WHEN Change is clicked on a tool, a family chosen and applied
    await click("tool-family-change-create_page");
    const select = byTestid("tool-family-select-create_page") as HTMLSelectElement | null;
    if (!select) throw new Error("no family select");
    select.value = "write_or_delete";
    select.dispatchEvent(new Event("change"));
    await settle();
    await click("tool-family-apply-create_page");

    // THEN the runtime is asked to reclassify the tool by its local name, and the card hears the answer
    expect(callsOf("set_connection_tool_family")).toEqual([
      { connectionId: ID, tool: "create_page", family: "write_or_delete" },
    ]);
    expect(onupdated).toHaveBeenCalledTimes(1);
    const updated = onupdated.mock.calls[0]?.[0] as Connection;
    expect(updated.tools.find((t) => t.id === "mcp:acme/create_page")?.familySource).toBe("operator");
  });

  test("a tool of the list is named in plain words, its identifier only in developer mode", async () => {
    // GIVEN a service whose server is not running, with the developer mode off
    // WHEN its card opens
    await mountCard(service());

    // THEN the tool reads by its name, and its identifier is not shown
    expect(byTestid("connection-tool-name-mcp:acme/create_page")?.textContent?.trim()).toBe("Create page");
    expect(byTestid("connection-tool-mcp:acme/create_page")?.textContent).not.toContain("mcp:acme/create_page");

    // WHEN the developer mode is turned on
    developerMode.set(true);
    await settle();

    // THEN the identifier is shown beside the name
    expect(byTestid("connection-tool-id-mcp:acme/create_page")?.textContent?.trim()).toBe("mcp:acme/create_page");
  });

  test("a service from the selection offers no Change", async () => {
    // GIVEN the card of a service from the selection
    // WHEN it opens
    await mountCard(service("selection"));

    // THEN its families are shown and none can be changed
    expect(byTestid("tool-family-search")).not.toBeNull();
    expect(byTestid("tool-family-change-search")).toBeNull();
    expect(byTestid("tool-family-change-create_page")).toBeNull();
  });
});
