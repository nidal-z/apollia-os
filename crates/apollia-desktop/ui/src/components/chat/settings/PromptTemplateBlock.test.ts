import { beforeAll, describe, expect, test, vi } from "vitest";
import { render } from "svelte/server";
import { get } from "svelte/store";
import { locale, t, waitLocale } from "svelte-i18n";
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

import { invoke } from "@tauri-apps/api/core";
import type { SessionPreset, SessionSettings } from "$lib/chat/sessionSettings";
import { getChatPromptTemplate } from "$lib/ipc/chatSettings";
import { chatPromptTemplate } from "$lib/stores/chatPromptTemplate";
import { developerMode } from "$lib/stores/developerMode";
import type { ChatSessionDetail, LoadedModelsResponse } from "$lib/types";
import PromptTemplateBlock, { promptTemplateShown } from "./PromptTemplateBlock.svelte";
import SessionSettingsTab from "./SessionSettingsTab.svelte";
import {
  createPromptTemplate,
  promptTemplateAnswer,
  promptTemplateKey,
  type PromptTemplateSubject,
} from "./usePromptTemplate.svelte";
import { createSessionSettings, type SessionSettingsEffects } from "./useSessionSettings.svelte";

const QWEN_TEMPLATE = "{%- if tools %}<|im_start|>system{%- endif %}";

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

const PRESETS: SessionPreset[] = [
  {
    id: 4,
    name: "Precise",
    model_source: "local",
    model_id: "qwen3-8b",
    request: { temperature: 0.2 },
    is_model_default: false,
  },
  {
    id: 5,
    name: "Own template",
    model_source: "local",
    model_id: "qwen3-8b",
    load: { chat_template: "{{ from_preset }}" },
    is_model_default: false,
  },
];

/** A runtime holding the settings of one conversation. */
function runtime(initial: SessionSettings = {}) {
  const stored = { settings: initial };
  const effects: SessionSettingsEffects = {
    getSettings: async () => stored.settings,
    setSettings: async (_id, next) => (stored.settings = next),
    getNotes: async () => "",
    setNotes: async (_id, notes) => notes,
    listPresets: async () => PRESETS,
    createPreset: async (draft) => ({ ...draft, id: 9, is_model_default: false }),
    capabilities: async () => ({ model: "qwen3-8b", local: true, reasoning_profile: null }),
  };
  return { effects, stored };
}

async function controllerOver(initial: SessionSettings = {}) {
  const controller = createSessionSettings(runtime(initial).effects);
  await controller.load("s-1");
  return controller;
}

/** The opening tag carrying `data-testid="<testid>"`, or `null`. */
function tagOf(html: string, testid: string): string | null {
  const match = html.match(new RegExp(`<[^>]*data-testid="${testid}"[^>]*>`));
  return match ? match[0] : null;
}

function resident(...paths: string[]): LoadedModelsResponse {
  return {
    engineAvailable: true,
    maxLoaded: null,
    models: paths.map((modelPath) => ({ modelPath })),
  } as unknown as LoadedModelsResponse;
}

function subject(over: Partial<PromptTemplateSubject> = {}): PromptTemplateSubject {
  return { sessionId: "s-1", model: null, preset: undefined, loaded: null, messages: 0, ...over };
}

beforeAll(async () => {
  locale.set("fr");
  await waitLocale("fr");
});

describe("the Settings tab shows the prompt template when asked, in developer mode", () => {
  const cases = [
    { setting: true, developer: true, shown: true },
    { setting: true, developer: false, shown: false },
    { setting: false, developer: true, shown: false },
    { setting: false, developer: false, shown: false },
  ];

  test.each(cases)(
    "setting $setting, developer mode $developer: block shown $shown",
    async ({ setting, developer, shown }) => {
      // GIVEN the preference of Settings > Chat and the developer mode
      chatPromptTemplate.set(setting);
      developerMode.set(developer);
      const controller = await controllerOver();

      // WHEN the Settings tab of a conversation renders
      const html = render(SessionSettingsTab, { props: { session: session(), controller } }).body;
      chatPromptTemplate.set(false);
      developerMode.set(false);

      // THEN the block is there only when both are on, under the preset
      expect(promptTemplateShown(setting, developer)).toBe(shown);
      const block = html.indexOf('data-testid="session-prompt-template"');
      expect(block > -1).toBe(shown);
      if (shown) {
        expect(block).toBeGreaterThan(html.indexOf('id="session-preset-select"'));
        expect(block).toBeLessThan(html.indexOf('data-testid="session-instructions"'));
      } else {
        expect(html).not.toContain("session-prompt-template");
      }
    },
  );
});

describe("the block shows the text of the source the runtime names", () => {
  test("the template of the preset of the conversation, read only, with Copy", async () => {
    // GIVEN a runtime answering the template the preset carries
    const reader = createPromptTemplate(async () => ({
      source: "preset",
      template: "{{ from_preset }}",
    }));
    await reader.refresh("s-1");
    const controller = await controllerOver({ preset: 5 });

    // WHEN the block renders
    const html = render(PromptTemplateBlock, {
      props: { session: session(), controller, reader },
    }).body;

    // THEN the text is shown in a fixed-width, read-only block that folds,
    // named as the one of the preset, with its Copy button
    const text = tagOf(html, "session-prompt-template-text");
    expect(text).toContain("<pre");
    expect(text).toContain("font-mono");
    expect(html).toContain("{{ from_preset }}");
    expect(html).not.toContain("<textarea");
    expect(tagOf(html, "session-prompt-template-source")).toContain('data-source="preset"');
    expect(html).toContain(get(t)("dev.chat.prompt_template.source_preset"));
    expect(html).toContain('aria-expanded="true"');
    expect(tagOf(html, "session-prompt-template-copy")).not.toBeNull();
    expect(html).toContain(get(t)("dev.chat.prompt_template.copy"));
    expect(tagOf(html, "session-prompt-template-unavailable")).toBeNull();
  });

  test("the template the engine applies to the loaded model", async () => {
    // GIVEN a runtime answering what the engine reports for the instance
    const reader = createPromptTemplate(async () => ({
      source: "engine",
      template: QWEN_TEMPLATE,
    }));
    await reader.refresh("s-1");
    const controller = await controllerOver();

    // WHEN the block renders
    const html = render(PromptTemplateBlock, {
      props: { session: session(), controller, reader },
    }).body;

    // THEN the text of the engine is shown, named as such
    expect(html).toContain("&lt;|im_start|>system");
    expect(tagOf(html, "session-prompt-template-source")).toContain('data-source="engine"');
    expect(html).toContain(get(t)("dev.chat.prompt_template.source_engine"));
    expect(tagOf(html, "session-prompt-template-copy")).not.toBeNull();
  });

  const unreadable: { name: string; read: () => Promise<unknown> }[] = [
    {
      name: "a model that is not loaded",
      read: async () => ({ source: "unavailable", template: null }),
    },
    { name: "an engine that leaves the field out", read: async () => ({ source: "engine" }) },
    {
      name: "a model served by the MLX engine",
      read: async () => ({ source: "unavailable", template: null }),
    },
    {
      name: "a read the runtime refuses",
      read: async () => {
        throw new Error("chat subsystem not available");
      },
    },
  ];

  test.each(unreadable)("$name: the block says so, with no text", async ({ read }) => {
    // GIVEN a runtime with no template to answer
    const reader = createPromptTemplate(read);
    await reader.refresh("s-1");
    const controller = await controllerOver();

    // WHEN the block renders
    const html = render(PromptTemplateBlock, {
      props: { session: session(), controller, reader },
    }).body;

    // THEN it says the template is built into the model, and shows neither
    // a text nor a Copy button
    expect(reader.answer).toEqual({ source: "unavailable", template: null });
    expect(html).toContain(get(t)("dev.chat.prompt_template.unavailable"));
    expect(tagOf(html, "session-prompt-template-text")).toBeNull();
    expect(tagOf(html, "session-prompt-template-copy")).toBeNull();
  });

  test("the read goes through the routed command, with the conversation", async () => {
    // GIVEN the shell answering the command
    vi.mocked(invoke).mockResolvedValueOnce({ source: "engine", template: QWEN_TEMPLATE });

    // WHEN the template of a conversation is read
    const reader = createPromptTemplate(getChatPromptTemplate);
    await reader.refresh("s-7");

    // THEN one command left, naming the conversation, and its answer is shown
    expect(invoke).toHaveBeenLastCalledWith("get_chat_prompt_template", { sessionId: "s-7" });
    expect(reader.answer).toEqual({ source: "engine", template: QWEN_TEMPLATE });
    expect(reader.sessionId).toBe("s-7");
  });

  test("an answer of an unknown shape reads as nothing to show", () => {
    // GIVEN answers the runtime should never send
    // WHEN each is read
    // THEN none is shown as a template
    for (const raw of [null, "text", { source: "file", template: "x" }, { source: "preset", template: " " }]) {
      expect(promptTemplateAnswer(raw)).toEqual({ source: "unavailable", template: null });
    }
  });
});

describe("the block follows the preset and the model of the conversation", () => {
  test("choosing another preset is a new read, once the runtime holds the choice", async () => {
    // GIVEN a conversation without a preset, whose model is loaded, and a
    // runtime that answers from the settings it holds
    const { effects, stored } = runtime();
    const controller = createSessionSettings(effects);
    await controller.load("s-1");
    const reader = createPromptTemplate(async () =>
      stored.settings.preset === 5
        ? { source: "preset", template: "{{ from_preset }}" }
        : { source: "engine", template: QWEN_TEMPLATE },
    );
    const key = () =>
      promptTemplateKey(subject({ preset: controller.storedPreset, loaded: resident("/m/q.gguf") }));
    const before = key();
    await reader.refresh("s-1");
    expect(reader.answer.source).toBe("engine");

    // WHEN the preset that carries a template is chosen in the panel
    const written = controller.choose(5);
    const during = key();
    await written;
    const after = key();
    if (after !== before) await reader.refresh("s-1");

    // THEN the key holds until the write is stored, then changes, and the
    // block shows the text of the preset without the screen being reloaded
    expect(during).toBe(before);
    expect(after).not.toBe(before);
    expect(reader.answer).toEqual({ source: "preset", template: "{{ from_preset }}" });

    // WHEN the preset is taken off again
    await controller.choose(null);
    expect(key()).toBe(before);
    await reader.refresh("s-1");

    // THEN the text of the engine is back
    expect(reader.answer).toEqual({ source: "engine", template: QWEN_TEMPLATE });
  });

  test("another model, a model loaded or ejected, and a first answer are each a new read", () => {
    // GIVEN the subject of a conversation on the default model, not loaded
    const base = promptTemplateKey(subject());

    // WHEN its model changes, its model is loaded, or its first answer comes
    const otherModel = promptTemplateKey(subject({ model: "gemma-3" }));
    const loaded = promptTemplateKey(subject({ loaded: resident("/m/q.gguf") }));
    const answered = promptTemplateKey(subject({ messages: 2 }));
    const otherSession = promptTemplateKey(subject({ sessionId: "s-2" }));

    // THEN each has a key of its own, the same subject keeps its key, and
    // no conversation has none
    expect(new Set([base, otherModel, loaded, answered, otherSession]).size).toBe(5);
    expect(promptTemplateKey(subject())).toBe(base);
    expect(promptTemplateKey(subject({ sessionId: null }))).toBeNull();
  });

  test("the answer of an earlier read never replaces the answer of a later one", async () => {
    // GIVEN a first read that answers after the second
    let release: (value: unknown) => void = () => {};
    const slow = new Promise<unknown>((resolve) => (release = resolve));
    const answers = [slow, Promise.resolve({ source: "engine", template: "second" })];
    const reader = createPromptTemplate(() => answers.shift() ?? Promise.resolve(null));

    // WHEN the model changes while the first read is on its way
    const first = reader.refresh("s-1");
    const second = reader.refresh("s-1");
    await second;
    release({ source: "engine", template: "first" });
    await first;

    // THEN the block shows the template of the model in force
    expect(reader.answer).toEqual({ source: "engine", template: "second" });
    expect(reader.loading).toBe(false);
  });

  test("opening another conversation never shows the template of the one before", async () => {
    // GIVEN the template of one conversation shown
    let release: (value: unknown) => void = () => {};
    const pending = new Promise<unknown>((resolve) => (release = resolve));
    const answers = [Promise.resolve({ source: "engine", template: "of s-1" }), pending];
    const reader = createPromptTemplate(() => answers.shift() ?? Promise.resolve(null));
    await reader.refresh("s-1");

    // WHEN another conversation opens and its read has not come back
    const next = reader.refresh("s-2");
    const meanwhile = reader.answer;
    release({ source: "unavailable", template: null });
    await next;

    // THEN nothing of the first is shown under the second
    expect(meanwhile).toEqual({ source: "unavailable", template: null });
    expect(reader.sessionId).toBe("s-2");
  });
});
