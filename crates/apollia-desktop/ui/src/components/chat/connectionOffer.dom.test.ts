import { describe, test, expect, vi, beforeEach, afterEach } from "vitest";
import { flushSync, mount, unmount } from "svelte";
import { get } from "svelte/store";
import { locale, waitLocale } from "svelte-i18n";

const invoke = vi.hoisted(() => vi.fn(async (..._args: unknown[]): Promise<unknown> => null));
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

import "$lib/i18n";
import { developerMode } from "$lib/stores/developerMode";
import { currentRoute } from "$lib/stores/navigation";
import { pendingConnectionAdd } from "$lib/stores/connectionAdd";
import { onRegenerateLast } from "$lib/stores/chatRegenerate";
import type { ReasoningItem } from "$lib/chat/reasoning";
import type { Connection, RegistryServerView } from "$lib/types";
import ToolStep from "./ToolStep.svelte";

// The dialogs run Svelte transitions, which jsdom cannot play: an animation
// that finishes at once stands in for them.
Element.prototype.animate = function (): Animation {
  const animation = {
    onfinish: null as (() => void) | null,
    cancel: () => {},
    finished: Promise.resolve(),
  };
  setTimeout(() => animation.onfinish?.(), 0);
  return animation as unknown as Animation;
};

// A step where Apollia looked for a connection, mounted in the DOM with the
// commands answered here: what is held is what the person sees under the step,
// and which commands their gestures reach.

type ToolCallItem = Extract<ReasoningItem, { kind: "tool_call" }>;

const NOTION: RegistryServerView = {
  name: "com.notion/mcp",
  title: "Notion",
  description: "Pages and databases",
  version: "1.0.0",
  repository_url: null,
  website_url: null,
  packages: null,
  trust_level: "verified_official",
  category: "productivity",
  enrichment: null,
  is_installed: false,
  remotes: [{ type: "streamable-http", url: "https://mcp.notion.com/mcp", headers: [] }],
};

function lookup(matches: Record<string, unknown>[]): ToolCallItem {
  return {
    id: "m-tc-0",
    kind: "tool_call",
    status: "success",
    tool: "connection_lookup",
    args: { query: "notion pages" },
    output: JSON.stringify({ matches, total: matches.length }),
    duration_ms: 12,
  };
}

const NOT_CONNECTED = {
  entry: "com.notion/mcp",
  label: "Notion",
  description: "Pages and databases",
  status: "not_connected",
  summary: "not connected: com.notion/mcp",
};

const NOT_GRANTED = {
  entry: "com.notion/mcp",
  label: "Notion",
  description: "Pages and databases",
  status: "connected_not_granted",
  connection: "mcp:notion",
  summary: "connected but not granted to you: mcp:notion",
};

function connection(id: string, tools: string[]): Connection {
  return {
    id,
    kind: "mcp",
    label: "Notion",
    account: null,
    provenance: "selection",
    remote: true,
    official: true,
    state: "connected",
    status: "working",
    reason: null,
    checkedAt: null,
    package: null,
    tools: tools.map((tool) => ({ id: tool, family: "read", familySource: "server", prechecked: true })),
  } as unknown as Connection;
}

let answers: Record<string, (args: Record<string, unknown>) => unknown> = {};
let target: HTMLElement;
let mounted: ReturnType<typeof mount> | null = null;
let stopListening: (() => void) | null = null;
let regenerated: string[] = [];

async function settle(rounds = 20): Promise<void> {
  for (let i = 0; i < rounds; i++) {
    await new Promise((resolve) => setTimeout(resolve, 0));
    flushSync();
  }
}

/** Dialogs may render outside the mount target, so the whole document is searched. */
function byTestid(id: string): HTMLElement | null {
  return document.querySelector(`[data-testid="${id}"]`);
}

async function click(id: string): Promise<void> {
  const element = byTestid(id);
  if (!element) throw new Error(`no element ${id}`);
  element.click();
  await settle();
}

/** "Connect" on the offer: the flow is loaded once asked for, so it is waited for. */
async function openFlow(): Promise<void> {
  await click("connection-offer-add");
  await vi.waitFor(
    () => {
      flushSync();
      if (!byTestid("connector-wizard")) throw new Error("the add flow is not open");
    },
    { timeout: 5000 },
  );
  await settle();
}

function calls(command: string): Record<string, unknown>[] {
  return invoke.mock.calls
    .filter(([name]) => name === command)
    .map(([, args]) => (args ?? {}) as Record<string, unknown>);
}

async function mountStep(item: ToolCallItem): Promise<void> {
  mounted = mount(ToolStep, {
    target,
    props: { item, skin: "operator", expanded: false, ontoggle: () => {}, sessionId: "session-1" },
  });
  await settle();
}

beforeEach(async () => {
  locale.set("en");
  await waitLocale("en");
  developerMode.set(false);
  currentRoute.set("chat");
  pendingConnectionAdd.set(null);
  invoke.mockReset();
  answers = { fetch_mcp_curated: () => [NOTION], meta_generate_capabilities_coaching: () => [] };
  invoke.mockImplementation(async (command: unknown, args: unknown) => {
    const answer = answers[String(command)];
    if (answer) return answer((args ?? {}) as Record<string, unknown>);
    if (/(^|_)list(_|$)/.test(String(command))) return [];
    return null;
  });
  regenerated = [];
  stopListening = onRegenerateLast(
    () => "session-1",
    () => regenerated.push("session-1"),
  );
  target = document.createElement("div");
  document.body.appendChild(target);
});

afterEach(() => {
  if (mounted) unmount(mounted);
  mounted = null;
  stopListening?.();
  target.remove();
});

describe("a service Apollia lacks", () => {
  test("the step offers to connect it, and the add flow opens on its entry in the conversation", async () => {
    // GIVEN a step where Apollia found Notion not connected
    await mountStep(lookup([NOT_CONNECTED]));
    const offer = byTestid("connection-offer");
    expect(offer?.dataset.kind).toBe("add");
    expect(byTestid("connection-offer-add")?.textContent).toContain("Connect Notion");

    // WHEN "Connect Notion" is clicked
    await openFlow();

    // THEN the add flow opens on Notion's warning, over the Chat, which is not left
    const flow = byTestid("connector-wizard");
    expect(flow?.dataset.kind).toBe("catalogue");
    expect(flow?.textContent).toContain("Notion");
    expect(byTestid("wizard-warning-accept")).not.toBeNull();
    expect(get(currentRoute)).toBe("chat");
    expect(get(pendingConnectionAdd)).toBeNull();
    expect(calls("add_mcp_server")).toHaveLength(0);
  });

  test("once added, its tools are proposed to the conversation and the request is played again", async () => {
    // GIVEN the add flow open on Notion from the step, a service that answers
    let added = "";
    answers.test_mcp_connection = () => ({ kind: "success" });
    answers.add_mcp_server = (args) => {
      added = String((args.config as { name: string }).name);
      return null;
    };
    answers.list_connections = () =>
      added ? [connection(`mcp:${added}`, [`mcp:${added}/search`, `mcp:${added}/create_page`])] : [];
    answers.get_chat_session_settings = () => ({ temperature: 0.4, proposed_accesses: ["file_write"] });
    answers.set_chat_session_settings = (args) => args.settings;
    answers.test_mcp_live_server = () => ({ kind: "success", tools: [{}, {}], live_health: null });
    await mountStep(lookup([NOT_CONNECTED]));
    await openFlow();

    // WHEN the addition is accepted, Apollia kept as who may use it, and the flow closed
    await click("wizard-warning-accept");
    await click("add-flow-who-continue");
    await click("wizard-done-btn");

    // THEN the tools of the new connection are proposed to this conversation,
    // its other settings kept, and the request is played again
    expect(added).not.toBe("");
    const written = calls("set_chat_session_settings");
    expect(written).toHaveLength(1);
    expect(written[0]).toEqual({
      sessionId: "session-1",
      settings: {
        temperature: 0.4,
        proposed_accesses: ["file_write", `mcp:${added}/search`, `mcp:${added}/create_page`],
      },
    });
    expect(regenerated).toEqual(["session-1"]);
    expect(byTestid("connector-wizard")).toBeNull();
    expect(byTestid("connection-offer-done")).not.toBeNull();
  });

  test("a flow cancelled before anything is added plays nothing again", async () => {
    // GIVEN the add flow open on Notion from the step
    await mountStep(lookup([NOT_CONNECTED]));
    await openFlow();

    // WHEN it is cancelled on its warning
    await click("wizard-cancel-btn");

    // THEN the step offers Notion again, and nothing was written or played
    expect(byTestid("connector-wizard")).toBeNull();
    expect(byTestid("connection-offer-add")).not.toBeNull();
    expect(calls("set_chat_session_settings")).toHaveLength(0);
    expect(regenerated).toEqual([]);
  });
});

describe("a service connected but not granted to Apollia", () => {
  test("the step offers to grant it, without opening the add flow", async () => {
    // GIVEN a step where Apollia found Notion connected but not granted to it
    answers.list_connections = () => [connection("mcp:notion", ["mcp:notion/search"])];
    await mountStep(lookup([NOT_GRANTED]));
    expect(byTestid("connection-offer")?.dataset.kind).toBe("grant");
    expect(byTestid("connection-offer-add")).toBeNull();

    // WHEN "Allow Apollia" is clicked
    await click("connection-offer-grant");

    // THEN Notion is granted to Apollia, no add flow opened, and the request is played again
    expect(calls("grant_connection")).toEqual([
      { connectionId: "mcp:notion", agents: ["apollia:chat"], tools: ["mcp:notion/search"] },
    ]);
    expect(byTestid("connector-wizard")).toBeNull();
    expect(calls("add_mcp_server")).toHaveLength(0);
    expect(regenerated).toEqual(["session-1"]);
    expect(byTestid("connection-offer-done")).not.toBeNull();
  });

  test("a service already granted offers nothing", async () => {
    // GIVEN a step where Notion is connected and granted to Apollia
    await mountStep(lookup([{ ...NOT_GRANTED, status: "granted" }]));

    // WHEN the step is shown
    const action = byTestid("tool-step-action")?.textContent;

    // THEN it reads in plain words, and no offer stands under it
    expect(action).toBe("Look for a connection");
    expect(byTestid("connection-offers")).toBeNull();
  });
});
