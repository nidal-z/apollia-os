import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { beforeAll, describe, expect, test, vi } from "vitest";
import { render as renderToString } from "svelte/server";
import { get } from "svelte/store";
import { t, waitLocale } from "svelte-i18n";
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

import type { SessionSettings } from "$lib/chat/sessionSettings";
import type { ChatSessionDetail } from "$lib/types";
import ChatSessionPanel from "../ChatSessionPanel.svelte";
import { presetErrorKey } from "./PresetBlock.svelte";
import SessionSettingsTab, { keptReasoning, notesExcerpt } from "./SessionSettingsTab.svelte";
import { createSessionSettings, type SessionSettingsEffects } from "./useSessionSettings.svelte";

function session(over: Partial<ChatSessionDetail> = {}): ChatSessionDetail {
  return {
    id: "s-1",
    mode: "agent",
    agent_name: "accountant",
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
    ...over,
  };
}

async function loaded(
  settings: SessionSettings,
  reasoning: "none" | "toggle" | "levels" | null,
  model: string | null = "qwen3-8b",
) {
  const effects: SessionSettingsEffects = {
    getSettings: async () => settings,
    setSettings: async (_id, next) => next,
    getNotes: async () => "",
    setNotes: async (_id, notes) => notes,
    listPresets: async () => [
      {
        id: 4,
        name: "Precise",
        model_source: "local",
        model_id: "qwen3-8b",
        request: { temperature: 0.2 },
        is_model_default: false,
      },
    ],
    createPreset: async (draft) => ({ ...draft, id: 9, is_model_default: false }),
    capabilities: async () => ({ model, local: true, reasoning_profile: reasoning }),
  };
  const controller = createSessionSettings(effects);
  await controller.load("s-1");
  return controller;
}

/** The opening tag carrying `data-testid="<testid>"`, or `null`. */
function tagOf(html: string, testid: string): string | null {
  const match = html.match(new RegExp(`<[^>]*data-testid="${testid}"[^>]*>`));
  return match ? match[0] : null;
}

function source(relative: string): string {
  return readFileSync(resolve(__dirname, relative), "utf8");
}

beforeAll(async () => {
  await waitLocale();
});

describe("the right panel of the chat holds two tabs", () => {
  test("Integrations and Settings are its tabs, in place of the former single panel", () => {
    // GIVEN the panel of a conversation
    // WHEN it renders
    const html = renderToString(ChatSessionPanel, {
      props: { session: session(), onupdated: () => {} },
    }).body;

    // THEN it carries the two tabs, and opens on the settings
    expect(tagOf(html, "chat-session-panel-tab-integrations")).not.toBeNull();
    expect(tagOf(html, "chat-session-panel-tab-settings")).toContain('aria-selected="true"');
    expect(html).toContain(get(t)("chat.session_panel.integrations"));
    expect(html).toContain(get(t)("chat.session_panel.settings"));
    expect(tagOf(html, "chat-session-settings")).not.toBeNull();
  });

  test("the chat screen and the sheet mount the two tabs, and the former body is gone", () => {
    // GIVEN the sources of the two hosts
    const screen = source("../../../routes/Chat.svelte");
    const sheet = source("../ChatConfigPanel.svelte");

    // WHEN their imports are read
    // THEN both mount the tabs, and neither names the former body
    expect(screen).toContain('data-testid="chat-inspector-integrations-tab"');
    expect(screen).toContain('data-testid="chat-inspector-settings-tab"');
    expect(sheet).toContain("ChatSessionPanel");
    expect(screen + sheet).not.toContain("ChatConfigPanelBody");
  });

  test("the list of engines and the step budget are no longer in the panel", () => {
    // GIVEN the sources of the two tabs
    const tabs = source("../ChatIntegrationsTab.svelte") + source("./SessionSettingsTab.svelte");

    // WHEN the two retired controls are looked for
    // THEN neither is rendered nor stored
    expect(tabs).not.toContain("config-backend-");
    expect(tabs).not.toContain("config-step-budget");
    expect(tabs).not.toContain("step_budget");
    expect(tabs).not.toContain("llmBackends");
  });
});

describe("the Settings tab of a conversation", () => {
  test("the preset block offers the presets of the model, a new one and a file", async () => {
    // GIVEN a conversation on the preset Precise
    const controller = await loaded({ preset: 4 }, "toggle");

    // WHEN the tab renders
    const html = renderToString(SessionSettingsTab, { props: { session: session(), controller } }).body;

    // THEN the preset is chosen in the list, the menu and the file input are
    // there, and nothing is marked
    expect(tagOf(html, "session-preset-select")).not.toBeNull();
    expect(html).toMatch(/<option[^>]*value="4"[^>]*selected|<option[^>]*selected[^>]*value="4"/);
    expect(tagOf(html, "session-preset-menu")).not.toBeNull();
    expect(tagOf(html, "session-preset-file")).toContain('accept=".json,application/json"');
    expect(tagOf(html, "session-preset-modified")).toBeNull();
    expect(tagOf(html, "session-preset-discard")).toBeNull();
  });

  test("a setting changed over the preset is marked, with the way back", async () => {
    // GIVEN a conversation that set its own temperature over its preset
    const controller = await loaded({ preset: 4, temperature: 1.1 }, "toggle");

    // WHEN the tab renders
    const html = renderToString(SessionSettingsTab, { props: { session: session(), controller } }).body;

    // THEN the block says so, names the setting, and offers to discard
    expect(tagOf(html, "session-preset-modified")).not.toBeNull();
    expect(html).toContain(get(t)("chat.session_settings.preset.field_temperature"));
    expect(tagOf(html, "session-preset-discard")).not.toBeNull();
    expect(html).toContain(get(t)("chat.session_settings.preset.discard"));
  });

  test("the instructions show their count and say they are added to the mission", async () => {
    // GIVEN a conversation with no instructions
    const controller = await loaded({}, null);

    // WHEN the tab renders
    const html = renderToString(SessionSettingsTab, { props: { session: session(), controller } }).body;

    // THEN the field, its count and its help are shown
    expect(tagOf(html, "session-instructions-field")).not.toBeNull();
    expect(tagOf(html, "session-instructions-count")).not.toBeNull();
    expect(html).toContain(get(t)("chat.session_settings.instructions.help"));
    expect(get(t)("chat.session_settings.instructions.help", { locale: "en" })).toBe(
      "They are added to the agent's mission, without replacing it, for this conversation only.",
    );
    expect(get(t)("chat.session_settings.instructions.help", { locale: "fr" })).toContain("mission");
  });

  test("the reasoning control follows the profile of the model", async () => {
    // GIVEN a model that switches reasoning, one with levels, one with none
    const toggle = await loaded({ thinking: false }, "toggle");
    const levels = await loaded({ reasoning_effort: "high" }, "levels");
    const none = await loaded({}, "none");
    const of = (controller: typeof toggle) =>
      renderToString(SessionSettingsTab, { props: { session: session(), controller } }).body;

    // WHEN the tab renders for each
    // THEN the first shows the switch at the stored value, the second the
    // level, the third no reasoning block
    expect(tagOf(of(toggle), "session-thinking")).toContain('aria-checked="false"');
    expect(tagOf(of(toggle), "session-reasoning-effort")).toBeNull();
    expect(tagOf(of(levels), "session-reasoning-effort")).not.toBeNull();
    expect(tagOf(of(levels), "session-thinking")).toBeNull();
    expect(tagOf(of(none), "session-reasoning")).toBeNull();
  });

  test("keeping the earlier reasoning is offered with the reasoning block, and only with it", async () => {
    // GIVEN a model that switches reasoning and keeps the earlier one, a
    // model with levels that does not, and a model with no reasoning
    const toggle = await loaded({ preserve_thinking: true }, "toggle");
    const levels = await loaded({}, "levels");
    const none = await loaded({ preserve_thinking: true }, "none");
    const of = (controller: typeof toggle) =>
      renderToString(SessionSettingsTab, { props: { session: session(), controller } }).body;

    // WHEN the tab renders for each
    // THEN the first two show the switch at the stored value, with the
    // sentence that says what it costs, and the third shows neither
    expect(tagOf(of(toggle), "session-preserve-thinking")).toContain('aria-checked="true"');
    expect(tagOf(of(levels), "session-preserve-thinking")).toContain('aria-checked="false"');
    expect(of(toggle)).toContain(get(t)("chat.session_settings.reasoning.preserve_thinking"));
    expect(tagOf(of(levels), "session-preserve-thinking-help")).not.toBeNull();
    expect(tagOf(of(none), "session-preserve-thinking")).toBeNull();
    expect(tagOf(of(none), "session-preserve-thinking-help")).toBeNull();
    expect(
      get(t)("chat.session_settings.reasoning.preserve_thinking", { locale: "fr" }),
    ).toMatch(/^Garder le raisonnement/);
    expect(get(t)("chat.session_settings.reasoning.preserve_thinking", { locale: "en" })).toBe(
      "Keep the reasoning of earlier answers",
    );
    for (const locale of ["fr", "en"]) {
      expect(
        get(t)("chat.session_settings.reasoning.preserve_thinking_help", { locale }),
      ).toMatch(/context/);
    }
  });

  test("the switch that keeps the earlier reasoning writes the settings of the conversation", async () => {
    // GIVEN the tab of a conversation that set a temperature, and its source
    const written: SessionSettings[] = [];
    const controller = createSessionSettings({
      getSettings: async () => ({ temperature: 0.4 }),
      setSettings: async (_id, next) => {
        written.push(next);
        return next;
      },
      getNotes: async () => "",
      setNotes: async (_id, notes) => notes,
      listPresets: async () => [],
      createPreset: async (draft) => ({ ...draft, id: 9, is_model_default: false }),
      capabilities: async () => ({ model: "qwen3-8b", local: true, reasoning_profile: "toggle" }),
    });
    await controller.load("s-1");
    const tab = source("./SessionSettingsTab.svelte");

    // WHEN the switch is turned on, then off, as the tab writes it
    await controller.set("preserve_thinking", keptReasoning(true));
    await controller.set("preserve_thinking", keptReasoning(false));

    // THEN the tab hands the switch to the settings of the conversation: on
    // is stored beside what was set, off leaves the field out again
    expect(tab).toContain('controller.set("preserve_thinking", keptReasoning(on))');
    expect(written).toEqual([{ temperature: 0.4, preserve_thinking: true }, { temperature: 0.4 }]);
    expect(controller.settings).toEqual({ temperature: 0.4 });
  });

  test("the answer block is closed and sums up its three settings", async () => {
    // GIVEN a conversation limited to 256 tokens that stops at the limit
    const controller = await loaded(
      { preset: 4, max_tokens: 256, context_overflow: "stop_at_limit" },
      null,
    );

    // WHEN the tab renders
    const html = renderToString(SessionSettingsTab, { props: { session: session(), controller } }).body;

    // THEN the summary names the temperature of the preset, the length and the limit
    const summary = get(t)("chat.session_settings.response.summary", {
      values: {
        temperature: 0.2,
        length: 256,
        overflow: get(t)("chat.session_settings.response.overflow_stop_at_limit"),
      },
    });
    expect(html).toContain(summary);
    expect(tagOf(html, "session-temperature")).toBeNull();
  });

  test("the foot keeps saving as a preset above applying to the agent", async () => {
    // GIVEN an open conversation, then a closed one
    const controller = await loaded({}, null);
    const open = renderToString(SessionSettingsTab, { props: { session: session(), controller } }).body;
    const closed = renderToString(SessionSettingsTab, {
      props: { session: session({ status: "closed" }), controller },
    }).body;

    // WHEN the foot is read
    // THEN both actions are there in that order, and gone once closed
    const save = open.indexOf('data-testid="session-save-as-preset"');
    const apply = open.indexOf('data-testid="session-apply-to-agent"');
    expect(save).toBeGreaterThan(-1);
    expect(apply).toBeGreaterThan(save);
    expect(tagOf(closed, "session-apply-to-agent")).toBeNull();
  });

  test("without a model, no preset can be chosen or saved, and the tab says why", async () => {
    // GIVEN a conversation with no model configured
    const controller = await loaded({}, null, null);

    // WHEN the tab renders
    const html = renderToString(SessionSettingsTab, { props: { session: session(), controller } }).body;

    // THEN the block says why, and neither the list nor the save is available
    expect(tagOf(html, "session-preset-no-model")).not.toBeNull();
    expect(tagOf(html, "session-preset-select")).toContain("disabled");
    expect(tagOf(html, "session-save-as-preset")).toContain("disabled");
  });
});

describe("what the tab says of notes and of a preset that was not created", () => {
  test("the closed notes show their start, or nothing when empty", () => {
    // GIVEN empty notes, short notes, and long notes on several lines
    // WHEN their excerpt is read
    // THEN empty gives none, short is whole, long is cut on one line
    expect(notesExcerpt("  \n ")).toBeNull();
    expect(notesExcerpt("Call back")).toBe("Call back");
    const long = notesExcerpt(`First line\n${"x".repeat(80)}`);
    expect(long?.startsWith("First line x")).toBe(true);
    expect(long?.endsWith("…")).toBe(true);
  });

  test("each reason of a preset not created has its sentence", () => {
    // GIVEN the reasons an import can fail for
    // WHEN each is turned into a message key
    // THEN the local ones have a sentence, and the refusal of the route shows its own text
    expect(presetErrorKey({ ok: false, reason: "not_json" })).toBe(
      "chat.session_settings.preset.error_not_json",
    );
    expect(presetErrorKey({ ok: false, reason: "not_a_preset" })).toBe(
      "chat.session_settings.preset.error_not_a_preset",
    );
    expect(presetErrorKey({ ok: false, reason: "no_model" })).toBe(
      "chat.session_settings.preset.error_no_model",
    );
    expect(presetErrorKey({ ok: false, reason: "refused", detail: "x" })).toBeNull();
    expect(presetErrorKey({ ok: true })).toBeNull();
    expect(
      get(t)("chat.session_settings.preset.error_refused", { values: { reason: "unknown field" } }),
    ).toContain("unknown field");
  });
});
