import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, test, expect, vi, beforeAll } from "vitest";
import { render as renderToString } from "svelte/server";
import { waitLocale } from "svelte-i18n";
import "$lib/i18n";

// Stores read `localStorage` at import time; the node environment has none.
vi.hoisted(() => {
  const memory = new Map<string, string>();
  globalThis.localStorage = {
    getItem: (k: string) => memory.get(k) ?? null,
    setItem: (k: string, v: string) => void memory.set(k, v),
    removeItem: (k: string) => void memory.delete(k),
    clear: () => memory.clear(),
    key: (i: number) => Array.from(memory.keys())[i] ?? null,
    get length() {
      return memory.size;
    },
  };
});

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => null) }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
  emit: vi.fn(async () => {}),
}));
vi.mock("@tauri-apps/plugin-notification", () => ({
  isPermissionGranted: vi.fn(async () => false),
  requestPermission: vi.fn(async () => "denied"),
  sendNotification: vi.fn(),
}));

import ChatIntegrationsTab, {
  profileIdOf,
  profileSelection,
  profileSwitchOf,
  writeProfileSwitch,
} from "./ChatIntegrationsTab.svelte";
import { chatConversationStats } from "$lib/stores/chat";
import type { ProfileSummaryView } from "$lib/ipc/profile";
import { developerMode } from "$lib/stores/developerMode";
import type { ChatSessionDetail } from "$lib/types";
import type { SessionSettings } from "$lib/chat/sessionSettings";
import {
  createSessionAccesses,
  type SessionAccessesController,
} from "./integrations/useSessionAccesses.svelte";

function libreSession(): ChatSessionDetail {
  return {
    id: "s-1",
    mode: "libre",
    agent_name: null,
    system_prompt: "",
    status: "active",
    available_tools: [],
    authorized_tools: [],
    messages: [],
    created_at: "2026-10-01T00:00:00Z",
    closed_at: null,
    llm_backend: null,
    title: null,
    plan_mode: false,
    plan_phase: "discovery",
  };
}

function agentSession(): ChatSessionDetail {
  return { ...libreSession(), mode: "agent", agent_name: "accountant" };
}

/** A controller read against a runtime that holds `settings` for the conversation. */
async function loaded(
  settings: SessionSettings = {},
  sources: string[] = [],
): Promise<SessionAccessesController> {
  const controller = createSessionAccesses(
    {
      getSettings: async () => settings,
      setSettings: async (_id, next) => next,
      getAgent: async () => ({ access: ["web_search", "mcp:gmail/draft_message"], sources }),
      listTools: async () => [
        { name: "web_search", description: "Search the web", kind: "native" },
        { name: "bash_executor", description: "Run a command", kind: "native" },
        { name: "mcp:gmail/draft_message", description: "Prepare a message", kind: "mcp" },
      ],
      listTargets: async () => [
        { kind: "agent", item_id: "market-watch", name: "Market watch", description: "" },
        { kind: "workflow", item_id: "sales/follow_up", name: "Quote follow-up", description: "" },
      ],
    },
    new Map([
      ["web_search", { nameKey: "tools.catalog.web_search.label_operator", detailKey: "tools.catalog.web_search.desc_operator" }],
    ]),
  );
  await controller.load("s-1", "accountant");
  return controller;
}

/**
 * Renders the panel through Svelte's server renderer. Vitest runs this suite
 * in its node environment, where `svelte` resolves to the server build and
 * `mount` is unavailable; the markup is what this suite asserts on, and the
 * server render walks the same template branches.
 */
function render(
  session: ChatSessionDetail | null = libreSession(),
  controller?: SessionAccessesController,
): string {
  return renderToString(ChatIntegrationsTab, {
    props: { session, onupdated: () => {}, ...(controller ? { controller } : {}) },
  }).body;
}

/** The opening tag carrying `data-testid="<testid>"`, or `null`. */
function tagOf(html: string, testid: string): string | null {
  const match = html.match(new RegExp(`<[^>]*data-testid="${testid}"[^>]*>`));
  return match ? match[0] : null;
}

/** The opening tag of the line of the access `id`, or `null`. */
function lineOf(html: string, id: string): string | null {
  const match = html.match(new RegExp(`<li[^>]*data-access="${id}"[^>]*>`));
  return match ? match[0] : null;
}

/** The markup of the line of the access `id`, up to its closing tag. */
function lineBody(html: string, id: string): string {
  const start = html.indexOf(`data-access="${id}"`);
  return start < 0 ? "" : html.slice(start, html.indexOf("</li>", start));
}

beforeAll(async () => {
  await waitLocale();
});

describe("ChatIntegrationsTab - the accesses of the agent, group by group", () => {
  test("the four groups are listed, with a filter by name", async () => {
    // GIVEN a conversation whose agent holds a tool and a tool of a connection
    developerMode.set(false);
    const controller = await loaded();

    // WHEN the tab renders
    const html = render(agentSession(), controller);
    const kinds = [...html.matchAll(/data-testid="session-access-group" data-kind="([a-z]+)"/g)].map((m) => m[1]);

    // THEN tools, connections, workflows and other agents come in order, under a filter
    expect(kinds).toEqual(["tools", "connections", "workflows", "agents"]);
    expect(tagOf(html, "session-access-filter")).not.toBeNull();
    expect(html.indexOf('data-testid="session-access-filter"')).toBeLessThan(
      html.indexOf('data-testid="session-access-group"'),
    );
  });

  test("the raw identifiers show in developer mode only", async () => {
    // GIVEN the same conversation
    const controller = await loaded();

    // WHEN the tab renders with the developer mode off, then on
    developerMode.set(false);
    const plain = render(agentSession(), controller);
    developerMode.set(true);
    const developer = render(agentSession(), controller);
    developerMode.set(false);

    // THEN no identifier is read by a business user, and each line names its own in developer mode
    expect(plain).not.toContain('data-testid="session-access-id"');
    expect(plain).not.toContain("bash_executor</code>");
    expect(lineBody(developer, "bash_executor")).toContain("bash_executor");
    expect(lineBody(developer, "mcp:gmail/draft_message")).toContain('data-testid="session-access-id"');
  });

  test("an access cut for the conversation says so, its switch off", async () => {
    // GIVEN a conversation that switched the web search off
    developerMode.set(false);
    const controller = await loaded({ disabled_accesses: ["web_search"] });

    // WHEN the tab renders
    const html = render(agentSession(), controller);

    // THEN the line is marked off here, and the access the agent keeps is not marked
    expect(lineOf(html, "web_search")).toContain('data-state="off"');
    expect(lineBody(html, "web_search")).toContain('data-testid="session-access-badge"');
    expect(lineBody(html, "web_search")).toContain('aria-checked="false"');
    expect(lineOf(html, "mcp:gmail/draft_message")).toContain('data-state="on"');
    expect(lineBody(html, "mcp:gmail/draft_message")).not.toContain('data-testid="session-access-badge"');
  });

  test("an access the agent does not hold is listed, and reads proposed once switched on", async () => {
    // GIVEN a conversation that proposes another agent, and not the command tool
    developerMode.set(false);
    const controller = await loaded({ proposed_accesses: ["agent:market-watch"] });

    // WHEN the tab renders
    const html = render(agentSession(), controller);

    // THEN both are listed with their state, only the proposed one switched on
    expect(lineOf(html, "bash_executor")).toContain('data-state="not_granted"');
    expect(lineBody(html, "bash_executor")).toContain('aria-checked="false"');
    expect(lineOf(html, "agent:market-watch")).toContain('data-state="proposed"');
    expect(lineBody(html, "agent:market-watch")).toContain('aria-checked="true"');
  });

  test("the former tool selector and its Save button are gone", async () => {
    // GIVEN a free conversation, where the selector used to be
    developerMode.set(true);
    const controller = await loaded();

    // WHEN the tab renders
    const html = render(libreSession(), controller);
    developerMode.set(false);

    // THEN no control of the selector is left, and nothing waits for a save
    expect(html).not.toContain("config-tool-");
    expect(html).not.toContain("config-group-");
    expect(html).not.toContain("config-save");
  });
});

describe("ChatIntegrationsTab - the profile switch names its case", () => {
  test("with the built-in assistant the profile is known, with an agent it may be read", async () => {
    // GIVEN a free conversation and a conversation with an installed agent
    developerMode.set(false);
    const controller = await loaded({}, ["profile"]);

    // WHEN each renders its tab
    const assistant = render(libreSession(), controller);
    const agent = render(agentSession(), controller);

    // THEN each carries the switch of its case, and never the other
    expect(tagOf(assistant, "session-profile-switch")).toContain('data-case="assistant"');
    expect(tagOf(assistant, "toggle-user-memory")).not.toBeNull();
    expect(tagOf(assistant, "toggle-profile-access")).toBeNull();
    expect(tagOf(agent, "session-profile-switch")).toContain('data-case="agent"');
    expect(tagOf(agent, "toggle-profile-access")).toContain('aria-checked="true"');
    expect(tagOf(agent, "toggle-user-memory")).toBeNull();
    // AND the two labels differ
    const label = (html: string) => html.slice(html.indexOf('data-testid="session-profile-switch"')).match(/<span[^>]*>([^<]+)<\/span>/)?.[1];
    expect(label(assistant)).toBeTruthy();
    expect(label(assistant)).not.toBe(label(agent));
  });

  test("a conversation that closed the profile shows the agent switch off", async () => {
    // GIVEN an agent that reads the profile, in a conversation that closed it
    const controller = await loaded({ profile_access: false }, ["profile"]);

    // WHEN the tab renders
    const html = render(agentSession(), controller);

    // THEN the switch is off for this conversation
    expect(tagOf(html, "toggle-profile-access")).toContain('aria-checked="false"');
  });
});

function profile(id: string, name: string, isDefault: boolean): ProfileSummaryView {
  return {
    id,
    name,
    is_default: isDefault,
    detail: null,
    created_at: "2026-10-01T00:00:00Z",
    updated_at: "2026-10-01T00:00:00Z",
  };
}

describe("ChatIntegrationsTab - the conversation designates its profile", () => {
  test("the profile select sits beside the preferences toggle", () => {
    // GIVEN the configuration panel of a libre session
    developerMode.set(false);

    // WHEN the panel renders
    const html = render();
    const select = html.indexOf('data-testid="config-profile-select"');
    const toggle = html.indexOf('data-testid="toggle-user-memory"');

    // THEN a profile select is offered, in the same section as the toggle
    expect(select).toBeGreaterThan(-1);
    expect(toggle).toBeGreaterThan(-1);
    const between = html.slice(Math.min(select, toggle), Math.max(select, toggle));
    expect(between).not.toContain("<section");
    expect(html.indexOf('data-testid="session-profile"')).toBeLessThan(select);
    // AND its first choice is the default profile, stored as an empty value
    expect(tagOf(html, "config-profile-option-default")).toContain('value=""');
  });

  test("a designated profile is selected while it exists", () => {
    // GIVEN a default profile and a "Formateur" profile
    const profiles = [profile("p-def", "Moi", true), profile("p-form", "Formateur", false)];

    // WHEN a session designating "Formateur" and a session designating nothing are shown
    const designated = profileSelection("p-form", profiles);
    const bare = profileSelection(null, profiles);

    // THEN the first selects "Formateur", the second the default choice
    expect(designated).toBe("p-form");
    expect(bare).toBe("");
  });

  test("a designation of a deleted profile reads as the default", () => {
    // GIVEN a session designating a profile that no longer exists
    const profiles = [profile("p-def", "Moi", true)];

    // WHEN the select value is computed and saved
    const selection = profileSelection("p-gone", profiles);

    // THEN the default choice is shown and the session stores no profile
    expect(selection).toBe("");
    expect(profileIdOf(selection)).toBeNull();
    expect(profileIdOf("p-form")).toBe("p-form");
  });
});

describe("ChatIntegrationsTab - the profile switch is stored with the conversation", () => {
  const stats = (injected: boolean) => ({
    message_count: 2,
    summarized_count: 0,
    context_usage_pct: 12,
    user_memory_injected: injected,
    cross_sessions_referenced: 0,
  });

  test("a conversation is on until it stored the switch off", () => {
    // GIVEN a session that never stored the switch, one on, one off
    // WHEN the switch of each is read
    // THEN only the stored "off" reads as off
    expect(profileSwitchOf(libreSession())).toBe(true);
    expect(profileSwitchOf({ ...libreSession(), use_profile: true })).toBe(true);
    expect(profileSwitchOf({ ...libreSession(), use_profile: false })).toBe(false);
    expect(profileSwitchOf(null)).toBe(true);
  });

  test("before a conversation exists the switch shows the default of the settings", () => {
    // GIVEN no session yet, and the default of Settings > Integrations on, then off
    // WHEN the switch is read
    // THEN it starts where the default says
    expect(profileSwitchOf(null, true)).toBe(true);
    expect(profileSwitchOf(null, false)).toBe(false);
  });

  test("an existing conversation keeps its own switch, whatever the default", () => {
    // GIVEN conversations that stored their switch, and the opposite default
    // WHEN the switch of each is read
    // THEN each keeps what it stored
    expect(profileSwitchOf({ ...libreSession(), use_profile: true }, false)).toBe(true);
    expect(profileSwitchOf({ ...libreSession(), use_profile: false }, true)).toBe(false);
    expect(profileSwitchOf(libreSession(), false)).toBe(true);
  });

  test("the panel seeds the switch from the default while no conversation is open", () => {
    // GIVEN the source of the panel
    const panel = readFileSync(
      resolve(__dirname, "ChatIntegrationsTab.svelte"),
      "utf8",
    );

    // WHEN its wiring of the default is read
    // THEN it reads the setting, and applies it only without a session
    expect(panel).toContain("hydrateShareProfileDefault()");
    expect(panel).toContain(
      "if (!session) useUserMemory.set(profileSwitchOf(null, $shareProfileDefault));",
    );
  });

  test("turning the switch off writes it to the session, then reads the gauge again", async () => {
    // GIVEN a runtime that stores the switch and answers the gauge
    const update = vi.fn(async () => {});
    const read = vi.fn(async () => stats(false));

    // WHEN the switch of "s-1" is turned off
    const gauge = await writeProfileSwitch("s-1", false, { update, stats: read });

    // THEN the session update carries the switch alone, and the gauge read after it is returned
    expect(update).toHaveBeenCalledWith("s-1", { use_profile: false });
    expect(read).toHaveBeenCalledWith("s-1");
    expect(update.mock.invocationCallOrder[0]).toBeLessThan(read.mock.invocationCallOrder[0]);
    expect(gauge?.user_memory_injected).toBe(false);
  });

  test("a refused write throws and the gauge is not read", async () => {
    // GIVEN a runtime that refuses the session update
    const update = vi.fn(async () => {
      throw new Error("SESSION_CLOSED");
    });
    const read = vi.fn(async () => stats(true));

    // WHEN the switch is written
    const attempt = writeProfileSwitch("s-1", false, { update, stats: read });

    // THEN the refusal reaches the caller, which puts the switch back
    await expect(attempt).rejects.toThrow("SESSION_CLOSED");
    expect(read).not.toHaveBeenCalled();
  });

  test("the panel carries no context gauge: the composer holds the only one", () => {
    // GIVEN a conversation whose context is measured and carries the profile
    developerMode.set(false);
    chatConversationStats.set(stats(true));

    // WHEN the panel renders
    const html = render();
    chatConversationStats.set(null);

    // THEN it draws no bar, no legend and no profile line of its own
    expect(html).not.toContain("context-window-bar");
    expect(html).not.toContain("context-profile-indicator");
    expect(html).not.toContain("context-indicator-footer");
  });
});
