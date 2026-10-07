import { describe, expect, test } from "vitest";
import { readFileSync, readdirSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import fr from "$lib/i18n/fr.json";
import en from "$lib/i18n/en.json";
import type { EmbeddingsRecommendation } from "$lib/ipc/embeddings";
import type { FoundModel, HfModelCard, ModelCardView, ModelVariant } from "$lib/ipc/modelHub";
import type { SelectionFamily } from "$lib/ipc/models";
import {
  SEARCH_SORTS,
  chosenVariant,
  detailFromFound,
  detailFromRepos,
  downloadAction,
  embeddingEntries,
  embeddingVariant,
  formatsFor,
  isOfflineError,
  isSearching,
  levelBits,
  parameterLabel,
  publisherKindKeys,
  searchEntries,
  selectionEntries,
  shownSelection,
  sortLabelKey,
  variantKey,
  variantSelector,
  verdictKeys,
} from "./discover";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const SRC = path.resolve(HERE, "../..");

function card(over: Partial<ModelCardView> = {}): ModelCardView {
  return {
    id: "qwen3/Qwen3-8B",
    kind: "language",
    label: "Qwen3 8B",
    publisher: "Qwen",
    language: "fr",
    description: "A compact model that reasons.",
    license: { id: "apache-2.0", source: "https://example.invalid/license" },
    source_url: "https://huggingface.co/Qwen/Qwen3-8B",
    capabilities: { tools: true, reasoning: true, vision: false, source: "template" },
    memory: { min_gb: 8, source: "estimate" },
    defaults: { source: "card" },
    author_fields: [],
    repos: ["Qwen/Qwen3-8B", "Qwen/Qwen3-8B-GGUF"],
    ...over,
  };
}

function family(over: Partial<SelectionFamily> = {}): SelectionFamily {
  return {
    id: "qwen3",
    label: "Qwen3",
    publisher: "Qwen",
    released: "2026-04-29",
    license: "apache-2.0",
    tool_calling: "native",
    superseded: false,
    variants: [
      { name: "Qwen3 8B", params_b: 8, gguf_repos: ["Qwen/Qwen3-8B-GGUF"], mlx_repos: ["mlx-community/Qwen3-8B-4bit"] },
      { name: "Qwen3 30B", params_b: 30, gguf_repos: ["Qwen/Qwen3-30B-GGUF"], mlx_repos: [] },
    ],
    ...over,
  };
}

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
    downloaded: false,
    recommended: false,
    ...over,
  };
}

function repoAnswer(over: Partial<HfModelCard> = {}): HfModelCard {
  return {
    repo_id: "Qwen/Qwen3-8B-GGUF",
    author: "Qwen",
    publisher_kind: "official",
    downloads: 872_724,
    likes: 127,
    last_modified: "2026-08-20T10:00:00Z",
    license: "apache-2.0",
    tags: [],
    gated: false,
    total_size_bytes: null,
    gguf_files: [],
    generation_config: null,
    compatibility_issue: null,
    model_type: "qwen3",
    capabilities: { tools: true, reasoning: true, vision: false },
    variants: [variant({ recommended: true }), variant({ level: "q8", origin_file: "Qwen3-8B-Q8_0.gguf" })],
    ...over,
  };
}

function found(over: Partial<FoundModel> = {}): FoundModel {
  return {
    ...repoAnswer({ repo_id: "bartowski/Qwen3-8B-GGUF", author: "bartowski", publisher_kind: "converter", variants: [] }),
    in_selection: false,
    card: {
      repo_id: "bartowski/Qwen3-8B-GGUF",
      source_url: "https://huggingface.co/bartowski/Qwen3-8B-GGUF",
      owner: "bartowski",
      publisher_kind: "converter",
      origin_repo: "Qwen/Qwen3-8B",
      origin_card_id: null,
      format: "gguf",
      description: { fr: "Conversion de Qwen3 8B.", en: "A conversion of Qwen3 8B." },
      parameters: 8_190_000_000,
      architecture: "qwen3",
      context_length: 32768,
      capabilities: { tools: true, reasoning: false, vision: false },
      license: "apache-2.0",
      variants: [variant({ repo_id: "bartowski/Qwen3-8B-GGUF", recommended: true })],
      size_bytes: 5_000_000_000,
      last_modified: "2026-09-01T00:00:00Z",
    },
    ...over,
  };
}

const EMBEDDINGS: EmbeddingsRecommendation = {
  models: [
    {
      id: "bge-m3",
      label: "BGE M3",
      publisher: "BAAI",
      license: "mit",
      repo: "gpustack/bge-m3-GGUF",
      file: "bge-m3-Q8_0.gguf",
      quantization: "Q8_0",
      size_bytes: 635_000_000,
      sha256: "aa",
      dimensions: 1024,
      context_length: 8192,
    },
    {
      id: "multilingual-e5-small",
      label: "Multilingual E5 Small",
      publisher: "intfloat",
      license: "mit",
      repo: "cstr/multilingual-e5-small-GGUF",
      file: "multilingual-e5-small-q8_0.gguf",
      quantization: "Q8_0",
      size_bytes: 130_000_000,
      sha256: "bb",
      dimensions: 384,
      context_length: 512,
    },
  ],
  recommended: "multilingual-e5-small",
  reason: null,
};

describe("the selection the window opens on", () => {
  test("a card and the shipped selection describe one row together", () => {
    // GIVEN a card of the catalogue and the selection the application ships
    // WHEN the selection is listed
    const entries = selectionEntries([card()], [family()]);
    // THEN the size the card describes takes its text from the card, its date
    // and its parameter count from the selection, and the other size stands alone
    expect(entries.map((e) => e.name)).toEqual(["Qwen3 8B", "Qwen3 30B"]);
    expect(entries[0]).toMatchObject({
      description: "A compact model that reasons.",
      updatedAt: "2026-04-29",
      parameters: 8_000_000_000,
      minMemoryGb: 8,
      inSelection: true,
    });
    expect(entries[0]?.repos).toContain("mlx-community/Qwen3-8B-4bit");
    expect(entries[1]?.description).toBe("");
  });

  test("without any card the shipped selection is still listed", () => {
    // GIVEN no card, as when the registry was never reached
    // WHEN the selection is listed
    const entries = selectionEntries([], [family(), family({ id: "old", superseded: true })]);
    // THEN every current size of the shipped selection is there
    expect(entries.map((e) => e.key)).toEqual(["selection:qwen3/Qwen3 8B", "selection:qwen3/Qwen3 30B"]);
  });

  test("an embeddings card is not a language model", () => {
    // GIVEN a card of each kind
    const cards = [card(), card({ id: "bge-m3", kind: "embeddings", label: "BGE M3", repos: ["gpustack/bge-m3-GGUF"] })];
    // WHEN the language selection is listed
    const entries = selectionEntries(cards, []);
    // THEN the embeddings card is left to its own category
    expect(entries.map((e) => e.name)).toEqual(["Qwen3 8B"]);
  });

  test("the box keeps what fits the capacity, and leaves out what nobody sized", () => {
    // GIVEN three entries: one small, one large, one of unknown memory
    const entries = selectionEntries(
      [card(), card({ id: "big", label: "Big", memory: { min_gb: 64, source: "" }, repos: ["x/big"] })],
      [family()],
    );
    // WHEN only what fits 21.3 GB is asked
    const shown = shownSelection(entries, { fitsOnly: true, capacityGb: 21.3, sort: "best_match" });
    // THEN the small one alone stays
    expect(shown.map((e) => e.name)).toEqual(["Qwen3 8B"]);
    // AND without the box every entry stays, in the order of the selection
    expect(shownSelection(entries, { fitsOnly: false, capacityGb: 21.3, sort: "best_match" })).toHaveLength(3);
  });
});

describe("the models of search by meaning", () => {
  test("the model for this computer comes first and is marked", () => {
    // GIVEN the registry entries, the second recommended here
    // WHEN they are listed
    const entries = embeddingEntries(EMBEDDINGS, [
      card({ id: "bge-m3", kind: "embeddings", description: "Searches in a hundred languages.", repos: [] }),
    ]);
    // THEN the recommended one leads, marked, and each is of the embeddings category
    expect(entries.map((e) => [e.name, e.recommendedHere])).toEqual([
      ["Multilingual E5 Small", true],
      ["BGE M3", false],
    ]);
    expect(entries.every((e) => e.category === "embeddings" && e.capabilities === null)).toBe(true);
    expect(entries[1]?.description).toBe("Searches in a hundred languages.");
  });

  test("its one variant is the file the registry pins", () => {
    // GIVEN a registry entry already on this computer
    const model = EMBEDDINGS.models[0];
    if (!model) throw new Error("fixture");
    // WHEN its variant is built
    const v = embeddingVariant(model, true);
    // THEN the download names that file of that repository
    expect(variantSelector(v)).toEqual({ kind: "gguf", file: "bge-m3-Q8_0.gguf" });
    expect(v).toMatchObject({ repo_id: "gpustack/bge-m3-GGUF", downloaded: true, size_bytes: 635_000_000 });
  });
});

describe("the results of a search", () => {
  test("a result reads its facts from the generated card, in the language asked", () => {
    // GIVEN a result of a recognised converter
    // WHEN it is listed in French and in English
    const [inFrench] = searchEntries([found()], "fr");
    const [inEnglish] = searchEntries([found()], "en-US");
    // THEN the name is the repository's, the owner and its sort are the card's
    expect(inFrench).toMatchObject({
      name: "Qwen3-8B-GGUF",
      identifier: "bartowski/Qwen3-8B-GGUF",
      publisher: "bartowski",
      publisherKind: "converter",
      description: "Conversion de Qwen3 8B.",
      parameters: 8_190_000_000,
    });
    expect(inEnglish?.description).toBe("A conversion of Qwen3 8B.");
  });

  test("the sort of publisher has a name and an explanation, in both languages", () => {
    // GIVEN the two sorts of publisher and a repository of neither
    // WHEN their keys are read
    const keys = (["official", "converter"] as const).flatMap((k) => {
      const pair = publisherKindKeys(k);
      return pair ? [pair.label, pair.explanation] : [];
    });
    // THEN each key has a text in French and in English, and neither sort has none
    expect(keys).toHaveLength(4);
    for (const key of keys) {
      expect(read(fr, key), key).toBeTruthy();
      expect(read(en, key), key).toBeTruthy();
    }
    expect(publisherKindKeys(null)).toBeNull();
    expect(read(en, "ai.discover.publisher.official")).toBe("Official publisher");
    expect(read(en, "ai.discover.publisher.converter")).toBe("Recognized converter");
  });

  test("words in the field start a search, an empty field shows the selection", () => {
    // GIVEN an empty field, spaces, and a word
    // WHEN each is read
    // THEN only the word searches
    expect([isSearching(""), isSearching("   "), isSearching("qwen")]).toEqual([false, false, true]);
  });

  test("the offline refusal of the search is told apart from another error", () => {
    // GIVEN the message of the shell offline, and an upstream error
    // WHEN each is read
    // THEN only the first says the network is cut
    expect(isOfflineError("search is unavailable offline")).toBe(true);
    expect(isOfflineError("huggingface answered with an error: 500")).toBe(false);
  });

  test("the four orders are offered, best match first, each with its text", () => {
    // GIVEN the orders of the sort menu
    // WHEN their texts are read
    // THEN best match leads and each has a name in both languages
    expect(SEARCH_SORTS).toEqual(["best_match", "likes", "downloads", "updated"]);
    for (const sort of SEARCH_SORTS) {
      expect(read(fr, sortLabelKey(sort)), sort).toBeTruthy();
      expect(read(en, sortLabelKey(sort)), sort).toBeTruthy();
    }
  });

  test("MLX is offered on Apple Silicon alone", () => {
    // GIVEN an Apple Silicon machine, a CUDA machine, and an unprobed one
    // WHEN the formats are listed
    // THEN MLX appears on the first only
    expect(formatsFor({ kind: "apple_silicon" })).toEqual(["gguf", "mlx"]);
    expect(formatsFor({ kind: "cuda" })).toEqual(["gguf"]);
    expect(formatsFor(null)).toEqual(["gguf"]);
  });
});

describe("the card and its download", () => {
  test("the detail gathers the variants of the chosen format across repositories", () => {
    // GIVEN the original repository and an MLX conversion
    const mlx = repoAnswer({
      repo_id: "mlx-community/Qwen3-8B-4bit",
      downloads: 10,
      variants: [variant({ repo_id: "mlx-community/Qwen3-8B-4bit", format: "mlx", origin_file: "config.json" })],
    });
    // WHEN the detail is built for each format
    const gguf = detailFromRepos([repoAnswer(), mlx], "gguf");
    const onMlx = detailFromRepos([repoAnswer(), mlx], "mlx");
    // THEN the figures are those of the first repository and the variants follow the format
    expect(gguf).toMatchObject({ downloads: 872_724, likes: 127, architecture: "qwen3", gated: false });
    expect(gguf.variants.map((v) => v.format)).toEqual(["gguf", "gguf"]);
    expect(onMlx.variants.map((v) => v.repo_id)).toEqual(["mlx-community/Qwen3-8B-4bit"]);
  });

  test("a search result brings its own variants", () => {
    // GIVEN a result whose generated card holds one variant
    // WHEN its detail is read
    const detail = detailFromFound(found());
    // THEN no repository has to be asked again
    expect(detail.variants).toHaveLength(1);
    expect(detail.updatedAt).toBe("2026-09-01T00:00:00Z");
  });

  test("the recommended variant is chosen, then the smallest, unless one was picked", () => {
    // GIVEN three variants, the middle one recommended
    const small = variant({ origin_file: "a.gguf", size_bytes: 1 });
    const best = variant({ origin_file: "b.gguf", size_bytes: 2, recommended: true });
    const large = variant({ origin_file: "c.gguf", size_bytes: 3 });
    // WHEN nothing is picked, when none is recommended, and when one is picked
    // THEN the recommended one, the smallest one, and the picked one are chosen
    expect(chosenVariant([large, best, small], null)).toBe(best);
    expect(chosenVariant([large, small], null)).toBe(small);
    expect(chosenVariant([large, best, small], variantKey(large))).toBe(large);
    expect(chosenVariant([], null)).toBeNull();
  });

  test("a variant too large for the machine asks before it downloads", () => {
    // GIVEN a variant that fits and one marked too large, online
    const state = { offline: false, started: new Set<string>() };
    // WHEN the action of each is read
    // THEN the first downloads in one click and the second asks first
    expect(downloadAction(variant(), state)).toBe("download");
    expect(downloadAction(variant({ compatibility: "too_large" }), state)).toBe("confirm");
  });

  test("offline, the download is disabled with its reason, even with no variant read", () => {
    // GIVEN the network cut
    const state = { offline: true, started: new Set<string>() };
    // WHEN the action is read with a variant and without
    // THEN both say offline, and a variant already here still says so
    expect(downloadAction(variant(), state)).toBe("offline");
    expect(downloadAction(null, state)).toBe("offline");
    expect(downloadAction(variant({ downloaded: true }), state)).toBe("downloaded");
    expect(read(fr, "ai.discover.status.offline")).toBeTruthy();
  });

  test("a download started from the window is not started twice", () => {
    // GIVEN a variant whose download just started
    const v = variant();
    // WHEN its action is read
    // THEN it says downloading; with nothing to choose from online, unavailable
    expect(downloadAction(v, { offline: false, started: new Set([variantKey(v)]) })).toBe("downloading");
    expect(downloadAction(null, { offline: false, started: new Set() })).toBe("unavailable");
  });

  test("an MLX variant downloads its whole folder", () => {
    // GIVEN an MLX variant
    // WHEN its selector is built
    // THEN it names the folder, not a file
    expect(variantSelector(variant({ format: "mlx", origin_file: "config.json" }))).toEqual({ kind: "mlx" });
  });

  test("precision is said in bits, and a parameter count in the reader's units", () => {
    // GIVEN the levels a repository offers and two parameter counts
    // WHEN they are read for display
    // THEN the levels are 4 to 16 bits, and the counts are billions or millions
    expect((["q4", "q5", "q6", "q8", "fp16"] as const).map(levelBits)).toEqual([4, 5, 6, 8, 16]);
    expect(parameterLabel(27_000_000_000, "fr")).toEqual({ value: "27", unitKey: "ai.discover.params_billions" });
    expect(parameterLabel(137_000_000, "en")).toEqual({ value: "137", unitKey: "ai.discover.params_millions" });
    expect(parameterLabel(null, "fr")).toBeNull();
  });

  test("every verdict has a name and an explanation in both languages", () => {
    // GIVEN the three verdicts
    // WHEN their keys are read
    for (const verdict of ["fits", "might_fit", "too_large"] as const) {
      const keys = verdictKeys(verdict);
      // THEN each has both texts in both catalogues
      expect(keys).not.toBeNull();
      for (const key of keys ? [keys.label, keys.explanation] : []) {
        expect(read(fr, key), key).toBeTruthy();
        expect(read(en, key), key).toBeTruthy();
      }
    }
    expect(verdictKeys(null)).toBeNull();
  });
});

describe("what the window no longer does", () => {
  const sources = readdirSync(HERE)
    .filter((name) => name.startsWith("Discover") || name === "discover.ts" || name === "discoverCard.ts")
    .map((name) => [name, readFileSync(path.join(HERE, name), "utf8")] as const);

  test("the window is read", () => {
    // GIVEN the sources of the discovery window
    // WHEN they are listed
    // THEN the window, its column, its card with its parts and their decisions are there
    expect(sources.map(([name]) => name).sort()).toEqual([
      "DiscoverCard.svelte",
      "DiscoverColumn.svelte",
      "DiscoverOptions.svelte",
      "DiscoverReadme.svelte",
      "DiscoverSettings.svelte",
      "DiscoverWindow.svelte",
      "discover.ts",
      "discoverCard.ts",
    ]);
  });

  test("nothing is guessed from a file or repository name", () => {
    // GIVEN the sources of the window
    // WHEN they are searched for a pattern that reads a quantisation, vision
    // or reasoning out of a name
    const guesses = sources.filter(([, src]) =>
      /mmproj|includes\("(think|reason|vision|instruct)|Q\d_K|\.gguf\$/i.test(src),
    );
    // THEN none does: those facts come from the shell
    expect(guesses.map(([name]) => name)).toEqual([]);
  });

  test("the filters of language, licence, type and restricted access are gone", () => {
    // GIVEN the sources of the window and both catalogues
    // WHEN the former filters are looked for
    const filters = sources.filter(([, src]) => /licenseFilter|langFilter|modelTypeFilter|gatedFilter/.test(src));
    // THEN no source holds one, and no text of the former screen remains
    expect(filters.map(([name]) => name)).toEqual([]);
    expect(read(fr, "settings.model_hub")).toBeUndefined();
    expect(read(en, "settings.model_hub")).toBeUndefined();
    expect(read(fr, "settings.nav.model_hub")).toBeUndefined();
  });

  test("the settings no longer hold the former screen", () => {
    // GIVEN the settings route, its navigation and its store
    const files = ["routes/Settings.svelte", "lib/settings/settingsNav.ts", "lib/stores/settings.ts"];
    // WHEN each is searched for the former entry
    const holding = files.filter((file) => readFileSync(path.join(SRC, file), "utf8").includes("model-hub"));
    // THEN none names it
    expect(holding).toEqual([]);
  });
});

function read(catalog: unknown, key: string): unknown {
  return key.split(".").reduce<unknown>(
    (node, part) => (node && typeof node === "object" ? (node as Record<string, unknown>)[part] : undefined),
    catalog,
  );
}
