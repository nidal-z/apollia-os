import { describe, test, expect } from "vitest";
import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import fr from "$lib/i18n/fr.json";
import en from "$lib/i18n/en.json";
import type { ModelDetail, PresetDetail } from "$lib/ipc/modelDetail";
import {
  DRAFT_MODEL_KEY,
  LOAD_FIELDS,
  draftOptions,
  draftUnavailable,
  fieldsOf,
  formFromPreset,
  formSettings,
  isAuto,
  loadSections,
  loadTabView,
  relaunchWarning,
} from "./modelDetail";

/**
 * The draft model of the Loading tab: "None" until one is chosen, only the
 * drafts of the same vocabulary offered, an explanation on hover, and nothing
 * at all for a model whose engine takes no draft model.
 */

const HERE = path.dirname(fileURLToPath(import.meta.url));

function source(name: string): string {
  return readFileSync(path.join(HERE, name), "utf8");
}

function read(catalog: unknown, key: string): unknown {
  return key
    .split(".")
    .reduce<unknown>(
      (node, part) =>
        node && typeof node === "object" ? (node as Record<string, unknown>)[part] : undefined,
      catalog,
    );
}

/** Whether the French catalogue holds its own text for `key`, not the English one. */
function translated(key: string): boolean {
  const text = read(fr, key);
  return typeof text === "string" && text.trim() !== "" && text !== read(en, key);
}

function detail(over: Partial<ModelDetail> = {}): ModelDetail {
  return {
    id: "Qwen2.5-7B-Instruct-Q4_K_M",
    file: "Qwen2.5-7B-Instruct-Q4_K_M.gguf",
    format: "gguf",
    quantization: "Q4_K_M",
    architecture: "qwen2",
    detected_category: "language",
    category_override: null,
    source_file_override: null,
    source_candidates: [],
    author_fields: [],
    draft_setting: true,
    compatible_drafts: [
      { id: "qwen2.5-coder-0.5b-q8_0", parameters: "0.5B", size_bytes: 531_068_128 },
      { id: "qwen2.5-1.5b-instruct-q4_k_m", parameters: null, size_bytes: 1_100_000_000 },
    ],
    ...over,
  };
}

function preset(load: Record<string, unknown>): PresetDetail {
  return {
    id: 4,
    name: "Fast",
    model_source: "local",
    model_id: "Qwen2.5-7B-Instruct-Q4_K_M",
    load,
    request: {},
    is_model_default: true,
  };
}

describe("the draft model of the Loading tab", () => {
  test("a GGUF model has the speculative decoding section, with the draft model alone", () => {
    // GIVEN the detail of a GGUF model, whose engine takes a draft model
    const gguf = detail();
    // WHEN the sections of its Loading tab are read
    const sections = loadSections(gguf);
    // THEN the section sits between the context and the advanced settings and
    // holds the draft model, named in both languages
    expect(sections).toEqual(["context", "speculative", "advanced"]);
    expect(fieldsOf(LOAD_FIELDS, "speculative").map((f) => f.key)).toEqual([DRAFT_MODEL_KEY]);
    expect(read(en, "ai.panel.load.section.speculative")).toBe("Speculative decoding");
    expect(translated("ai.panel.load.section.speculative")).toBe(true);
    expect(read(en, "ai.panel.load.field.draft_model.label")).toBe("Draft model");
    expect(translated("ai.panel.load.field.draft_model.label")).toBe(true);
  });

  test("an MLX model has no draft model, and no setting stands in for it", () => {
    // GIVEN the detail of an MLX model, whose engine takes no draft model, and
    // a model whose detail is not read yet
    const mlx = detail({ format: "mlx", draft_setting: false, compatible_drafts: [] });
    // WHEN the Loading tab of each is laid out
    const sections = loadSections(mlx);
    const unread = loadSections(null);
    // THEN the tab of an MLX model shows its one sentence, and the section is
    // in neither list
    expect(loadTabView("mlx")).toBe("none");
    expect(sections).not.toContain("speculative");
    expect(unread).not.toContain("speculative");
    expect(draftOptions(mlx, "")).toEqual([]);
  });

  test("the draft model is None until one is chosen", () => {
    // GIVEN a model with no preset, and a preset that names a draft model
    const none = formFromPreset(null);
    const chosen = formFromPreset(preset({ draft_model: "qwen2.5-coder-0.5b-q8_0" }));
    // WHEN the setting is read
    // THEN it is empty without a preset, shown as "None", and holds the
    // identifier of the draft otherwise
    expect(isAuto(none.load[DRAFT_MODEL_KEY])).toBe(true);
    expect(chosen.load[DRAFT_MODEL_KEY]).toBe("qwen2.5-coder-0.5b-q8_0");
    expect(read(en, "ai.panel.load.draft.none")).toBe("None");
    expect(translated("ai.panel.load.draft.none")).toBe(true);
    expect(source("PanelSettingField.svelte")).toContain(
      '<option value="">{$t("ai.panel.load.draft.none")}</option>',
    );
  });

  test("only the compatible drafts are offered", () => {
    // GIVEN a model with two compatible drafts, the runtime having left out
    // the drafts of another vocabulary
    const gguf = detail();
    // WHEN the list is built with no draft chosen, then with one of them
    const free = draftOptions(gguf, "");
    const held = draftOptions(gguf, "qwen2.5-coder-0.5b-q8_0");
    // THEN it is the compatible drafts, in order, each usable, and the note
    // under the list says why the others are not there
    expect(free).toEqual([
      { id: "qwen2.5-coder-0.5b-q8_0", parameters: "0.5B", available: true },
      { id: "qwen2.5-1.5b-instruct-q4_k_m", parameters: null, available: true },
    ]);
    expect(held).toEqual(free);
    expect(draftUnavailable(gguf, "qwen2.5-coder-0.5b-q8_0")).toBe(false);
    expect(read(en, "ai.panel.load.draft.note")).toBe(
      "Only the draft models of the same vocabulary as this model are offered.",
    );
    expect(translated("ai.panel.load.draft.note")).toBe(true);
  });

  test("a draft the preset names and the model can no longer use is shown as unavailable", () => {
    // GIVEN a preset naming a draft that is no longer among the compatible ones
    const gguf = detail();
    const form = formFromPreset(preset({ draft_model: "llama-3.2-1b-q4_k_m" }));
    // WHEN the list is built
    const options = draftOptions(gguf, form.load[DRAFT_MODEL_KEY]);
    // THEN the draft stays in the list, marked, and the tab says the model
    // will load without it
    expect(options.at(-1)).toEqual({
      id: "llama-3.2-1b-q4_k_m",
      parameters: null,
      available: false,
    });
    expect(draftUnavailable(gguf, form.load[DRAFT_MODEL_KEY])).toBe(true);
    expect(translated("ai.panel.load.draft.unavailable")).toBe(true);
    expect(translated("ai.panel.load.draft.unavailable_note")).toBe(true);
  });

  test("choosing a draft writes its identifier, and None writes nothing", () => {
    // GIVEN a form where a draft is chosen, and one left on None
    const chosen = formFromPreset(null);
    chosen.load[DRAFT_MODEL_KEY] = "qwen2.5-coder-0.5b-q8_0";
    const none = formFromPreset(preset({ draft_model: "qwen2.5-coder-0.5b-q8_0" }));
    none.load[DRAFT_MODEL_KEY] = "";
    // WHEN each form is read
    const written = formSettings(chosen);
    const cleared = formSettings(none);
    // THEN the first carries the identifier of the index and no speculative
    // mode, which the runtime fixes; the second carries no draft model
    expect(written).toEqual({
      ok: true,
      load: { draft_model: "qwen2.5-coder-0.5b-q8_0" },
      request: {},
    });
    expect(cleared).toEqual({ ok: true, load: {}, request: {} });
  });

  test("changing the draft of a model in memory warns that it will be relaunched", () => {
    // GIVEN a loaded model whose panel opened without a draft
    const baseline = formFromPreset(null);
    const form = formFromPreset(null);
    // WHEN a draft is chosen
    form.load[DRAFT_MODEL_KEY] = "qwen2.5-coder-0.5b-q8_0";
    // THEN the relaunch warning shows, as for any load setting
    expect(relaunchWarning(true, form, baseline)).toBe(true);
    expect(relaunchWarning(false, form, baseline)).toBe(false);
  });

  test("the draft model explains itself on hover, in both languages, with no native title", () => {
    // GIVEN the setting, and the component that draws every setting
    const field = LOAD_FIELDS.find((f) => f.key === DRAFT_MODEL_KEY);
    const row = source("PanelSettingField.svelte");
    const tab = source("ModelLoadTab.svelte");
    // WHEN its explanation is read from the catalogues
    const helpKey = field?.helpKey ?? "";
    // THEN it says what a draft model does and that the answer is unchanged,
    // it is drawn by the house Tooltip, and the list is addressable
    expect(read(en, helpKey)).toBe(
      "A small model of the same vocabulary proposes what comes next and this model checks it: same answer, faster.",
    );
    expect(translated(helpKey)).toBe(true);
    expect(row).toContain("<Tooltip content={$t(field.helpKey)}>");
    expect(row).not.toMatch(/\stitle=/);
    expect(row).toMatch(/field\.kind === "draft"[\s\S]*?data-testid=\{id\}/);
    expect(tab).toContain("draftOptions(detail, form.load[DRAFT_MODEL_KEY])");
    expect(tab).toContain('data-testid="ai-panel-load-draft-note"');
  });
});
