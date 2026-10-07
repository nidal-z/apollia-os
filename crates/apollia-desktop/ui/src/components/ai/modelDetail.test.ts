import { describe, test, expect } from "vitest";
import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import fr from "$lib/i18n/fr.json";
import en from "$lib/i18n/en.json";
import type { InstalledModel } from "$lib/ipc/modelHub";
import type { ModelDetail, PresetDetail } from "$lib/ipc/modelDetail";
import { startRequest } from "$lib/chat/applyTargetChoice";
import { detailSidebar, shownTab } from "$lib/stores/detailSidebar";
import {
  INFERENCE_FIELDS,
  INFERENCE_SECTIONS,
  INFO_ROWS,
  LOAD_FIELDS,
  LOAD_SECTIONS,
  PANEL_TABS,
  SCHEMA_ERROR_KEY,
  TYPE_CHOICES,
  authorFieldRows,
  type AuthorFieldFacts,
  canSetDefault,
  chatTargetOf,
  defaultPresetWrite,
  devSections,
  fieldsOf,
  formChanged,
  formFromPreset,
  formSettings,
  headActions,
  infoRows,
  initialPreset,
  isAuto,
  isLoadSettingsRefusal,
  loadTabView,
  newPresetWrite,
  panelTabs,
  overrideOf,
  panelSelection,
  parseField,
  presetNameError,
  presetUpdateWrite,
  presetsOf,
  relaunchWarning,
  sourceToggle,
  sourceView,
  shownPanelTab,
  tabLabelKey,
  typeChoiceOf,
  typeExplanationKey,
  typeLabelKey,
} from "./modelDetail";

const HERE = path.dirname(fileURLToPath(import.meta.url));

function source(file: string): string {
  return readFileSync(path.join(HERE, file), "utf8");
}

function read(catalog: unknown, key: string): unknown {
  return key.split(".").reduce<unknown>(
    (node, part) => (node && typeof node === "object" ? (node as Record<string, unknown>)[part] : undefined),
    catalog,
  );
}

/** Whether the French catalogue holds its own text for `key`, not the English one. */
function translated(key: string): boolean {
  const text = read(fr, key);
  return typeof text === "string" && text.trim() !== "" && text !== read(en, key);
}

function model(over: Partial<InstalledModel> = {}): InstalledModel {
  return {
    id: "Qwen3-8B-Q4_K_M",
    path: "/models/Qwen3-8B-Q4_K_M.gguf",
    name: "Qwen3-8B-Q4_K_M.gguf",
    size_bytes: 5_030_000_000,
    kind: "llm",
    format: "gguf",
    category: "language",
    parameters: "8B",
    modified_at: null,
    pinned_at: null,
    origin_verified: true,
    in_use: false,
    used_by: null,
    capabilities: null,
    source_url: null,
    embedding_entry: null,
    default_chat: false,
    default_search: false,
    ...over,
  };
}

function detail(over: Partial<ModelDetail> = {}): ModelDetail {
  return {
    id: "Qwen3-8B-Q4_K_M",
    file: "Qwen3-8B-Q4_K_M.gguf",
    format: "gguf",
    quantization: "Q4_K_M",
    architecture: "qwen3",
    detected_category: "language",
    category_override: null,
    source_file_override: null,
    source_candidates: [],
    author_fields: [],
    ...over,
  };
}

function preset(over: Partial<PresetDetail> = {}): PresetDetail {
  return {
    id: 4,
    name: "Precise",
    model_source: "local",
    model_id: "Qwen3-8B-Q4_K_M",
    load: {},
    request: {},
    is_model_default: false,
    ...over,
  };
}

function okSettings(form: ReturnType<typeof formFromPreset>) {
  const settings = formSettings(form);
  if (!settings.ok) throw new Error("the form was expected to be readable");
  return settings;
}

describe("the panel opens on a model of the list", () => {
  test("the panel shows the model named, as the list has it now, and whether it is in memory", () => {
    // GIVEN two installed models, the first in memory
    const first = model();
    const second = model({ id: "Mistral-7B", path: "/models/Mistral-7B.gguf" });
    const loaded = new Set([first.path]);
    // WHEN the panel of each is asked, then of a model that left the list
    // THEN each selection carries its model and its residency, and the last is none
    expect(panelSelection([first, second], loaded, first.id)).toEqual({ model: first, loaded: true });
    expect(panelSelection([first, second], loaded, second.id)).toEqual({ model: second, loaded: false });
    expect(panelSelection([first, second], loaded, "gone")).toBeNull();
    expect(panelSelection([first, second], loaded, null)).toBeNull();
  });

  test("the detail sidebar offers the Model tab only while a panel is open", () => {
    // GIVEN the detail sidebar opened by a click on a row
    detailSidebar.reset();
    detailSidebar.openModel();
    let state = { open: false, tab: "guide" as string };
    const stop = detailSidebar.subscribe((s) => (state = s));
    // WHEN a model is offered, then not, then the panel is left
    // THEN the tab is shown, falls back to the guide, and the column collapses
    expect(state).toEqual({ open: true, tab: "model" });
    expect(shownTab("model", false, false, true)).toBe("model");
    expect(shownTab("model", false, false, false)).toBe("guide");
    detailSidebar.closeModel();
    expect(state).toEqual({ open: false, tab: "guide" });
    stop();
    detailSidebar.reset();
  });

  test("the three tabs are Infos, Loading and Inference, in that order, in both languages", () => {
    // GIVEN the tabs of the panel
    // WHEN their names are read from each catalogue
    const names = (catalog: unknown) => PANEL_TABS.map((tab) => read(catalog, tabLabelKey(tab)));
    // THEN there are three, named in English as the reference names them, and each has its French name
    expect(PANEL_TABS).toEqual(["info", "load", "inference"]);
    expect(names(en)).toEqual(["Info", "Loading", "Inference"]);
    expect(names(fr).every((name) => typeof name === "string" && name.trim() !== "")).toBe(true);
  });

  test("with the developer mode off the panel offers Infos alone, with it on the three tabs", () => {
    // GIVEN the two modes
    // WHEN the tabs of the panel are read
    // THEN Loading and Inference exist with the mode on only, after Infos
    expect(panelTabs(false)).toEqual(["info"]);
    expect(panelTabs(true)).toEqual(["info", "load", "inference"]);
  });

  test("a tab the mode no longer offers falls back to Infos", () => {
    // GIVEN a panel left on Loading, then on Inference
    // WHEN the developer mode is on, then turned off
    // THEN the tab stays with the mode on and Infos is shown with it off
    expect(shownPanelTab("load", true)).toBe("load");
    expect(shownPanelTab("inference", true)).toBe("inference");
    expect(shownPanelTab("load", false)).toBe("info");
    expect(shownPanelTab("inference", false)).toBe("info");
    expect(shownPanelTab("info", false)).toBe("info");
  });

  test("the panel draws its tab bar and its content from the tabs of the mode in force", () => {
    // GIVEN the source of the panel
    const src = source("ModelPanel.svelte");
    // WHEN its tab bar and the tab it shows are read
    // THEN both follow the developer mode, and the full list is named nowhere
    expect(src).toMatch(/panelTabs\(\$developerMode\)\.map\(/);
    expect(src).toMatch(/const tab = \$derived\(shownPanelTab\(asked, \$developerMode\)\)/);
    expect(src).not.toContain("PANEL_TABS");
  });

  test("a row of the list opens the panel by a click and by its gear", () => {
    // GIVEN the source of the installed list
    const src = source("InstalledModelsSection.svelte");
    // WHEN its row and its gear are read
    // THEN both call the opening of the panel
    expect(src).toMatch(/onclick=\{\(event\) => openFromRow\(event, model\)\}/);
    expect(src).toMatch(/onclick=\{\(\) => onopen\(model\)\}[\s\S]*triggerTestid="ai-model-settings-btn"/);
  });
});

describe("the head of the panel", () => {
  test("a language model offers a conversation and its loading, then its unloading once in memory", () => {
    // GIVEN a language model, out of memory then in memory
    const m = model();
    // WHEN the head is read in both states
    // THEN the conversation is offered, and the second button follows the residency
    expect(headActions(m, false)).toEqual({ chat: { enabled: true }, load: { kind: "load", enabled: true } });
    expect(headActions(m, true).load).toEqual({ kind: "unload", enabled: true });
  });

  test("a model that does not converse offers neither, and says why", () => {
    // GIVEN an embeddings model and a dictation model
    const embeddings = headActions(model({ category: "embeddings" }), false);
    const dictation = headActions(model({ category: "dictation" }), false);
    // WHEN their heads are read
    // THEN both buttons are inactive, each with the reason of its category
    expect(embeddings.chat).toEqual({ enabled: false, reasonKey: "ai.panel.head.reason_embeddings" });
    expect(embeddings.load.enabled).toBe(false);
    expect(dictation.chat.reasonKey).toBe("ai.panel.head.reason_dictation");
  });

  test("the new conversation is started on this model, as the selector of a conversation would", () => {
    // GIVEN an installed model
    const m = model();
    // WHEN the target of "Use in a new conversation" is turned into a request
    const request = startRequest(chatTargetOf(m));
    // THEN the conversation is a free chat on the index identifier of the model
    expect(request).toEqual({ mode: "libre", model: "Qwen3-8B-Q4_K_M" });
  });
});

describe("the Infos tab", () => {
  test("the lines come in the order of the reference", () => {
    // GIVEN a model and its detail
    // WHEN the lines are read
    const rows = infoRows(model(), detail());
    // THEN model, file, format, quantisation, architecture, capabilities, domain, size
    expect(rows.map((r) => r.id)).toEqual([
      "model",
      "file",
      "format",
      "quantization",
      "architecture",
      "capabilities",
      "domain",
      "size",
    ]);
    expect(rows.map((r) => r.id)).toEqual([...INFO_ROWS]);
    expect(rows.slice(0, 5).map((r) => r.value)).toEqual([
      "Qwen3-8B-Q4_K_M",
      "Qwen3-8B-Q4_K_M.gguf",
      "GGUF",
      "Q4_K_M",
      "qwen3",
    ]);
  });

  test("the domain is the category of the index, and what the weights do not say reads unknown", () => {
    // GIVEN an embeddings model whose detail names neither quantisation nor architecture
    const rows = infoRows(
      model({ category: "embeddings" }),
      detail({ quantization: null, architecture: null }),
    );
    const by = (id: string) => rows.find((r) => r.id === id);
    // WHEN its lines are read
    // THEN the domain is its category and the two unknowns say so
    expect(by("domain")?.valueKey).toBe("ai.category.embeddings");
    expect(by("quantization")).toMatchObject({ value: null, valueKey: "ai.panel.info.unknown" });
    expect(by("architecture")?.valueKey).toBe("ai.panel.info.unknown");
  });

  test("before the detail answers, the file is the name the list holds", () => {
    // GIVEN a model whose detail is not read yet
    // WHEN its lines are read
    const rows = infoRows(model(), null);
    // THEN the file line already reads
    expect(rows.find((r) => r.id === "file")?.value).toBe("Qwen3-8B-Q4_K_M.gguf");
  });

  test("the tab shows the identifier of the index, and no server address", () => {
    // GIVEN the source of the Infos tab
    const src = source("ModelInfoTab.svelte");
    // WHEN it is read
    // THEN the identifier is the one of the model, and nothing names an address or a port
    expect(src).toMatch(/data-testid="ai-panel-identifier"\s*>\{model\.id\}/);
    expect(src).not.toMatch(/localhost|127\.0\.0\.1|base_url|:7771/);
  });
});

describe("the Loading tab", () => {
  test("a GGUF model shows its settings, an MLX model the sentence that says it takes none", () => {
    // GIVEN the two formats
    // WHEN the view of the tab is read
    // THEN a GGUF has settings and an MLX has none
    expect(loadTabView("gguf")).toBe("settings");
    expect(loadTabView("mlx")).toBe("none");
    expect(read(en, "ai.panel.load.none")).toBe("No configurable parameters");
    expect(translated("ai.panel.load.none")).toBe(true);
  });

  test("the two sections hold the context and the offload, then the advanced settings", () => {
    // GIVEN the sections of the tab
    // WHEN their fields are read
    const context = fieldsOf(LOAD_FIELDS, "context").map((f) => f.key);
    const advanced = fieldsOf(LOAD_FIELDS, "advanced").map((f) => f.key);
    // THEN the first holds the two the loading window asks, the second the rest
    expect([...LOAD_SECTIONS]).toEqual(["context", "advanced"]);
    expect(context).toEqual(["n_ctx", "n_gpu_layers"]);
    expect(advanced).toEqual([
      "n_threads",
      "n_batch",
      "n_ubatch",
      "n_parallel",
      "kv_unified",
      "ctx_checkpoints",
      "reasoning_budget",
      "reasoning_budget_message",
      "rope_freq_base",
      "rope_freq_scale",
      "kv_offload",
      "mlock",
      "mmap",
      "seed",
      "chat_template",
      "flash_attn",
      "cache_type_k",
      "cache_type_v",
    ]);
  });

  test("every setting is automatic until it is set", () => {
    // GIVEN the form of a model with no preset, then of a preset fixing two settings
    const none = formFromPreset(null);
    const fixed = formFromPreset(preset({ load: { n_ctx: 16384, mlock: true } }));
    // WHEN each setting is read
    // THEN all are automatic without a preset, and only the two fixed ones are not
    expect(LOAD_FIELDS.every((f) => isAuto(none.load[f.key]))).toBe(true);
    expect(LOAD_FIELDS.filter((f) => !isAuto(fixed.load[f.key])).map((f) => f.key)).toEqual([
      "n_ctx",
      "mlock",
    ]);
    expect(fixed.load.n_ctx).toBe("16384");
    expect(fixed.load.mlock).toBe("true");
    expect(read(fr, "ai.panel.auto")).toBe("Auto");
    expect(read(en, "ai.panel.auto")).toBe("Auto");
  });

  test("the sections are folded when the tab opens", () => {
    // GIVEN the sources of the two tabs of settings
    // WHEN their folding sections are read
    // THEN none is opened by default
    for (const file of ["ModelLoadTab.svelte", "ModelInferenceTab.svelte", "ModelDevSections.svelte"]) {
      const tags = source(file).match(/<Disclosure\b[^>]*>/g) ?? [];
      expect(tags.length).toBeGreaterThan(0);
      for (const tag of tags) expect(tag).not.toMatch(/\bopen\b/);
    }
  });

  test("changing a load setting of a model in memory warns that it will be relaunched", () => {
    // GIVEN the form the panel opened with, then a context length typed
    const baseline = formFromPreset(null);
    const edited = formFromPreset(null);
    edited.load.n_ctx = "8192";
    const sampling = formFromPreset(null);
    sampling.request.temperature = "0.2";
    // WHEN the warning is asked for a model in memory and for one that is not
    // THEN it shows for the loaded model alone, and never for a request setting
    expect(relaunchWarning(true, edited, baseline)).toBe(true);
    expect(relaunchWarning(false, edited, baseline)).toBe(false);
    expect(relaunchWarning(true, baseline, baseline)).toBe(false);
    expect(relaunchWarning(true, sampling, baseline)).toBe(false);
  });
});

describe("the Inference tab", () => {
  test("the settings come in the order of the reference, with no system prompt", () => {
    // GIVEN the request settings of the tab
    // WHEN their keys are read
    const keys = INFERENCE_FIELDS.map((f) => f.key);
    // THEN they follow the reference, and none is a system prompt
    expect(keys).toEqual([
      "temperature",
      "max_tokens",
      "stop",
      "top_k",
      "repeat_penalty",
      "presence_penalty",
      "top_p",
      "min_p",
    ]);
    expect([...INFERENCE_SECTIONS]).toEqual(["settings", "sampling", "structured"]);
    expect(keys.some((k) => k.includes("system"))).toBe(false);
    expect(source("ModelInferenceTab.svelte")).not.toMatch(/system_prompt|systemPrompt/);
  });

  test("structured output carries the help of the reference, translated", () => {
    // GIVEN the help of the JSON Schema zone
    // WHEN it is read in each language
    // THEN it is the sentence of the reference, and the French catalogue translates it
    expect(read(en, "ai.panel.inference.structured.help")).toBe(
      "Advanced: you can provide a JSON Schema to enforce a particular output format from the model. Read the documentation to learn more.",
    );
    expect(translated("ai.panel.inference.structured.help")).toBe(true);
  });

  test("the schema is written only while the box is ticked, and has to be a JSON object", () => {
    // GIVEN a form with a schema typed, the box ticked then not
    const form = formFromPreset(null);
    form.schema = '{ "type": "object" }';
    form.structured = true;
    const ticked = okSettings(form);
    const unticked = okSettings({ ...form, structured: false });
    // WHEN the settings are read, then with a text that is not JSON and one that is not an object
    const broken = formSettings({ ...form, schema: "{ nope" });
    const list = formSettings({ ...form, schema: "[1, 2]" });
    // THEN the schema is written only when ticked, and both wrong texts are refused
    expect(ticked.request.json_schema).toEqual({ type: "object" });
    expect("json_schema" in unticked.request).toBe(false);
    expect(broken).toEqual({ ok: false, errors: { [SCHEMA_ERROR_KEY]: "ai.panel.error.schema_json" } });
    expect(list).toEqual({ ok: false, errors: { [SCHEMA_ERROR_KEY]: "ai.panel.error.schema_object" } });
  });

  test("every setting of both tabs has its name and its explanation in both languages", () => {
    // GIVEN every setting of the Loading and the Inference tabs
    const keys = [...LOAD_FIELDS, ...INFERENCE_FIELDS].flatMap((f) => [f.labelKey, f.helpKey]);
    // WHEN their texts are read from each catalogue
    const missing = [fr, en].flatMap((catalog) =>
      keys.filter((key) => {
        const value = read(catalog, key);
        return typeof value !== "string" || value.trim() === "";
      }),
    );
    // THEN none is missing
    expect(missing).toEqual([]);
  });
});

describe("the settings a form is worth", () => {
  test("a setting left automatic is not written, a typed one is written with its type", () => {
    // GIVEN a form with a few settings typed
    const form = formFromPreset(null);
    form.load.n_ctx = "16384";
    form.load.kv_offload = "false";
    form.load.flash_attn = "on";
    form.request.temperature = "0,7";
    form.request.stop = "</s>\n\nEND";
    // WHEN it is read
    const settings = okSettings(form);
    // THEN only those are written, numbers as numbers and the stop strings as a list
    expect(settings.load).toEqual({ n_ctx: 16384, kv_offload: false, flash_attn: "on" });
    expect(settings.request).toEqual({ temperature: 0.7, stop: ["</s>", "END"] });
  });

  test("a value that cannot be read is refused at its field", () => {
    // GIVEN a whole number written with a decimal, a text for a number, a value out of range
    const form = formFromPreset(null);
    form.load.n_ctx = "12.5";
    form.request.temperature = "hot";
    form.request.top_p = "3";
    // WHEN the form is read
    const settings = formSettings(form);
    // THEN each field carries its refusal and nothing is written
    expect(settings).toEqual({
      ok: false,
      errors: {
        n_ctx: "ai.panel.error.integer",
        temperature: "ai.panel.error.number",
        top_p: "ai.panel.error.range",
      },
    });
    const flash = LOAD_FIELDS.find((f) => f.key === "flash_attn");
    expect(flash && parseField(flash, "sometimes")).toEqual({
      kind: "invalid",
      errorKey: "ai.panel.error.choice",
    });
  });

  test("a preset read back gives the form that wrote it", () => {
    // GIVEN a preset holding load and request settings and a schema
    const held = preset({
      load: { n_ctx: 8192, mmap: false, chat_template: "{{ messages }}" },
      request: { temperature: 0.2, top_k: 40, stop: ["END"], json_schema: { type: "object" } },
    });
    // WHEN its form is read back into settings
    const settings = okSettings(formFromPreset(held));
    // THEN they are the ones the preset holds
    expect(settings.load).toEqual(held.load);
    expect(settings.request).toEqual(held.request);
  });
});

describe("the presets of a model", () => {
  const presets = [
    preset({ id: 1, name: "Varied", is_model_default: true }),
    preset({ id: 2, name: "Precise" }),
    preset({ id: 3, name: "Other", model_id: "Mistral-7B" }),
    preset({ id: 4, name: "Served", model_source: "server" }),
  ];

  test("the panel lists the presets of its model and opens on its default", () => {
    // GIVEN the presets of several models
    // WHEN those of one local model are read
    const mine = presetsOf(presets, "Qwen3-8B-Q4_K_M");
    // THEN only its own are listed, by name, and the default is the one shown first
    expect(mine.map((p) => p.name)).toEqual(["Precise", "Varied"]);
    expect(initialPreset(mine)?.id).toBe(1);
    expect(initialPreset(presetsOf(presets, "Mistral-7B"))).toBeNull();
  });

  test("choosing a preset shows its settings", () => {
    // GIVEN a preset fixing a temperature and a context
    const chosen = preset({ load: { n_ctx: 4096 }, request: { temperature: 0.1 } });
    // WHEN it is chosen
    const form = formFromPreset(chosen);
    // THEN the form carries both, and nothing differs from the preset
    expect(form.load.n_ctx).toBe("4096");
    expect(form.request.temperature).toBe("0.1");
    expect(formChanged(form, formFromPreset(chosen))).toBe(false);
  });

  test("Save as a preset writes the settings on screen under a new name", () => {
    // GIVEN the settings typed and a name
    const form = formFromPreset(null);
    form.request.temperature = "0.3";
    form.load.n_ctx = "8192";
    // WHEN the preset is written, with load settings kept by presets
    const write = newPresetWrite("  Careful  ", okSettings(form), true);
    // THEN it carries the name trimmed, both groups of settings, and is not the default
    expect(write).toEqual({
      name: "Careful",
      load: { n_ctx: 8192 },
      request: { temperature: 0.3 },
      is_model_default: false,
    });
  });

  test("a name left empty or already taken by this model is refused", () => {
    // GIVEN the presets of the model
    const mine = presetsOf(presets, "Qwen3-8B-Q4_K_M");
    // WHEN names are tried
    // THEN an empty one and a taken one are refused, whatever the case, a new one passes
    expect(presetNameError("   ", mine)).toBe("ai.panel.preset.error_name_empty");
    expect(presetNameError("precise", mine)).toBe("ai.panel.preset.error_name_taken");
    expect(presetNameError("Other", mine)).toBeNull();
  });

  test("while presets do not keep load settings, a write carries none of the typed ones", () => {
    // GIVEN load settings typed, and a preset that already holds some
    const form = formFromPreset(null);
    form.load.n_ctx = "8192";
    const settings = okSettings(form);
    const held = preset({ load: { n_threads: 6 }, is_model_default: true });
    // WHEN a new preset and a modification are written with the setting off
    const created = newPresetWrite("New", settings, false);
    const updated = presetUpdateWrite(held, settings, false);
    // THEN the new one carries none, and the modified one keeps what it held
    expect(created.load).toEqual({});
    expect(updated.load).toEqual({ n_threads: 6 });
    expect(updated.is_model_default).toBe(true);
    expect(presetUpdateWrite(held, settings, true).load).toEqual({ n_ctx: 8192 });
  });

  test("Set as default writes the preset as held, made the default of its model", () => {
    // GIVEN a preset that is not the default
    const chosen = preset({ load: { n_ctx: 4096 }, request: { temperature: 0.1 } });
    // WHEN it is made the default
    const write = defaultPresetWrite(chosen);
    // THEN its settings are untouched and it is the default
    expect(write).toEqual({
      name: "Precise",
      load: { n_ctx: 4096 },
      request: { temperature: 0.1 },
      is_model_default: true,
    });
  });

  test("Set as default needs a preset chosen, not already the default, with nothing unsaved", () => {
    // GIVEN the states of the bar
    // WHEN the button is asked
    // THEN it is offered only for a saved preset that is not the default
    expect(canSetDefault(preset(), false)).toBe(true);
    expect(canSetDefault(null, false)).toBe(false);
    expect(canSetDefault(preset({ is_model_default: true }), false)).toBe(false);
    expect(canSetDefault(preset(), true)).toBe(false);
  });

  test("the refusal of load settings by the runtime is recognised by the setting it names", () => {
    // GIVEN the message of the route and another error
    const refusal =
      "load settings are not written to a preset while models.preset_load_settings is off: send request settings only, or turn the setting on";
    // WHEN each is read
    // THEN only the first is the refusal the panel explains in its own words
    expect(isLoadSettingsRefusal(refusal)).toBe(true);
    expect(isLoadSettingsRefusal("the model already has a preset of that name")).toBe(false);
  });

  test("the bar offers the three gestures", () => {
    // GIVEN the source of the bar of presets
    const src = source("ModelPresetBar.svelte");
    // WHEN its controls are read
    // THEN choosing, saving as a preset and setting the default are there
    for (const id of ["ai-panel-preset-select", "ai-panel-preset-save-as", "ai-panel-preset-set-default"]) {
      expect(src).toContain(`data-testid="${id}"`);
    }
  });
});

describe("the sections of the developer mode", () => {
  test("with the developer mode off no section exists, with it on both do", () => {
    // GIVEN the two modes
    // WHEN the sections under the tabs are read
    // THEN none with the mode off, the type and the source file with it on
    expect(devSections(false)).toEqual([]);
    expect(devSections(true)).toEqual(["type", "source"]);
  });

  test("the component renders nothing but the sections of the mode in force", () => {
    // GIVEN the source of the developer sections
    const src = source("ModelDevSections.svelte");
    const markup = src.slice(src.indexOf("</script>") + "</script>".length).trim();
    // WHEN its markup is read
    // THEN everything it renders sits inside the loop over the sections of the mode
    expect(src).toMatch(/const sections = \$derived\(devSections\(\$developerMode\)\)/);
    expect(markup.startsWith("{#each sections as section (section)}")).toBe(true);
    expect(markup.endsWith("{/each}")).toBe(true);
    expect(markup.match(/\{#each sections as section/g)?.length).toBe(1);
  });

  test("the type starts on no override, then offers the four categories, each explained", () => {
    // GIVEN the choices of "Model type"
    // WHEN they are read, with their texts in both languages
    const texts = (catalog: unknown) =>
      TYPE_CHOICES.flatMap((c) => [read(catalog, typeLabelKey(c)), read(catalog, typeExplanationKey(c))]);
    // THEN no override comes first, and every choice has a name and an explanation
    expect([...TYPE_CHOICES]).toEqual(["none", "language", "embeddings", "dictation", "draft"]);
    expect(typeChoiceOf(detail())).toBe("none");
    expect(typeChoiceOf(detail({ category_override: "draft" }))).toBe("draft");
    expect(read(en, typeLabelKey("none"))).toBe("No override");
    expect(translated(typeLabelKey("none"))).toBe(true);
    for (const catalog of [fr, en]) {
      expect(texts(catalog).every((v) => typeof v === "string" && v.trim() !== "")).toBe(true);
    }
  });

  test("a type choice writes its category to the index, and no override removes it", () => {
    // GIVEN the choices
    // WHEN each is turned into what the index is written with
    // THEN a category is written as is and no override writes none
    expect(overrideOf("embeddings")).toBe("embeddings");
    expect(overrideOf("none")).toBeNull();
  });

  test("the source file lists the other weights files, and the switch turns the override on and off", () => {
    // GIVEN a model with another quantisation on this computer
    const other = { path: "/models/Qwen3-8B-Q8_0.gguf", name: "Qwen3-8B-Q8_0.gguf", quantization: "Q8_0", size_bytes: 8e9 };
    const off = detail({ source_candidates: [other] });
    const on = detail({ source_candidates: [other], source_file_override: other.path });
    // WHEN the section is read in both states and the switch is turned
    // THEN the override is off then on that file, and turning it writes the file then nothing
    expect(sourceView(off)).toEqual({ enabled: false, available: true, chosen: null });
    expect(sourceView(on)).toEqual({ enabled: true, available: true, chosen: other.path });
    expect(sourceToggle(off, true)).toBe(other.path);
    expect(sourceToggle(on, false)).toBeNull();
  });

  test("a model with no other weights file cannot turn the override on", () => {
    // GIVEN a model alone in its folder
    const alone = detail();
    // WHEN the section is read and the switch is turned on
    // THEN the switch is unavailable and nothing is written
    expect(sourceView(alone).available).toBe(false);
    expect(sourceToggle(alone, true)).toBeUndefined();
  });
});

describe("the fields of the author of the card", () => {
  const fields: AuthorFieldFacts[] = [
    {
      name: "enable_thinking",
      default: true,
      explanation: "The model thinks before it answers.",
    },
    {
      name: "reasoning_effort",
      values: ["low", "medium", "high"],
      default: "medium",
      explanation: "How long the model thinks.",
    },
  ];

  test("each field reads its name, its default and what it changes", () => {
    // GIVEN a boolean switch and a list
    // WHEN the lines of the Inference tab are read
    const rows = authorFieldRows(fields);
    // THEN the boolean reads on, the list its default and its values, each with its explanation
    expect(rows).toEqual([
      {
        name: "enable_thinking",
        explanation: "The model thinks before it answers.",
        defaultText: null,
        defaultKey: "ai.panel.inference.author.on",
        values: [],
      },
      {
        name: "reasoning_effort",
        explanation: "How long the model thinks.",
        defaultText: "medium",
        defaultKey: null,
        values: ["low", "medium", "high"],
      },
    ]);
  });

  test("nothing of the card is written to a preset", () => {
    // GIVEN a form saved while the card declares switches
    const form = formFromPreset(null);
    form.request.temperature = "0.5";
    // WHEN the preset is written
    const write = newPresetWrite("Careful", okSettings(form), true);
    // THEN no key of it names a field of the card
    const written = JSON.stringify(write);
    for (const f of fields) expect(written).not.toContain(f.name);
    expect(Object.keys(write.request)).toEqual(["temperature"]);
  });

  test("the fields are shown to read, with no control", () => {
    // GIVEN the source of the Inference tab
    const src = source("ModelInferenceTab.svelte");
    const section = src.slice(src.indexOf('data-testid="ai-panel-author-fields"'));
    // WHEN the block of the author fields is read
    // THEN it holds the name, the default and the explanation, and no input
    for (const id of ["ai-panel-author-name", "ai-panel-author-default", "ai-panel-author-explanation"]) {
      expect(section).toContain(id);
    }
    expect(section).not.toMatch(/<(Input|Select|Toggle|Checkbox|Textarea|input|select|button)\b/);
  });
});
