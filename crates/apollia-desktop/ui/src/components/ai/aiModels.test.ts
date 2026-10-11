import { describe, test, expect } from "vitest";
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import type { InstalledModel } from "$lib/ipc/modelHub";
import type { LoadedModel } from "$lib/types";
import {
  categoryCount,
  defaultAction,
  filterShortcutLabel,
  gigabytes,
  groupModels,
  hostPlatform,
  isFilterShortcut,
  loadedFormat,
  loadedKindKey,
  loadedMemoryBytes,
  menuEntries,
  parameterCount,
  revealLabelKey,
  shownModelName,
  sortModels,
  totalBytes,
} from "./aiModels";

function model(over: Partial<InstalledModel> & { id: string }): InstalledModel {
  return {
    path: `/m/${over.id}.gguf`,
    name: `${over.id}.gguf`,
    size_bytes: 1_000_000_000,
    kind: "llm",
    format: "gguf",
    category: "language",
    parameters: null,
    modified_at: null,
    pinned_at: null,
    origin_verified: true,
    in_use: false,
    used_by: null,
    capabilities: null,
    source_url: "https://huggingface.co/org/repo",
    embedding_entry: null,
    default_chat: false,
    default_search: false,
    ...over,
  };
}

function loaded(over: Partial<LoadedModel>): LoadedModel {
  return {
    engine: "llama-server",
    kind: "chat",
    model: "qwen",
    modelPath: "/m/qwen.gguf",
    port: 1,
    nCtx: 4096,
    nGpuLayers: 99,
    loadedAt: "2026-10-04T08:00:00Z",
    lastUsedAt: "2026-10-04T08:00:00Z",
    activeRequests: 0,
    ...over,
  };
}

const MODELS = [
  model({ id: "qwen", parameters: "27B" }),
  model({ id: "gemma", parameters: "4B" }),
  model({ id: "e5", category: "embeddings", embedding_entry: "multilingual-e5-small" }),
  model({ id: "bge", category: "embeddings" }),
  model({ id: "whisper", category: "dictation", kind: "stt" }),
];

describe("categories and filter", () => {
  test("each category counts its models and All counts every one", () => {
    // GIVEN two language, two embeddings and one dictation model
    // WHEN each entry of the column is counted
    const counts = ["all", "language", "embeddings", "dictation", "draft"].map((f) =>
      categoryCount(MODELS, f as never),
    );
    // THEN the counts follow the categories of the index
    expect(counts).toEqual([5, 2, 2, 1, 0]);
  });

  test("All lists one group per non-empty category, in screen order", () => {
    // GIVEN the models of three categories
    // WHEN the All entry is shown with no filter text
    const groups = groupModels(MODELS, "all", "");
    // THEN language, embeddings and dictation appear in that order, drafts left out
    expect(groups.map((g) => [g.category, g.models.map((m) => m.id)])).toEqual([
      ["language", ["gemma", "qwen"]],
      ["embeddings", ["bge", "e5"]],
      ["dictation", ["whisper"]],
    ]);
  });

  test("a chosen empty category is still shown, so the screen can say it is empty", () => {
    // GIVEN no draft model
    // WHEN the Drafts entry is chosen
    const groups = groupModels(MODELS, "draft", "");
    // THEN one empty group is returned
    expect(groups).toEqual([{ category: "draft", models: [] }]);
  });

  test("the filter text matches the name, the identifier and the parameter count", () => {
    // GIVEN the models of the machine
    // WHEN the filter reads a part of a name, then a parameter count
    const byName = groupModels(MODELS, "all", "GEM").flatMap((g) => g.models.map((m) => m.id));
    const byParams = groupModels(MODELS, "language", "27b").flatMap((g) => g.models.map((m) => m.id));
    // THEN only the matching models remain
    expect(byName).toEqual(["gemma"]);
    expect(byParams).toEqual(["qwen"]);
  });

  test("a pinned model stays first of its category, the latest pin on top", () => {
    // GIVEN three models, two pinned at different times
    const list = [
      model({ id: "a" }),
      model({ id: "b", pinned_at: "2026-10-01T00:00:00Z" }),
      model({ id: "c", pinned_at: "2026-10-03T00:00:00Z" }),
    ];
    // WHEN they are sorted for the table
    const order = sortModels(list).map((m) => m.id);
    // THEN the latest pin leads, then the older one, then the rest by name
    expect(order).toEqual(["c", "b", "a"]);
  });
});

describe("footer and sizes", () => {
  test("the footer sums every model and prints gigabytes in the reader's locale", () => {
    // GIVEN five models of one gigabyte each
    // WHEN their total is printed in French and in English
    const total = totalBytes(MODELS);
    // THEN the size reads with the decimal separator of each language
    expect(total).toBe(5_000_000_000);
    expect(gigabytes(16_100_000_000, "fr")).toBe("16,1");
    expect(gigabytes(16_100_000_000, "en")).toBe("16.1");
  });

  test("a parameter count of the index reads in billions or millions", () => {
    // GIVEN labels of the index
    // WHEN they are read for display
    // THEN the number and its unit are split, and an odd label is left as written
    expect(parameterCount("27B", "fr")).toEqual({ value: "27", unitKey: "ai.installed.params_billions" });
    expect(parameterCount("1.5B", "fr")).toEqual({ value: "1,5", unitKey: "ai.installed.params_billions" });
    expect(parameterCount("137M", "en")).toEqual({ value: "137", unitKey: "ai.installed.params_millions" });
    expect(parameterCount("8x7B", "en")).toBeNull();
    expect(parameterCount(null, "en")).toBeNull();
  });
});

describe("loaded models", () => {
  test("the memory of a loaded model is the guardrail estimate, else its weights", () => {
    // GIVEN a model with an estimate, one with only its size, one with neither
    // WHEN their memory is read
    // THEN the estimate wins, the size is the fallback, and nothing is invented
    expect(loadedMemoryBytes(loaded({ footprintGb: 4.5, sizeBytes: 3e9 }))).toBe(4.5e9);
    expect(loadedMemoryBytes(loaded({ footprintGb: null, sizeBytes: 3e9 }))).toBe(3e9);
    expect(loadedMemoryBytes(loaded({}))).toBeNull();
  });

  test("a loaded model shows its kind and its format", () => {
    // GIVEN the embeddings instance and an MLX chat model
    const embeddings = loaded({ kind: "embeddings" });
    const mlx = loaded({ engine: "vllm-mlx" });
    // WHEN their labels are chosen
    // THEN embeddings and language models are told apart, and the format follows the engine
    expect(loadedKindKey(embeddings)).toBe("ai.kind.embeddings");
    expect(loadedKindKey(mlx)).toBe("ai.kind.language");
    expect(loadedFormat(mlx)).toBe("mlx");
    expect(loadedFormat(embeddings)).toBe("gguf");
  });
});

describe("actions of a row", () => {
  test("an embeddings model is offered for search, never for chat", () => {
    // GIVEN an embeddings model the registry knows, one it does not, and a language model
    const e5 = MODELS[2] as InstalledModel;
    const bge = MODELS[3] as InstalledModel;
    const qwen = MODELS[0] as InstalledModel;
    // WHEN their default action and menu are read
    const e5Menu = menuEntries(e5, "mac").map((e) => e.id);
    const bgeMenu = menuEntries(bge, "mac").map((e) => e.id);
    // THEN the embeddings model offers search only, and the language model chat
    expect(defaultAction(e5)).toBe("use_search");
    expect(e5Menu).toContain("use_search");
    expect(e5Menu).not.toContain("use_default");
    expect(bgeMenu).not.toContain("use_default");
    expect(bgeMenu).not.toContain("use_search");
    expect(defaultAction(qwen)).toBe("use_default");
  });

  test("the model already in use by default offers no default action", () => {
    // GIVEN the default chat model and the search model
    const chat = model({ id: "qwen", default_chat: true });
    const search = model({
      id: "e5",
      category: "embeddings",
      embedding_entry: "multilingual-e5-small",
      default_search: true,
    });
    // WHEN their default action is read
    // THEN neither offers to become what it already is
    expect(defaultAction(chat)).toBeNull();
    expect(defaultAction(search)).toBeNull();
  });

  test("the menu lists its entries in order, with the reveal named for each system", () => {
    // GIVEN a downloaded language model
    const qwen = MODELS[0] as InstalledModel;
    // WHEN its menu is built on macOS, and the reveal label on the other systems
    const entries = menuEntries(qwen, "mac");
    // THEN the order is the reference one, and delete closes the menu as a danger
    expect(entries.map((e) => e.id)).toEqual([
      "reveal",
      "pin",
      "copy_id",
      "copy_path",
      "source",
      "use_default",
      "delete",
    ]);
    expect(entries[0]?.labelKey).toBe("ai.menu.reveal_mac");
    expect(revealLabelKey("windows")).toBe("ai.menu.reveal_windows");
    expect(revealLabelKey("linux")).toBe("ai.menu.reveal_linux");
    expect(entries.at(-1)?.destructive).toBe(true);
  });

  test("pin to top is never called keep in memory, and turns into unpin once pinned", () => {
    // GIVEN a model, then the same model pinned
    const free = menuEntries(model({ id: "a" }), "linux");
    const pinned = menuEntries(model({ id: "a", pinned_at: "2026-10-01T00:00:00Z" }), "linux");
    // WHEN the pin entry is read
    const label = (list: typeof free) => list.find((e) => e.id === "pin")?.labelKey;
    // THEN it pins to the top or removes the pin, and no entry names residency
    expect(label(free)).toBe("ai.menu.pin");
    expect(label(pinned)).toBe("ai.menu.unpin");
    expect([...free, ...pinned].some((e) => /memory|memoire|keep/i.test(e.labelKey))).toBe(false);
  });

  test("delete is inactive for a model a configuration still names, with its reason", () => {
    // GIVEN a model nothing names, and one the local engine is configured on
    const free = menuEntries(model({ id: "a" }), "mac");
    const used = menuEntries(model({ id: "b", in_use: true, used_by: "LLM backend 'local'" }), "mac");
    // WHEN the delete entry is read
    const reason = (list: typeof free) => list.find((e) => e.id === "delete")?.reasonKey;
    // THEN the first can be deleted, and the second says why it cannot
    expect(reason(free)).toBeUndefined();
    expect(reason(used)).toBe("ai.menu.delete_in_use");
  });

  test("view source is inactive for an imported model, with its reason", () => {
    // GIVEN a model imported with no known origin
    const imported = model({ id: "local", source_url: null });
    // WHEN its source entry is read
    const source = menuEntries(imported, "mac").find((e) => e.id === "source");
    // THEN it stays in the menu, inactive, with the reason shown on hover
    expect(source?.reasonKey).toBe("ai.menu.source_none");
    expect(menuEntries(MODELS[0] as InstalledModel, "mac").find((e) => e.id === "source")?.reasonKey).toBeUndefined();
  });
});

describe("the filter shortcut", () => {
  test("Command F on macOS and Control F elsewhere focus the filter", () => {
    // GIVEN the key presses of each system
    const cmdF = { key: "f", metaKey: true, ctrlKey: false };
    const ctrlF = { key: "F", metaKey: false, ctrlKey: true };
    // WHEN they are read on each platform
    // THEN each system answers its own shortcut only, and the hint says it
    expect(isFilterShortcut(cmdF, "mac")).toBe(true);
    expect(isFilterShortcut(ctrlF, "mac")).toBe(false);
    expect(isFilterShortcut(ctrlF, "windows")).toBe(true);
    expect(isFilterShortcut(cmdF, "linux")).toBe(false);
    expect(isFilterShortcut({ ...ctrlF, shiftKey: true }, "linux")).toBe(false);
    expect(filterShortcutLabel("mac")).toBe("⌘ F");
    expect(filterShortcutLabel("windows")).toBe("Ctrl F");
  });

  test("the platform is read from the webview", () => {
    // GIVEN the platform strings of the three systems
    // WHEN they are classified
    // THEN each names its file manager family
    expect(hostPlatform("MacIntel")).toBe("mac");
    expect(hostPlatform("Win32")).toBe("windows");
    expect(hostPlatform("Linux x86_64")).toBe("linux");
  });
});

describe("the name a row of the list shows", () => {
  test("it is the name of the model unless the full file name is asked", () => {
    // GIVEN a model whose file is not named like it
    const m = model({ id: "qwen3-8b", name: "Qwen3-8B-Q4_K_M.gguf" });
    // WHEN the row is named with the setting off, then on
    // THEN it shows the name of the model, then the name of the file
    expect(shownModelName(m, false)).toBe("qwen3-8b");
    expect(shownModelName(m, true)).toBe("Qwen3-8B-Q4_K_M.gguf");
  });

  test("a model without a file name keeps its name", () => {
    // GIVEN a model the index gave no file name
    const m = model({ id: "qwen3-8b", name: "" });
    // WHEN the full file name is asked
    // THEN the row still shows a name
    expect(shownModelName(m, true)).toBe("qwen3-8b");
  });

  test("the list of the AI screen names its rows through that rule", () => {
    // GIVEN the source of the section "On this computer"
    const src = readFileSync(
      resolve(dirname(fileURLToPath(import.meta.url)), "InstalledModelsSection.svelte"),
      "utf8",
    );
    // WHEN the cell of the name is read
    // THEN it goes through the rule with the setting of Settings > General
    expect(src).toContain(
      'data-testid="ai-model-name">{shownModelName(model, $modelFullFileName)}</span>',
    );
  });
});
