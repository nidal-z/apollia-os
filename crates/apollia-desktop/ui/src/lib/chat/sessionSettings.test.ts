import { describe, expect, test, vi } from "vitest";
import {
  CONTEXT_OVERFLOWS,
  INSTRUCTIONS_MAX_CHARS,
  SESSION_PANEL_TABS,
  agentIdOf,
  applyToAgentSteps,
  choosePreset,
  chosenPreset,
  discardModifications,
  effectiveValue,
  instructionsTooLong,
  instructionsValue,
  maxTokensValue,
  missionWithInstructions,
  modifiedFields,
  parsePresetFile,
  presetDraftFrom,
  presetsOfModel,
  revertField,
  runApplySteps,
  sessionModelOf,
  temperatureValue,
  withSetting,
  type SessionModel,
  type SessionPreset,
  type SessionSettings,
} from "./sessionSettings";

const MODEL: SessionModel = { model_source: "local", model_id: "qwen3-8b" };

function preset(over: Partial<SessionPreset> = {}): SessionPreset {
  return {
    id: 4,
    name: "Precise",
    model_source: "local",
    model_id: "qwen3-8b",
    load: { n_ctx: 8192 },
    request: { temperature: 0.2, top_p: 0.9 },
    is_model_default: false,
    ...over,
  };
}

describe("the right panel of the chat", () => {
  test("it holds two tabs, Integrations then Settings", () => {
    // GIVEN the tabs of the panel
    // WHEN they are listed
    // THEN they are the two, in that order
    expect([...SESSION_PANEL_TABS]).toEqual(["integrations", "settings"]);
  });
});

describe("the presets a conversation can choose", () => {
  test("only the presets of its model are offered, by name", () => {
    // GIVEN presets of two models
    const all = [
      preset({ id: 1, name: "Wide" }),
      preset({ id: 2, name: "Other", model_id: "gemma-4b" }),
      preset({ id: 3, name: "Calm" }),
      preset({ id: 5, name: "Served", model_source: "server" }),
    ];

    // WHEN the presets of the model of the conversation are read
    const offered = presetsOfModel(all, MODEL);

    // THEN the two of that model come back, sorted, and none without a model
    expect(offered.map((p) => p.name)).toEqual(["Calm", "Wide"]);
    expect(presetsOfModel(all, null)).toEqual([]);
  });

  test("the model of a conversation is read from its capabilities", () => {
    // GIVEN the capabilities of a local model, of a served one, and of none
    // WHEN each is turned into the model a preset is attached by
    // THEN the source follows where it runs, and no model gives none
    expect(sessionModelOf({ model: "qwen3-8b", local: true })).toEqual(MODEL);
    expect(sessionModelOf({ model: "lab/qwen", local: false })).toEqual({
      model_source: "server",
      model_id: "lab/qwen",
    });
    expect(sessionModelOf({ model: null, local: false })).toBeNull();
    expect(sessionModelOf(null)).toBeNull();
  });

  test("choosing a preset drops what the conversation changed over the previous one", () => {
    // GIVEN a conversation on a preset, with a temperature of its own
    const settings: SessionSettings = {
      preset: 4,
      temperature: 1.1,
      max_tokens: 200,
      instructions: "Answer in one sentence.",
      disabled_accesses: ["web_search"],
    };

    // WHEN it chooses another preset, then none
    const other = choosePreset(settings, 9);
    const none = choosePreset(settings, null);

    // THEN the preset changes, its own values go, and the rest is untouched
    expect(other).toEqual({
      preset: 9,
      instructions: "Answer in one sentence.",
      disabled_accesses: ["web_search"],
    });
    expect("preset" in none).toBe(false);
    expect(chosenPreset(other, [preset()])).toBeNull();
    expect(chosenPreset(settings, [preset()])?.name).toBe("Precise");
  });
});

describe("a setting changed over the preset is marked until saved or discarded", () => {
  test("a value of the conversation that differs from the preset is a modification", () => {
    // GIVEN a preset at 0.2 and a conversation at 0.9 with a length of its own
    const settings: SessionSettings = { preset: 4, temperature: 0.9, max_tokens: 256 };

    // WHEN the modifications are read
    // THEN both settings are marked, and the value in force is the conversation's
    expect(modifiedFields(settings, preset())).toEqual(["temperature", "max_tokens"]);
    expect(effectiveValue(settings, preset(), "temperature")).toBe(0.9);
  });

  test("a value equal to the preset's, or left to it, is not marked", () => {
    // GIVEN a conversation whose temperature is the preset's
    // WHEN the modifications are read
    // THEN nothing is marked, and the preset's value is in force
    expect(modifiedFields({ preset: 4, temperature: 0.2 }, preset())).toEqual([]);
    expect(modifiedFields({ preset: 4 }, preset())).toEqual([]);
    expect(effectiveValue({ preset: 4 }, preset(), "temperature")).toBe(0.2);
    expect(effectiveValue({}, null, "max_tokens")).toBeUndefined();
  });

  test("without a preset, any value of the conversation is a modification", () => {
    // GIVEN a conversation with no preset and a length of its own
    // WHEN the modifications are read
    // THEN the length is marked
    expect(modifiedFields({ max_tokens: 128 }, null)).toEqual(["max_tokens"]);
  });

  test("discarding puts the conversation back on its preset and keeps the rest", () => {
    // GIVEN a modified conversation with instructions and a reasoning choice
    const settings: SessionSettings = {
      preset: 4,
      temperature: 0.9,
      max_tokens: 256,
      thinking: false,
      instructions: "Be brief.",
    };

    // WHEN the modifications are discarded, or one setting is reverted
    const discarded = discardModifications(settings);
    const reverted = revertField(settings, "temperature");

    // THEN the preset is in force again and nothing else moved
    expect(discarded).toEqual({ preset: 4, thinking: false, instructions: "Be brief." });
    expect(modifiedFields(discarded, preset())).toEqual([]);
    expect(reverted.max_tokens).toBe(256);
    expect("temperature" in reverted).toBe(false);
  });

  test("saving as a preset keeps the settings in force and never the instructions", () => {
    // GIVEN a conversation that changed the temperature over its preset
    const settings: SessionSettings = {
      preset: 4,
      temperature: 0.9,
      instructions: "Answer in one sentence.",
    };

    // WHEN it is saved as a preset
    const draft = presetDraftFrom(settings, preset(), "  Lively  ", MODEL);

    // THEN the preset is of the model of the conversation, carries the
    // conversation's value over the preset's other settings, and no instruction
    expect(draft).toEqual({
      name: "Lively",
      model_source: "local",
      model_id: "qwen3-8b",
      load: { n_ctx: 8192 },
      request: { temperature: 0.9, top_p: 0.9 },
    });
    expect(JSON.stringify(draft)).not.toContain("Answer in one sentence");
  });
});

describe("importing a preset from a file", () => {
  test("the preset is created for the model of the conversation and keeps its load settings", () => {
    // GIVEN a preset file written for another model
    const file = JSON.stringify({
      id: 12,
      name: "Imported",
      model_source: "server",
      model_id: "lab/other",
      load: { n_ctx: 4096 },
      request: { temperature: 0.4, max_tokens: 512 },
      is_model_default: true,
    });

    // WHEN it is read for the conversation
    const parsed = parsePresetFile(file, MODEL);

    // THEN the draft is of the model of the conversation, load kept
    expect(parsed).toEqual({
      ok: true,
      draft: {
        name: "Imported",
        model_source: "local",
        model_id: "qwen3-8b",
        load: { n_ctx: 4096 },
        request: { temperature: 0.4, max_tokens: 512 },
      },
    });
  });

  test("a file that is not JSON, or not a preset, is refused with its reason", () => {
    // GIVEN a text file, a JSON list, a preset without a name, and odd settings
    // WHEN each is read
    // THEN each is refused, and the reason tells the two cases apart
    expect(parsePresetFile("temperature = 0.4", MODEL)).toEqual({ ok: false, error: "not_json" });
    expect(parsePresetFile("[1, 2]", MODEL)).toEqual({ ok: false, error: "not_a_preset" });
    expect(parsePresetFile('{"request": {}}', MODEL)).toEqual({ ok: false, error: "not_a_preset" });
    expect(parsePresetFile('{"name": "x", "request": 3}', MODEL)).toEqual({
      ok: false,
      error: "not_a_preset",
    });
  });
});

describe("the instructions of a conversation", () => {
  test("they hold 4 000 characters at most", () => {
    // GIVEN instructions at the cap, and one character beyond
    // WHEN their length is checked
    // THEN only the longer one is refused
    expect(INSTRUCTIONS_MAX_CHARS).toBe(4000);
    expect(instructionsTooLong("a".repeat(4000))).toBe(false);
    expect(instructionsTooLong("a".repeat(4001))).toBe(true);
  });

  test("blank instructions are stored as none", () => {
    // GIVEN a field emptied, and one with a sentence between spaces
    // WHEN each is turned into what the conversation stores
    // THEN the blank one is left out, the other trimmed
    expect(instructionsValue("   \n")).toBeUndefined();
    expect(instructionsValue("  Be brief. ")).toBe("Be brief.");
  });
});

describe("the request settings of a conversation", () => {
  test("a setting written is set, and one cleared is inherited again", () => {
    // GIVEN a conversation with a temperature
    const settings: SessionSettings = { temperature: 0.4 };

    // WHEN the reasoning level is set, then the temperature cleared
    const withLevel = withSetting(settings, "reasoning_effort", "high");
    const cleared = withSetting(withLevel, "temperature", undefined);

    // THEN each write leaves the others, and the cleared field is left out
    expect(withLevel).toEqual({ temperature: 0.4, reasoning_effort: "high" });
    expect(cleared).toEqual({ reasoning_effort: "high" });
    expect(settings).toEqual({ temperature: 0.4 });
  });

  test("the temperature is held between 0 and 2", () => {
    // GIVEN values typed in the field or dragged on the slider
    // WHEN each is read
    // THEN it is held in bounds, and an empty field is no value
    expect(temperatureValue("0.75")).toBe(0.75);
    expect(temperatureValue("3")).toBe(2);
    expect(temperatureValue("-1")).toBe(0);
    expect(temperatureValue("")).toBeUndefined();
    expect(temperatureValue("warm")).toBeUndefined();
  });

  test("the answer length is at least one token", () => {
    // GIVEN lengths typed in the field
    // WHEN each is read
    // THEN a whole positive number is kept, anything else is no value
    expect(maxTokensValue("512")).toBe(512);
    expect(maxTokensValue("12.9")).toBe(12);
    expect(maxTokensValue("0")).toBeUndefined();
    expect(maxTokensValue("")).toBeUndefined();
  });

  test("the context limit offers three values, summarizing first", () => {
    // GIVEN the values of the list
    // WHEN they are read
    // THEN they are the three the runtime knows, the default first
    expect([...CONTEXT_OVERFLOWS]).toEqual(["summarize_middle", "sliding_window", "stop_at_limit"]);
  });
});

describe("applying the conversation to its agent", () => {
  const base = {
    model: MODEL,
    presetName: "Preset of Accountant",
    mission: "Sort the invoices.",
  };

  test("an unmodified conversation gives the agent its model and its preset, nothing else", () => {
    // GIVEN a conversation on a preset, unmodified, with instructions
    const settings: SessionSettings = { preset: 4, instructions: "Be brief." };

    // WHEN it is applied with the box left unchecked
    const steps = applyToAgentSteps({ ...base, settings, preset: preset(), addInstructions: false });

    // THEN the one write assigns the model and that preset; the mission is untouched
    expect(steps).toEqual([{ kind: "set_model", model: MODEL, preset: 4 }]);
  });

  test("changed settings become a new preset, assigned after it is created", () => {
    // GIVEN a conversation that changed the temperature over its preset
    const settings: SessionSettings = { preset: 4, temperature: 0.9 };

    // WHEN it is applied
    const steps = applyToAgentSteps({ ...base, settings, preset: preset(), addInstructions: false });

    // THEN a preset is created with the settings in force, then assigned
    expect(steps.map((s) => s.kind)).toEqual(["create_preset", "set_model"]);
    expect(steps[0]).toMatchObject({
      draft: { name: "Preset of Accountant", request: { temperature: 0.9, top_p: 0.9 } },
    });
    expect(steps[1]).toMatchObject({ preset: "created" });
  });

  test("the instructions reach the mission only when the box is checked", () => {
    // GIVEN a conversation with instructions and no preset
    const settings: SessionSettings = { instructions: "Answer in one sentence." };

    // WHEN it is applied with the box checked
    const steps = applyToAgentSteps({ ...base, settings, preset: null, addInstructions: true });

    // THEN the mission obtained is the mission followed by the instructions
    expect(steps).toEqual([
      { kind: "set_model", model: MODEL, preset: null },
      { kind: "set_mission", mission: "Sort the invoices.\n\nAnswer in one sentence." },
    ]);
    expect(missionWithInstructions("Sort the invoices.\n", "  ")).toBe("Sort the invoices.\n");
    expect(missionWithInstructions("", "Be brief.")).toBe("Be brief.");
  });

  test("the writes leave in order, the created preset assigned by its identifier", async () => {
    // GIVEN the writes of a modified conversation with its instructions added
    const steps = applyToAgentSteps({
      ...base,
      settings: { temperature: 0.9, instructions: "Be brief." },
      preset: null,
      addInstructions: true,
    });
    const sent: string[] = [];
    const effects = {
      createPreset: vi.fn(async () => (sent.push("create"), { id: 21 })),
      setModel: vi.fn(async () => void sent.push("model")),
      setMission: vi.fn(async () => void sent.push("mission")),
    };

    // WHEN they are sent for the agent
    await runApplySteps("accountant", steps, effects);

    // THEN the preset is created first and assigned by the identifier it got
    expect(sent).toEqual(["create", "model", "mission"]);
    expect(effects.setModel).toHaveBeenCalledWith("accountant", MODEL, 21);
    expect(effects.setMission).toHaveBeenCalledWith("accountant", "Sort the invoices.\n\nBe brief.");
  });

  test("a preset the runtime refuses is never assigned", async () => {
    // GIVEN a runtime that refuses the preset
    const steps = applyToAgentSteps({
      ...base,
      settings: { temperature: 0.9 },
      preset: null,
      addInstructions: false,
    });
    const effects = {
      createPreset: vi.fn(async () => {
        throw new Error("a preset of this name exists");
      }),
      setModel: vi.fn(async () => {}),
      setMission: vi.fn(async () => {}),
    };

    // WHEN the writes are sent
    const sent = runApplySteps("accountant", steps, effects);

    // THEN the refusal comes back and the agent is left as it was
    await expect(sent).rejects.toThrow("a preset of this name exists");
    expect(effects.setModel).not.toHaveBeenCalled();
  });

  test("the agent of a free conversation is the built-in one", () => {
    // GIVEN a free conversation and a conversation with an installed agent
    // WHEN the agent of each is named
    // THEN the free one names the built-in agent
    expect(agentIdOf({ mode: "libre", agent_name: null }, "apollia:chat")).toBe("apollia:chat");
    expect(agentIdOf({ mode: "agent", agent_name: "accountant" }, "apollia:chat")).toBe("accountant");
  });
});
