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

import ChatConfigPanelBody, {
  profileIdOf,
  profileSelection,
  profileSwitchOf,
  writeProfileSwitch,
} from "./ChatConfigPanelBody.svelte";
import { chatConversationStats } from "$lib/stores/chat";
import type { ProfileSummaryView } from "$lib/ipc/profile";
import { developerMode } from "$lib/stores/developerMode";
import { isSensitiveFamily } from "$lib/tools/action-family";
import type { ChatSessionDetail } from "$lib/types";

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

/**
 * Renders the panel through Svelte's server renderer. Vitest runs this suite
 * in its node environment, where `svelte` resolves to the server build and
 * `mount` is unavailable; the markup is what this suite asserts on, and the
 * server render walks the same template branches.
 */
function render(): string {
  return renderToString(ChatConfigPanelBody, {
    props: { session: libreSession(), onupdated: () => {} },
  }).body;
}

/** The opening tag carrying `data-testid="<testid>"`, or `null`. */
function tagOf(html: string, testid: string): string | null {
  const match = html.match(new RegExp(`<[^>]*data-testid="${testid}"[^>]*>`));
  return match ? match[0] : null;
}

/** The `data-sensitive` value of a tool's icon, or `null` when absent. */
function sensitivityOf(html: string, toolId: string): string | null {
  const tag = tagOf(html, `config-tool-icon-${toolId}`);
  const match = tag?.match(/data-sensitive="([^"]*)"/);
  return match ? match[1] : null;
}

beforeAll(async () => {
  await waitLocale();
});

describe("isSensitiveFamily", () => {
  test("only reading and the exempt family are not sensitive", () => {
    // GIVEN every family, plus a value no build knows
    // WHEN each is classified
    // THEN reading and exempt are the only non-sensitive values
    expect(isSensitiveFamily("read")).toBe(false);
    expect(isSensitiveFamily("exempt")).toBe(false);
    for (const family of [
      "read_personal",
      "write_or_delete",
      "send_external",
      "execute_code",
      "unclassified",
      "teleport",
    ]) {
      expect(isSensitiveFamily(family)).toBe(true);
    }
  });
});

describe("ChatConfigPanelBody - tool sensitivity follows the action family", () => {
  test("a reading tool is plain, every other family is marked", () => {
    // GIVEN the builder view of a libre session's configuration
    developerMode.set(true);

    // WHEN the panel renders its tool list
    const html = render();
    const marker = (id: string) => tagOf(html, `config-tool-sensitive-${id}`);

    // THEN tools of the read family carry no warning
    for (const id of ["file_read", "file_list", "web_search"]) {
      expect(sensitivityOf(html, id), id).toBe("false");
      expect(marker(id), id).toBeNull();
    }
    // AND a tool of every other family is marked sensitive
    for (const id of [
      "memory_search",
      "file_write",
      "http_fetch",
      "bash_executor",
    ]) {
      expect(sensitivityOf(html, id), id).toBe("true");
      expect(marker(id), id).not.toBeNull();
    }
  });

  test("the operator view keeps the colour but drops the warning glyph", () => {
    // GIVEN the operator view
    developerMode.set(false);

    // WHEN the panel renders
    const html = render();

    // THEN a sensitive tool is still flagged on its icon, without the glyph
    expect(sensitivityOf(html, "file_write")).toBe("true");
    expect(tagOf(html, "config-tool-sensitive-file_write")).toBeNull();
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

describe("ChatConfigPanelBody - the conversation designates its profile", () => {
  test("the profile select sits beside the preferences toggle", () => {
    // GIVEN the configuration panel of a libre session
    developerMode.set(false);

    // WHEN the panel renders
    const html = render();
    const select = html.indexOf('data-testid="config-profile-select"');
    const toggle = html.indexOf('data-testid="toggle-user-memory"');

    // THEN a profile select is offered, in the same card as the toggle
    expect(select).toBeGreaterThan(-1);
    expect(toggle).toBeGreaterThan(-1);
    const between = html.slice(Math.min(select, toggle), Math.max(select, toggle));
    expect(between).not.toContain('data-testid="context-window-bar"');
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

describe("ChatConfigPanelBody - the profile switch is stored with the conversation", () => {
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

  test("the gauge says whether the profile is in the prompt", () => {
    // GIVEN the gauge of a conversation whose prompt carries the profile, then not
    developerMode.set(false);
    chatConversationStats.set(stats(true));
    const carried = tagOf(render(), "context-profile-indicator");
    chatConversationStats.set(stats(false));
    const absent = tagOf(render(), "context-profile-indicator");
    chatConversationStats.set(null);

    // WHEN the panel renders each
    // THEN the indicator follows the gauge
    expect(carried).toContain('data-injected="true"');
    expect(absent).toContain('data-injected="false"');
  });
});
