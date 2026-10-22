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
import { forgetRegenerableAnswer, onRegenerateLast, setRegenerableAnswer } from "$lib/stores/chatRegenerate";
import { answerRequested } from "$lib/chat/chatBehaviours";
import type { ReasoningItem } from "$lib/chat/reasoning";
import type { ChatMessageView, Connection, RegistryServerView } from "$lib/types";
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
// The thread of the conversation the step is in: its last answer, `m`, holds
// the step. The conversation regenerates as `ChatConversation` does.
let thread: Pick<ChatMessageView, "id" | "role">[] = [];

/** A message of the operator follows the answer of the step. */
function operatorWrites(): void {
  thread = [...thread, { id: "u-2", role: "user" }];
  setRegenerableAnswer("session-1", null);
  flushSync();
}

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
    props: { item, skin: "operator", expanded: false, ontoggle: () => {}, sessionId: "session-1", messageId: "m" },
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
  thread = [
    { id: "u-1", role: "user" },
    { id: "m", role: "assistant" },
  ];
  setRegenerableAnswer("session-1", "m");
  stopListening = onRegenerateLast(
    () => "session-1",
    (answerId) => {
      const id = answerRequested(thread, false, answerId);
      if (!id) return false;
      regenerated.push(id);
      return true;
    },
  );
  target = document.createElement("div");
  document.body.appendChild(target);
});

afterEach(() => {
  if (mounted) unmount(mounted);
  mounted = null;
  stopListening?.();
  forgetRegenerableAnswer("session-1");
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
    expect(regenerated).toEqual(["m"]);
    expect(byTestid("connector-wizard")).toBeNull();
    expect(byTestid("connection-offer-done")?.textContent).toContain("Done, the request is played again");
  });

  test("once added after the operator wrote again, nothing is played again and the card says so", async () => {
    // GIVEN the add flow open on Notion from the step, a service that answers
    let added = "";
    answers.test_mcp_connection = () => ({ kind: "success" });
    answers.add_mcp_server = (args) => {
      added = String((args.config as { name: string }).name);
      return null;
    };
    answers.list_connections = () => (added ? [connection(`mcp:${added}`, [`mcp:${added}/search`])] : []);
    answers.get_chat_session_settings = () => ({});
    answers.set_chat_session_settings = (args) => args.settings;
    answers.test_mcp_live_server = () => ({ kind: "success", tools: [{}], live_health: null });
    await mountStep(lookup([NOT_CONNECTED]));
    await openFlow();

    // WHEN the operator writes a new message meanwhile, then the addition ends
    operatorWrites();
    await click("wizard-warning-accept");
    await click("add-flow-who-continue");
    await click("wizard-done-btn");

    // THEN the tools are proposed, nothing is played again, and the card does
    // not say the request is
    expect(calls("set_chat_session_settings")).toHaveLength(1);
    expect(regenerated).toEqual([]);
    const done = byTestid("connection-offer-done")?.textContent ?? "";
    expect(done).toContain("Done. Send your request again to use it");
    expect(done).not.toContain("played again");
  });

  test("a flow closed before anything is added plays nothing again", async () => {
    // GIVEN the add flow open on Notion from the step
    await mountStep(lookup([NOT_CONNECTED]));
    await openFlow();

    // WHEN the dialog is closed on its warning
    await click("dialog-close");

    // THEN the step offers Notion again, idle, and nothing was written or played
    expect(byTestid("connector-wizard")).toBeNull();
    expect(byTestid("connection-offer")?.dataset.state).toBe("idle");
    expect(byTestid("connection-offer-add")).not.toBeNull();
    expect(calls("set_chat_session_settings")).toHaveLength(0);
    expect(regenerated).toEqual([]);
  });

  test("under an answer another turn has followed, the step offers nothing", async () => {
    // GIVEN a step where Apollia found Notion not connected, then the operator wrote again
    await mountStep(lookup([NOT_CONNECTED]));
    expect(byTestid("connection-offer-add")).not.toBeNull();

    // WHEN the answer of the step is no longer the last one
    operatorWrites();

    // THEN neither "Connect" nor "Allow Apollia" stands under it
    expect(byTestid("connection-offers")).toBeNull();
    expect(byTestid("connection-offer-add")).toBeNull();
    expect(byTestid("connection-offer-grant")).toBeNull();
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
    expect(regenerated).toEqual(["m"]);
    expect(byTestid("connection-offer-done")).not.toBeNull();
  });

  test("under an older answer, the step does not offer to grant it", async () => {
    // GIVEN a step where Notion is connected but not granted, under an answer
    // another turn has followed
    setRegenerableAnswer("session-1", "m-later");
    await mountStep(lookup([NOT_GRANTED]));

    // WHEN the step is shown
    const offers = byTestid("connection-offers");

    // THEN no offer stands under it, and nothing was granted
    expect(offers).toBeNull();
    expect(byTestid("connection-offer-grant")).toBeNull();
    expect(calls("grant_connection")).toHaveLength(0);
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
