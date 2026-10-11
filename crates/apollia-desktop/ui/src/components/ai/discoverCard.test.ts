import { beforeEach, describe, expect, test, vi } from "vitest";
import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import fr from "$lib/i18n/fr.json";
import en from "$lib/i18n/en.json";

const invoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));

import {
  getDictationModels,
  getModelReadme,
  type DictationModel,
  type DictationModels,
  type FoundModel,
  type InstalledModel,
  type ModelCardView,
  type ModelVariant,
} from "$lib/ipc/modelHub";
import type { SelectionFamily } from "$lib/ipc/models";
import { startRequest } from "$lib/chat/applyTargetChoice";
import {
  DISCOVER_CATEGORIES,
  chosenVariant,
  dictationEntries,
  dictationVariant,
  discoverCategoryKey,
  downloadAction,
  searchEntries,
  selectionEntries,
  variantKey,
} from "./discover";
import {
  authorFieldRows,
  conversationAction,
  defaultParameterRows,
  domainKey,
  installedMatch,
  precisionLabel,
  purposeKey,
  readmeBody,
  readmeErrorText,
  readmeRepo,
  repoOfSourceUrl,
  showsFormat,
  sourcePage,
} from "./discoverCard";

const HERE = path.dirname(fileURLToPath(import.meta.url));

function read(catalog: unknown, key: string): unknown {
  return key.split(".").reduce<unknown>(
    (node, part) => (node && typeof node === "object" ? (node as Record<string, unknown>)[part] : undefined),
    catalog,
  );
}

function card(over: Partial<ModelCardView> = {}): ModelCardView {
  return {
    id: "qwen3/Qwen3-8B",
    kind: "language",
    label: "Qwen3 8B",
    publisher: "Qwen",
    language: "fr",
    description: "A compact model that reasons and calls tools.",
    license: { id: "apache-2.0", source: "https://huggingface.co/Qwen/Qwen3-8B" },
    source_url: "https://huggingface.co/Qwen/Qwen3-8B",
    capabilities: { tools: true, reasoning: true, vision: false, source: "template" },
    memory: { min_gb: 8, source: "file" },
    defaults: { temperature: 0.6, top_p: 0.95, top_k: 20, repetition_penalty: 1, presence_penalty: 0, source: "generation_config.json" },
    author_fields: [
      {
        name: "enable_thinking",
        type: "booleen",
        default: true,
        explanation: "The model thinks before it answers.",
        source: "chat template",
      },
      {
        name: "reasoning_effort",
        type: "liste",
        values: ["low", "medium", "high"],
        default: "medium",
        explanation: "Sets how much the model thinks before its answer.",
        source: "chat template",
      },
    ],
    repos: ["Qwen/Qwen3-8B", "Qwen/Qwen3-8B-GGUF"],
    ...over,
  };
}

const FAMILY: SelectionFamily = {
  id: "qwen3",
  label: "Qwen3",
  publisher: "Qwen",
  released: "2026-04-29",
  license: "apache-2.0",
  tool_calling: "native",
  variants: [
    { name: "Qwen3 8B", params_b: 8, gguf_repos: ["Qwen/Qwen3-8B-GGUF"], mlx_repos: ["mlx-community/Qwen3-8B-4bit"] },
  ],
};

function variant(over: Partial<ModelVariant> = {}): ModelVariant {
  return {
    repo_id: "Qwen/Qwen3-8B-GGUF",
    format: "gguf",
    level: "q4",
    quantization: "Q4_K_M",
    origin_file: "Qwen3-8B-Q4_K_M.gguf",
    files: ["Qwen3-8B-Q4_K_M.gguf"],
    size_bytes: 5_000_000_000,
    weights_bytes: 5_000_000_000,
    memory_gb: 6.2,
    compatibility: "fits",
    fully_offloaded: true,
    downloaded: false,
    recommended: false,
    ...over,
  };
}

function installed(over: Partial<InstalledModel> = {}): InstalledModel {
  return {
    id: "qwen3-8b-q4_k_m",
    path: "/Users/someone/.apollia/models/Qwen3-8B-Q4_K_M.gguf",
    name: "Qwen3-8B-Q4_K_M.gguf",
    size_bytes: 5_000_000_000,
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
    source_url: "https://huggingface.co/Qwen/Qwen3-8B-GGUF",
    embedding_entry: null,
    default_chat: false,
    default_search: false,
    ...over,
  };
}

function dictationModel(over: Partial<DictationModel> = {}): DictationModel {
  return {
    id: "whisper-base",
    label: "Whisper Base",
    publisher: "OpenAI",
    released: "2022-09-21",
    license: "mit",
    repo: "ggerganov/whisper.cpp",
    file: "ggml-base.bin",
    quantization: "F16",
    size_bytes: 148_000_000,
    sha256: "aa",
    card: "whisper/base",
    footprint_gb: 1.14,
    fits: true,
    ...over,
  };
}

const DICTATION: DictationModels = {
  models: [
    dictationModel(),
    dictationModel({ id: "whisper-base-q5_1", file: "ggml-base-q5_1.bin", quantization: "Q5_1", size_bytes: 60_000_000, footprint_gb: 1.06 }),
    dictationModel({
      id: "whisper-large-v3",
      label: "Whisper Large v3",
      file: "ggml-large-v3.bin",
      card: "whisper/large-v3",
      size_bytes: 3_100_000_000,
      footprint_gb: 3.9,
      fits: false,
    }),
  ],
  recommended: "whisper-base-q5_1",
};

const DICTATION_CARD = card({
  id: "whisper/base",
  kind: "dictation",
  label: "Whisper Base",
  publisher: "OpenAI",
  description: "Turns speech into text, light and fast.",
  source_url: "https://huggingface.co/openai/whisper-base",
  memory: { min_gb: 1.2, source: "file" },
  author_fields: [],
  defaults: { source: "none" },
  repos: ["ggerganov/whisper.cpp"],
});

function selectionEntry(over: Partial<ModelCardView> = {}) {
  const entry = selectionEntries([card(over)], [FAMILY])[0];
  if (!entry) throw new Error("the selection has one entry");
  return entry;
}

beforeEach(() => {
  invoke.mockReset();
});

describe("the card of a model", () => {
  test("a selection entry carries the description, the memory, the licence and what its author defines", () => {
    // GIVEN the card of a model of the catalogue, read in the language of the interface
    // WHEN the selection builds its row
    const entry = selectionEntry();
    // THEN the card in the centre has everything it shows
    expect(entry.description).toBe("A compact model that reasons and calls tools.");
    expect(entry.minMemoryGb).toBe(8);
    expect(entry.license).toBe("apache-2.0");
    expect(entry.sourceUrl).toBe("https://huggingface.co/Qwen/Qwen3-8B");
    expect(entry.authorFields.map((f) => f.name)).toEqual(["enable_thinking", "reasoning_effort"]);
    expect(entry.defaults?.temperature).toBe(0.6);
  });

  test("an author field reads its name, its type, its default and its explanation", () => {
    // GIVEN a yes-or-no field and a list field of a card
    // WHEN their rows are built
    const rows = authorFieldRows(card().author_fields);
    // THEN each states its type, its values and its default, the boolean by a text of the interface
    expect(rows).toEqual([
      {
        name: "enable_thinking",
        typeKey: "ai.discover.author_fields.type_boolean",
        values: [],
        defaultValue: { key: "ai.discover.author_fields.yes" },
        explanation: "The model thinks before it answers.",
      },
      {
        name: "reasoning_effort",
        typeKey: "ai.discover.author_fields.type_list",
        values: ["low", "medium", "high"],
        defaultValue: { text: "medium" },
        explanation: "Sets how much the model thinks before its answer.",
      },
    ]);
  });

  test("the default parameters are the ones the card states, a neutral penalty reading as off", () => {
    // GIVEN the advised settings of a card, with two penalties at their neutral value
    // WHEN their rows are built in French
    const rows = defaultParameterRows(card().defaults, "fr");
    // THEN min-p, which the card does not state, is absent, and the penalties read as off
    expect(rows.map((r) => [r.id, r.value])).toEqual([
      ["temperature", "0,6"],
      ["top_p", "0,95"],
      ["top_k", "20"],
      ["repetition_penalty", null],
      ["presence_penalty", null],
    ]);
    // AND a card without advised settings shows none
    expect(defaultParameterRows({ source: "none" }, "fr")).toEqual([]);
    expect(defaultParameterRows(null, "fr")).toEqual([]);
  });

  test("a search result inherits the author fields of the card of its model, and keeps its own page", () => {
    // GIVEN a conversion whose generated card names a card of the catalogue
    const found = {
      repo_id: "bartowski/Qwen3-8B-GGUF",
      in_selection: false,
      card: {
        repo_id: "bartowski/Qwen3-8B-GGUF",
        source_url: "https://huggingface.co/bartowski/Qwen3-8B-GGUF",
        owner: "bartowski",
        publisher_kind: "converter",
        origin_repo: "Qwen/Qwen3-8B",
        origin_card_id: "qwen3/Qwen3-8B",
        format: "gguf",
        description: { fr: "Conversion de Qwen3 8B.", en: "A conversion of Qwen3 8B." },
        parameters: 8_190_000_000,
        architecture: "qwen3",
        context_length: 32768,
        capabilities: { tools: true, reasoning: false, vision: false },
        license: "apache-2.0",
        variants: [],
        size_bytes: null,
        last_modified: null,
      },
    } as unknown as FoundModel;
    // WHEN the rows of the search are built with the cards of the catalogue, and without
    const withCard = searchEntries([found], "fr", [card()])[0];
    const without = searchEntries([found], "fr")[0];
    // THEN the fields come from the card, the page stays the repository of the result
    expect(withCard?.authorFields).toHaveLength(2);
    expect(withCard?.sourceUrl).toBe("https://huggingface.co/bartowski/Qwen3-8B-GGUF");
    expect(without?.authorFields).toEqual([]);
    // AND the README is the one the author of the model wrote
    expect(readmeRepo(withCard!, null)).toBe("Qwen/Qwen3-8B");
  });
});

describe("the download options", () => {
  const variants = [
    variant({ recommended: true }),
    variant({ level: "q8", quantization: "Q8_0", origin_file: "Qwen3-8B-Q8_0.gguf", size_bytes: 8_500_000_000, fully_offloaded: false }),
    variant({ repo_id: "mlx-community/Qwen3-8B-4bit", format: "mlx", quantization: "4bit", origin_file: "", downloaded: true }),
  ];

  test("a row reads its precision in bits for a language model, as published otherwise", () => {
    // GIVEN one variant
    const one = variants[1]!;
    // WHEN its precision is read for a language model and for a model of search by meaning
    // THEN the first is said in bits, the second as the registry states it
    expect(precisionLabel(one, "language")).toEqual({ kind: "bits", bits: 8 });
    expect(precisionLabel(one, "embeddings")).toEqual({ kind: "published", text: "Q8_0" });
  });

  test("use default goes back to the recommended variant, whatever was picked", () => {
    // GIVEN the user picked the largest variant
    const picked = chosenVariant(variants, variantKey(variants[1]!));
    // WHEN the choice is given up
    const back = chosenVariant(variants, null);
    // THEN the recommended variant is chosen again
    expect(picked?.quantization).toBe("Q8_0");
    expect(back?.recommended).toBe(true);
  });

  test("the options hold one row per variant with its format, its marks, its size and whether it is downloaded", () => {
    // GIVEN the source of the options
    const src = readFileSync(path.join(HERE, "DiscoverOptions.svelte"), "utf8");
    // WHEN what a row draws is looked for
    // THEN the row has each element of a download option, and the list its two controls
    for (const testid of [
      "ai-discover-options-trigger",
      "ai-discover-options-default",
      "ai-discover-variant-format",
      "ai-discover-variant-pick",
      "ai-discover-variant-offload",
      "ai-discover-variant-recommended",
      "ai-discover-variant-downloaded",
      "ai-discover-variant-size",
    ]) {
      expect(src).toContain(`"${testid}"`);
    }
    expect(src).toContain("<CapabilityIcon {capability} />");
    expect(src).toContain("{#each variants as variant (variantKey(variant))}");
  });

  test.each([
    ["fr", fr, "Choisir une option de téléchargement", "Utiliser le défaut", "Déchargement GPU complet possible", "Téléchargée"],
    ["en", en, "Choose a download option", "Use default", "Full GPU offload possible", "Downloaded"],
  ])("the options are worded in %s", (_loc, catalog, title, useDefault, offload, downloaded) => {
    // GIVEN the catalogue of one language
    // WHEN the texts of the options are read
    // THEN they are the ones the options show
    expect(read(catalog, "ai.discover.options.title")).toBe(title);
    expect(read(catalog, "ai.discover.options.use_default")).toBe(useDefault);
    expect(read(catalog, "ai.discover.offload.label")).toBe(offload);
    expect(read(catalog, "ai.discover.downloaded")).toBe(downloaded);
  });
});

describe("the recommendation and the full offload, explained on hover", () => {
  test.each([
    ["fr", fr, "Recommandée pour votre matériel : bon équilibre entre vitesse et qualité.", /accélér.*empêcher le chargement/s],
    ["en", en, "Recommended for your hardware: a good balance of speed and quality.", /faster.*prevent it from loading/s],
  ])("both are worded in %s", (_loc, catalog, recommended, offload) => {
    // GIVEN the catalogue of one language
    // WHEN the texts the two marks show on hover are read
    // THEN the recommendation reads the sentence decided for it, and the
    // offload says both what it speeds up and that loading can still fail
    expect(read(catalog, "ai.discover.recommended_explanation")).toBe(recommended);
    expect(String(read(catalog, "ai.discover.offload.explanation"))).toMatch(offload);
  });

  test("one variant at most carries the thumb, whatever its format", () => {
    // GIVEN the variants of a card across two formats, as the shell grades them
    const variants = [
      variant({ recommended: false }),
      variant({ repo_id: "mlx-community/Qwen3-8B-4bit", format: "mlx", quantization: "4bit", origin_file: "", recommended: true }),
      variant({ level: "q8", quantization: "Q8_0", origin_file: "Qwen3-8B-Q8_0.gguf" }),
    ];
    // WHEN the default choice is read
    // THEN it is the one the shell recommends, and the options mark that row alone
    expect(chosenVariant(variants, null)?.format).toBe("mlx");
    expect(variants.filter((v) => v.recommended)).toHaveLength(1);
  });
});

describe("the README, read on demand", () => {
  test("the README is asked from the route by its repository, and its metadata block is not shown", async () => {
    // GIVEN the route answers the README of a repository
    invoke.mockResolvedValue({
      repo: "Qwen/Qwen3-8B",
      author: "Qwen",
      license: "apache-2.0",
      source_url: "https://huggingface.co/Qwen/Qwen3-8B",
      text: "---\nlicense: apache-2.0\ntags:\n- chat\n---\n\n# Qwen3 8B\n\nA compact model.\n",
    });
    // WHEN the card asks for it
    const readme = await getModelReadme("Qwen/Qwen3-8B");
    // THEN the command carries the repository, and the text starts at its title
    expect(invoke).toHaveBeenCalledWith("get_model_readme", { repo: "Qwen/Qwen3-8B" });
    expect(readmeBody(readme.text)).toBe("# Qwen3 8B\n\nA compact model.\n");
    // AND a text without a metadata block, or with one never closed, is left as it is
    expect(readmeBody("# Title\n---\nbody")).toBe("# Title\n---\nbody");
    expect(readmeBody("---\nlicense: mit\n# Title")).toBe("---\nlicense: mit\n# Title");
  });

  test("offline, the card shows the message of the route", async () => {
    // GIVEN the route refuses for lack of network, as the shell reports it
    invoke.mockImplementation(() => Promise.reject('API error (503): {"error":"README unavailable offline"}'));
    // WHEN the card asks for the README
    const message = await getModelReadme("Qwen/Qwen3-8B").then(
      () => "",
      (err: unknown) => readmeErrorText(err),
    );
    // THEN the reader gets the words of the route, not the envelope
    expect(message).toBe("README unavailable offline");
    // AND a refusal in plain text is kept whole
    expect(readmeErrorText(new Error("the runtime is unreachable"))).toBe("the runtime is unreachable");
  });

  test("the README is the one of the page of the card, then of the chosen variant", () => {
    // GIVEN a selection entry with a card, and one the catalogue has no card for
    const carded = selectionEntry();
    const uncarded = selectionEntries([], [FAMILY])[0]!;
    // WHEN the repository of the README is read
    // THEN the card names it, and without a card the chosen variant does
    expect(readmeRepo(carded, variant())).toBe("Qwen/Qwen3-8B");
    expect(readmeRepo(uncarded, variant())).toBe("Qwen/Qwen3-8B-GGUF");
    expect(readmeRepo(uncarded, null)).toBeNull();
    expect(repoOfSourceUrl("https://example.invalid/Qwen/Qwen3-8B")).toBeNull();
  });

  test("nothing of the README is kept: no store, no storage, one instance per repository", () => {
    // GIVEN the sources of the README and of the card that mounts it
    const readme = readFileSync(path.join(HERE, "DiscoverReadme.svelte"), "utf8");
    const cardSource = readFileSync(path.join(HERE, "DiscoverCard.svelte"), "utf8");
    // WHEN they are searched for a place that would outlive the display
    // THEN the text lives in the state of the component alone, which hiding empties
    expect(readme).not.toMatch(/localStorage|sessionStorage|svelte\/store|\$lib\/stores/);
    expect(readme).toMatch(/function drop\(\): void \{[^}]*readme = null;/s);
    // AND the card mounts a new instance for another repository, so the text of one never shows under another
    expect(cardSource).toMatch(/\{#key readmeRepo\}\s*<DiscoverReadme repo=\{readmeRepo\} \/>/);
  });
});

describe("see the source", () => {
  test("the page is the one the card states, else the repository of the chosen variant", () => {
    // GIVEN a carded entry and an uncarded one
    const carded = selectionEntry();
    const uncarded = selectionEntries([], [FAMILY])[0]!;
    // WHEN the page of the source is read
    // THEN the card wins, and a repository gives its page on the Hub
    expect(sourcePage(carded, variant())).toBe("https://huggingface.co/Qwen/Qwen3-8B");
    expect(sourcePage(uncarded, variant({ repo_id: "mlx-community/Qwen3-8B-4bit" }))).toBe(
      "https://huggingface.co/mlx-community/Qwen3-8B-4bit",
    );
  });

  test("the card opens it through the opener of the system, never a link of the webview", () => {
    // GIVEN the sources of the window and of the card
    const windowSource = readFileSync(path.join(HERE, "DiscoverWindow.svelte"), "utf8");
    const cardSource = readFileSync(path.join(HERE, "DiscoverCard.svelte"), "utf8");
    // WHEN the gesture is followed
    // THEN the button calls the window, which calls the opener module
    expect(cardSource).toContain('data-testid="ai-discover-card-source"');
    expect(windowSource).toContain("openExternalUrl(sourceUrl)");
    expect(cardSource).not.toMatch(/<a\s/);
  });
});

describe("use in a new conversation", () => {
  test("a downloaded variant opens a conversation on the model of the index", () => {
    // GIVEN the chosen variant is on this computer, under an identifier of the index
    const entry = selectionEntry();
    const chosen = variant({ downloaded: true });
    // WHEN the action is read
    const action = conversationAction(entry, chosen, [installed()]);
    // THEN a conversation is ready on that identifier, the one the selector routes by
    expect(action).toEqual({ kind: "ready", route: "qwen3-8b-q4_k_m" });
    expect(
      startRequest({
        kind: "model",
        route: "qwen3-8b-q4_k_m",
        label: "qwen3-8b-q4_k_m",
        source: "local",
        format: "gguf",
        server: null,
        parameters: null,
        path: null,
      }),
    ).toEqual({ mode: "libre", model: "qwen3-8b-q4_k_m" });
  });

  test("a folder of weights is matched by the repository it was downloaded from", () => {
    // GIVEN an MLX variant, and the model of the index downloaded from its repository
    const chosen = variant({ repo_id: "mlx-community/Qwen3-8B-4bit", format: "mlx", origin_file: "", downloaded: true });
    const model = installed({
      id: "qwen3-8b-4bit",
      format: "mlx",
      path: "/Users/someone/.apollia/models/Qwen3-8B-4bit",
      source_url: "https://huggingface.co/mlx-community/Qwen3-8B-4bit",
    });
    // WHEN the model the variant became is looked for among both formats
    // THEN the folder is found, and the file of another format is not taken for it
    expect(installedMatch(chosen, [installed(), model])?.id).toBe("qwen3-8b-4bit");
  });

  test("otherwise the button is inactive, with the first reason that holds", () => {
    // GIVEN a language model whose variant is absent, one present but not yet listed,
    // and a dictation model
    const entry = selectionEntry();
    const voice = dictationEntries(DICTATION, [DICTATION_CARD])[0]!;
    // WHEN the action is read for each
    // THEN each is blocked with its own reason
    expect(conversationAction(entry, variant(), [installed()])).toEqual({
      kind: "blocked",
      reasonKey: "ai.discover.conversation.reason.not_downloaded",
    });
    expect(conversationAction(entry, null, [])).toEqual({
      kind: "blocked",
      reasonKey: "ai.discover.conversation.reason.not_downloaded",
    });
    expect(conversationAction(entry, variant({ downloaded: true }), [])).toEqual({
      kind: "blocked",
      reasonKey: "ai.discover.conversation.reason.not_listed",
    });
    expect(conversationAction(voice, variant({ downloaded: true }), [installed()])).toEqual({
      kind: "blocked",
      reasonKey: "ai.discover.conversation.reason.not_language",
    });
  });

  test.each([
    ["fr", fr, "Utiliser dans une nouvelle conversation"],
    ["en", en, "Use in a new conversation"],
  ])("the button and its three reasons are worded in %s", (_loc, catalog, label) => {
    // GIVEN the catalogue of one language
    // WHEN the texts of the button are read
    const reasons = ["not_downloaded", "not_listed", "not_language"].map((r) =>
      read(catalog, `ai.discover.conversation.reason.${r}`),
    );
    // THEN the label is the one decided and no reason is missing
    expect(read(catalog, "ai.discover.conversation.use")).toBe(label);
    expect(reasons.every((r) => typeof r === "string" && r.trim() !== "")).toBe(true);
  });
});

describe("the three categories", () => {
  test("the window filters language models, embeddings and dictation, each with its name", () => {
    // GIVEN the categories of the window
    // WHEN their chips are read in both languages
    const names = DISCOVER_CATEGORIES.map((c) => [c, read(fr, discoverCategoryKey(c)), read(en, discoverCategoryKey(c))]);
    // THEN the three are offered, in that order
    expect(names).toEqual([
      ["language", "Modèles de langage", "Language models"],
      ["embeddings", "Recherche par le sens", "Search by meaning"],
      ["dictation", "Dictée", "Dictation"],
    ]);
  });

  test("the dictation models are read from the shell, one row per model, the one for this computer first", async () => {
    // GIVEN the shell answers the dictation models of the registry
    invoke.mockResolvedValue(DICTATION);
    // WHEN the window reads them and builds its rows
    const answer = await getDictationModels();
    const entries = dictationEntries(answer, [DICTATION_CARD]);
    // THEN the command is the one of the registry, and the files of one model make one row
    expect(invoke).toHaveBeenCalledWith("get_dictation_models");
    expect(entries.map((e) => [e.name, e.dictation.length, e.recommendedHere])).toEqual([
      ["Whisper Base", 2, true],
      ["Whisper Large v3", 1, false],
    ]);
    // AND a row with a card reads its description, one without keeps the facts of the registry
    expect(entries[0]?.description).toBe("Turns speech into text, light and fast.");
    expect(entries[1]?.description).toBe("");
    expect(entries[1]?.publisher).toBe("OpenAI");
    expect(dictationEntries(null, [])).toEqual([]);
  });

  test("a dictation file is a variant with the verdict of the registry, downloaded when the index holds it", () => {
    // GIVEN the files of a dictation model, one of them in the index as a dictation model
    const onDisk = installed({
      id: "ggml-base-q5_1",
      kind: "stt",
      category: "dictation",
      path: "C:\\Users\\someone\\.apollia\\models\\ggml-base-q5_1.bin",
    });
    // WHEN their variants are built
    const [full, quantized] = DICTATION.models.slice(0, 2).map((m) => dictationVariant(m, DICTATION.recommended, [onDisk]));
    const large = dictationVariant(DICTATION.models[2]!, DICTATION.recommended, [onDisk]);
    // THEN the recommended file is marked, the one on disk is downloaded, the large one is too large
    expect([full?.recommended, quantized?.recommended]).toEqual([false, true]);
    expect([full?.downloaded, quantized?.downloaded]).toEqual([false, true]);
    expect(large.compatibility).toBe("too_large");
    expect(downloadAction(large, { offline: false, started: new Set() })).toBe("confirm");
    // AND a language model of the same file name is not taken for it
    expect(dictationVariant(DICTATION.models[1]!, null, [installed({ path: "/m/ggml-base-q5_1.bin" })]).downloaded).toBe(false);
  });

  test("the card of a dictation model says what it is for, in place of capabilities", () => {
    // GIVEN the three categories
    // WHEN the purpose and the domain of each card are read
    // THEN a language model has capabilities and no purpose, the two others say what they are for
    expect(purposeKey("language")).toBeNull();
    expect(read(fr, purposeKey("dictation")!)).toBe(
      "Ce modèle transcrit votre voix en texte pour la dictée. Il ne converse pas.",
    );
    expect(read(en, purposeKey("dictation")!)).toBe(
      "This model turns your voice into text for dictation. It does not converse.",
    );
    expect(read(fr, purposeKey("embeddings")!)).toBe(read(fr, "ai.kind.embeddings_explanation"));
    expect(read(fr, domainKey("dictation"))).toBe("Dictée");
    // AND its rows state no storage format, which the filter of formats does not cover
    expect(showsFormat("dictation")).toBe(false);
    expect(showsFormat("language")).toBe(true);
    // AND the card draws capabilities only when there is no purpose to state
    const cardSource = readFileSync(path.join(HERE, "DiscoverCard.svelte"), "utf8");
    expect(cardSource).toMatch(/\{#if purpose\}[\s\S]*\{:else if capabilities\.length > 0\}/);
  });
});
