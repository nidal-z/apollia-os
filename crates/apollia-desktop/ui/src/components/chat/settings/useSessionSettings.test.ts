import { describe, expect, test, vi } from "vitest";
import type { SessionPreset, SessionSettings } from "$lib/chat/sessionSettings";
import { createSessionSettings, type SessionSettingsEffects } from "./useSessionSettings.svelte";

const PRESET: SessionPreset = {
  id: 4,
  name: "Precise",
  model_source: "local",
  model_id: "qwen3-8b",
  load: {},
  request: { temperature: 0.2 },
  is_model_default: false,
};

function runtime(over: Partial<SessionSettingsEffects> = {}) {
  const stored: { settings: SessionSettings; notes: string } = {
    settings: { preset: 4, disabled_accesses: ["web_search"] },
    notes: "",
  };
  const effects: SessionSettingsEffects = {
    getSettings: vi.fn(async () => stored.settings),
    setSettings: vi.fn(async (_id: string, next: SessionSettings) => (stored.settings = next)),
    getNotes: vi.fn(async () => stored.notes),
    setNotes: vi.fn(async (_id: string, notes: string) => (stored.notes = notes)),
    listPresets: vi.fn(async () => [PRESET]),
    createPreset: vi.fn(async (draft) => ({ ...PRESET, ...draft, id: 9 })),
    capabilities: vi.fn(async () => ({
      model: "qwen3-8b",
      local: true,
      reasoning_profile: "toggle" as const,
    })),
    ...over,
  };
  return { effects, stored };
}

describe("the settings of a conversation are written as they are changed", () => {
  test("each setting writes the whole settings of the conversation", async () => {
    // GIVEN the Settings tab open on a conversation that switched an access off
    const { effects, stored } = runtime();
    const controller = createSessionSettings(effects);
    await controller.load("s-1");

    // WHEN the temperature, the length, the reasoning and the limit are set
    await controller.set("temperature", 0.9);
    await controller.set("max_tokens", 256);
    await controller.set("thinking", false);
    await controller.set("reasoning_effort", "high");
    await controller.set("context_overflow", "sliding_window");

    // THEN the conversation stores them all, its access choice untouched
    expect(effects.setSettings).toHaveBeenCalledTimes(5);
    expect(stored.settings).toEqual({
      preset: 4,
      disabled_accesses: ["web_search"],
      temperature: 0.9,
      max_tokens: 256,
      thinking: false,
      reasoning_effort: "high",
      context_overflow: "sliding_window",
    });
    expect(controller.reasoning).toBe("toggle");
  });

  test("a modified setting is marked until it is discarded", async () => {
    // GIVEN a conversation on the preset Precise
    const { effects, stored } = runtime();
    const controller = createSessionSettings(effects);
    await controller.load("s-1");
    expect(controller.preset?.name).toBe("Precise");
    expect(controller.modified).toEqual([]);

    // WHEN the temperature is changed, then the modifications discarded
    await controller.set("temperature", 1.2);
    const marked = controller.modified;
    await controller.discard();

    // THEN the temperature was marked, and the conversation is back on its preset
    expect(marked).toEqual(["temperature"]);
    expect(controller.modified).toEqual([]);
    expect(stored.settings).toEqual({ preset: 4, disabled_accesses: ["web_search"] });
  });

  test("a modified setting is marked until it is saved as a preset", async () => {
    // GIVEN a conversation that changed its temperature
    const { effects, stored } = runtime();
    const controller = createSessionSettings(effects);
    await controller.load("s-1");
    await controller.set("temperature", 1.2);

    // WHEN it is saved as a preset
    const outcome = await controller.saveAsPreset("Lively");

    // THEN the preset is created for its model with the value in force, the
    // conversation chooses it, and nothing is marked any more
    expect(outcome).toEqual({ ok: true });
    expect(effects.createPreset).toHaveBeenCalledWith({
      name: "Lively",
      model_source: "local",
      model_id: "qwen3-8b",
      load: {},
      request: { temperature: 1.2 },
    });
    expect(stored.settings).toEqual({ preset: 9, disabled_accesses: ["web_search"] });
    expect(controller.preset?.name).toBe("Lively");
    expect(controller.modified).toEqual([]);
  });

  test("a refused write puts the setting back and says why", async () => {
    // GIVEN a runtime that refuses the settings of a closed conversation
    const { effects } = runtime({
      setSettings: vi.fn(async () => {
        throw new Error("the conversation is closed");
      }),
    });
    const controller = createSessionSettings(effects);
    await controller.load("s-1");

    // WHEN a setting is changed
    await controller.set("temperature", 0.9);

    // THEN the tab shows the stored settings again, and the reason
    expect(controller.settings.temperature).toBeUndefined();
    expect(controller.error).toBe("the conversation is closed");
  });
});

describe("the preset block of a conversation", () => {
  test("choosing a preset stores it for the conversation", async () => {
    // GIVEN a conversation on a preset
    const { effects, stored } = runtime();
    const controller = createSessionSettings(effects);
    await controller.load("s-1");

    // WHEN no preset is chosen
    await controller.choose(null);

    // THEN the conversation follows the settings of its agent
    expect(stored.settings).toEqual({ disabled_accesses: ["web_search"] });
    expect(controller.preset).toBeNull();
  });

  test("an imported file is validated by the route that creates the preset", async () => {
    // GIVEN a runtime that refuses a preset carrying a system prompt
    const { effects } = runtime({
      createPreset: vi.fn(async () => {
        throw new Error("unknown field `system_prompt`");
      }),
    });
    const controller = createSessionSettings(effects);
    await controller.load("s-1");

    // WHEN such a file is imported, then a file that is not JSON
    const refused = await controller.importPreset(
      JSON.stringify({ name: "Bad", request: { system_prompt: "You are" } }),
    );
    const unreadable = await controller.importPreset("not json");

    // THEN the first comes back with the reason of the route, the second
    // without having been sent
    expect(refused).toEqual({
      ok: false,
      reason: "refused",
      detail: "unknown field `system_prompt`",
    });
    expect(unreadable).toEqual({ ok: false, reason: "not_json" });
    expect(effects.createPreset).toHaveBeenCalledTimes(1);
  });

  test("an imported preset is chosen for the conversation", async () => {
    // GIVEN a conversation and a preset file of another model
    const { effects, stored } = runtime();
    const controller = createSessionSettings(effects);
    await controller.load("s-1");

    // WHEN the file is imported
    const outcome = await controller.importPreset(
      JSON.stringify({ name: "Imported", model_id: "other", load: { n_ctx: 4096 } }),
    );

    // THEN the preset is created for the model of the conversation, and chosen
    expect(outcome).toEqual({ ok: true });
    expect(effects.createPreset).toHaveBeenCalledWith(
      expect.objectContaining({ model_id: "qwen3-8b", load: { n_ctx: 4096 } }),
    );
    expect(stored.settings.preset).toBe(9);
  });

  test("without a model, no preset is created", async () => {
    // GIVEN a conversation with no model configured
    const { effects } = runtime({
      capabilities: vi.fn(async () => ({ model: null, local: false, reasoning_profile: null })),
    });
    const controller = createSessionSettings(effects);
    await controller.load("s-1");

    // WHEN a preset is asked for
    const outcome = await controller.newPreset("Mine");

    // THEN it is refused before the runtime is asked
    expect(outcome).toEqual({ ok: false, reason: "no_model" });
    expect(effects.createPreset).not.toHaveBeenCalled();
  });
});

describe("the notes of a conversation are stored as they are typed", () => {
  test("typing stores the notes a moment after the last key, with no save button", async () => {
    // GIVEN the notes of a conversation, empty
    vi.useFakeTimers();
    try {
      const { effects, stored } = runtime();
      const controller = createSessionSettings(effects, 600);
      await controller.load("s-1");

      // WHEN two keys are typed in a row, then the moment passes
      controller.typeNotes("Call");
      controller.typeNotes("Call back on Monday");
      const before = controller.notesSaved;
      await vi.advanceTimersByTimeAsync(600);

      // THEN one write stored the text as it stood
      expect(before).toBe(false);
      expect(effects.setNotes).toHaveBeenCalledTimes(1);
      expect(stored.notes).toBe("Call back on Monday");
      expect(controller.notesSaved).toBe(true);
    } finally {
      vi.useRealTimers();
    }
  });

  test("leaving the field stores the notes at once", async () => {
    // GIVEN notes just typed
    const { effects, stored } = runtime();
    const controller = createSessionSettings(effects, 60_000);
    await controller.load("s-1");
    controller.typeNotes("Ask for the invoice");

    // WHEN the field is left before the moment passes
    await controller.flushNotes();

    // THEN the notes are stored, and leaving again writes nothing more
    expect(stored.notes).toBe("Ask for the invoice");
    await controller.flushNotes();
    expect(effects.setNotes).toHaveBeenCalledTimes(1);
  });
});
