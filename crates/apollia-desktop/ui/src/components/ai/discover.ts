/**
 * The decisions of the discovery window, kept out of its components so they
 * are tested in `node`: which models the left column lists (the Apollia
 * selection, the results of a search, the models of search by meaning), what
 * the card in the centre reads, which variant a download takes and whether it
 * asks first. What the card does once a variant is chosen is in
 * `discoverCard.ts`.
 *
 * Nothing here reads a file name or a repository name to guess what a model
 * is: capabilities, precision and category all come from the answers of the
 * shell. Pure and i18n-agnostic: labels are catalogue keys the caller resolves.
 */
import type { EmbeddingModel, EmbeddingsRecommendation } from "$lib/ipc/embeddings";
import type {
  AcceleratorProfile,
  Compatibility,
  DictationModel,
  DictationModels,
  FoundModel,
  HfModelCard,
  HubCapabilities,
  InstalledModel,
  ModelCardView,
  ModelFormat,
  ModelVariant,
  PublisherKind,
  QuantLevel,
  SearchSort,
  VariantSelector,
} from "$lib/ipc/modelHub";
import type { SelectionFamily } from "$lib/ipc/models";

/** The three kinds of model the window tells apart. */
export type DiscoverCategory = "language" | "embeddings" | "dictation";

/** The categories, in the order of their chips. */
export const DISCOVER_CATEGORIES: readonly DiscoverCategory[] = ["language", "embeddings", "dictation"];

const CATEGORY_KEYS: Record<DiscoverCategory, string> = {
  language: "ai.category.language",
  embeddings: "ai.discover.category_embeddings",
  dictation: "ai.category.dictation",
};

/** The catalogue key of a category chip. */
export function discoverCategoryKey(category: DiscoverCategory): string {
  return CATEGORY_KEYS[category];
}

/** The orders of the sort menu, the default first. */
export const SEARCH_SORTS: readonly SearchSort[] = ["best_match", "likes", "downloads", "updated"];

const SORT_KEYS: Record<SearchSort, string> = {
  best_match: "ai.discover.sort.best_match",
  likes: "ai.discover.sort.likes",
  downloads: "ai.discover.sort.downloads",
  updated: "ai.discover.sort.updated",
};

/** The catalogue key of a sort order. */
export function sortLabelKey(sort: SearchSort): string {
  return SORT_KEYS[sort];
}

/**
 * The formats the filter offers on this machine: MLX folders only run on
 * Apple Silicon, so the choice exists there alone.
 */
export function formatsFor(accelerator: AcceleratorProfile | null | undefined): ModelFormat[] {
  return accelerator?.kind === "apple_silicon" ? ["gguf", "mlx"] : ["gguf"];
}

/** Where a row of the left column comes from. */
export type EntrySource = "selection" | "search" | "embeddings" | "dictation";

/** A setting the author of a model defines, as its card states it. */
export type AuthorField = ModelCardView["author_fields"][number];

/** The request settings the author of a model advises, as its card states them. */
export type CardDefaults = ModelCardView["defaults"];

/** One row of the left column, and what the card in the centre starts from. */
export interface DiscoverEntry {
  /** Stable key of the row. */
  key: string;
  source: EntrySource;
  category: DiscoverCategory;
  /** The name the user reads. */
  name: string;
  /** The identifier the copy button takes: a card id or a repository. */
  identifier: string;
  /** Who published it, in text. */
  publisher: string;
  publisherKind: PublisherKind;
  /** A short description, empty when none is written. */
  description: string;
  capabilities: HubCapabilities | null;
  /** When it was released or last changed, RFC 3339 or a date. */
  updatedAt: string | null;
  /** The entry belongs to the Apollia selection. */
  inSelection: boolean;
  /** The model the machine is recommended for search by meaning. */
  recommendedHere: boolean;
  /** The least memory the model asks, in GB, when a card says it. */
  minMemoryGb: number | null;
  license: string | null;
  /** Parameter count, in units, when known. */
  parameters: number | null;
  architecture: string | null;
  /** Repositories whose variants the card gathers, for a selection entry. */
  repos: string[];
  /** The result of a search, which already carries its variants. */
  found: FoundModel | null;
  /** The registry entry of a model of search by meaning. */
  embedding: EmbeddingModel | null;
  /** The files of a dictation model, one per precision. */
  dictation: DictationModel[];
  /** The page the model comes from, when a card or a result states it. */
  sourceUrl: string | null;
  /** The settings its author defines, each with its default and what it does. */
  authorFields: AuthorField[];
  /** The request settings its author advises; null when no card states them. */
  defaults: CardDefaults | null;
}

function emptyEntry(): Omit<DiscoverEntry, "key" | "source" | "category" | "name" | "identifier"> {
  return {
    publisher: "",
    publisherKind: null,
    description: "",
    capabilities: null,
    updatedAt: null,
    inSelection: false,
    recommendedHere: false,
    minMemoryGb: null,
    license: null,
    parameters: null,
    architecture: null,
    repos: [],
    found: null,
    embedding: null,
    dictation: [],
    sourceUrl: null,
    authorFields: [],
    defaults: null,
  };
}

/** What a row takes from the card that describes its model. */
function fromCard(card: ModelCardView): Pick<DiscoverEntry, "sourceUrl" | "authorFields" | "defaults"> {
  return { sourceUrl: card.source_url, authorFields: card.author_fields, defaults: card.defaults };
}

function sameRepo(a: string, b: string): boolean {
  return a.toLowerCase() === b.toLowerCase();
}

function sharesRepo(a: readonly string[], b: readonly string[]): boolean {
  return a.some((x) => b.some((y) => sameRepo(x, y)));
}

interface SelectionSize {
  family: SelectionFamily;
  name: string;
  paramsB: number;
  repos: string[];
}

function selectionSizes(families: readonly SelectionFamily[]): SelectionSize[] {
  return families
    .filter((family) => !family.superseded)
    .flatMap((family) =>
      family.variants.map((variant) => ({
        family,
        name: variant.name,
        paramsB: variant.params_b,
        repos: [...variant.gguf_repos, ...variant.mlx_repos],
      })),
    );
}

/**
 * The language models of the Apollia selection. A card of the catalogue gives
 * a row its description, its capabilities and its memory; the selection the
 * application ships gives it its release date and its size, and stands alone
 * when no card could be read, so the list never empties with the network.
 */
export function selectionEntries(
  cards: readonly ModelCardView[],
  families: readonly SelectionFamily[],
): DiscoverEntry[] {
  const sizes = selectionSizes(families);
  const language = cards.filter((card) => card.kind === "language");
  const fromCards = language.map((card): DiscoverEntry => {
    const size = sizes.find((s) => sharesRepo(s.repos, card.repos));
    return {
      ...emptyEntry(),
      key: `selection:${card.id}`,
      source: "selection",
      category: "language",
      name: card.label,
      identifier: card.id,
      publisher: card.publisher,
      description: card.description,
      capabilities: {
        tools: card.capabilities.tools,
        reasoning: card.capabilities.reasoning,
        vision: card.capabilities.vision,
      },
      updatedAt: size?.family.released ?? null,
      inSelection: true,
      minMemoryGb: card.memory.min_gb,
      license: card.license.id,
      parameters: size ? Math.round(size.paramsB * 1e9) : null,
      repos: size ? mergeRepos(size.repos, card.repos) : [...card.repos],
      ...fromCard(card),
    };
  });
  const uncarded = sizes
    .filter((size) => !language.some((card) => sharesRepo(size.repos, card.repos)))
    .map(
      (size): DiscoverEntry => ({
        ...emptyEntry(),
        key: `selection:${size.family.id}/${size.name}`,
        source: "selection",
        category: "language",
        name: size.name,
        identifier: size.repos[0] ?? size.name,
        publisher: size.family.publisher,
        updatedAt: size.family.released,
        inSelection: true,
        license: size.family.license,
        parameters: Math.round(size.paramsB * 1e9),
        repos: size.repos,
      }),
    );
  return [...fromCards, ...uncarded];
}

function mergeRepos(first: readonly string[], second: readonly string[]): string[] {
  const out = [...first];
  for (const repo of second) {
    if (!out.some((r) => sameRepo(r, repo))) out.push(repo);
  }
  return out;
}

/** The text of `text` in the language of the interface, English otherwise. */
export function localized(text: { fr: string; en: string }, locale: string): string {
  const french = locale.toLowerCase().startsWith("fr") && text.fr.trim() !== "";
  return french ? text.fr : text.en;
}

/**
 * The rows of a search, in the order the search answered. A result whose
 * model has a card of the catalogue takes its author fields and its advised
 * settings from it; its page stays the one of its own repository.
 */
export function searchEntries(
  models: readonly FoundModel[],
  locale: string,
  cards: readonly ModelCardView[] = [],
): DiscoverEntry[] {
  return models.map((model): DiscoverEntry => {
    const card = model.card;
    const origin = card.origin_card_id ? cards.find((c) => c.id === card.origin_card_id) : undefined;
    return {
      ...emptyEntry(),
      key: `search:${model.repo_id}`,
      source: "search",
      category: "language",
      name: model.repo_id.split("/").pop() ?? model.repo_id,
      identifier: model.repo_id,
      publisher: card.owner,
      publisherKind: card.publisher_kind,
      description: localized(card.description, locale),
      capabilities: card.capabilities,
      updatedAt: card.last_modified ?? model.last_modified ?? null,
      inSelection: model.in_selection,
      license: card.license,
      parameters: card.parameters,
      architecture: card.architecture,
      found: model,
      ...(origin ? fromCard(origin) : {}),
      sourceUrl: card.source_url,
    };
  });
}

/**
 * The models of search by meaning: the entries of the registry, the one for
 * this machine first and marked as such.
 */
export function embeddingEntries(
  recommendation: EmbeddingsRecommendation | null,
  cards: readonly ModelCardView[],
): DiscoverEntry[] {
  if (!recommendation) return [];
  const entries = recommendation.models.map((model): DiscoverEntry => {
    const card = cards.find(
      (c) => c.kind === "embeddings" && (c.id === model.id || c.repos.some((r) => sameRepo(r, model.repo))),
    );
    return {
      ...emptyEntry(),
      key: `embeddings:${model.id}`,
      source: "embeddings",
      category: "embeddings",
      name: model.label,
      identifier: model.id,
      publisher: model.publisher,
      description: card?.description ?? "",
      inSelection: true,
      recommendedHere: recommendation.recommended === model.id,
      minMemoryGb: card?.memory.min_gb ?? null,
      license: model.license,
      embedding: model,
      sourceUrl: card?.source_url ?? null,
    };
  });
  return [...entries].sort((a, b) => Number(b.recommendedHere) - Number(a.recommendedHere));
}

/**
 * The dictation models: one row per model, which gathers its files, the model
 * whose file is recommended for this machine first and marked as such. The
 * registry orders the files; a row keeps the order it found them in.
 */
export function dictationEntries(
  dictation: DictationModels | null,
  cards: readonly ModelCardView[],
): DiscoverEntry[] {
  if (!dictation) return [];
  const groups = new Map<string, DictationModel[]>();
  for (const model of dictation.models) {
    groups.set(model.card, [...(groups.get(model.card) ?? []), model]);
  }
  const entries = [...groups.entries()].map(([cardId, files]): DiscoverEntry => {
    const first = files[0] as DictationModel;
    const card = cards.find((c) => c.kind === "dictation" && c.id === cardId);
    return {
      ...emptyEntry(),
      key: `dictation:${cardId}`,
      source: "dictation",
      category: "dictation",
      name: card?.label ?? first.label,
      identifier: cardId,
      publisher: card?.publisher ?? first.publisher,
      description: card?.description ?? "",
      updatedAt: first.released,
      inSelection: true,
      recommendedHere: files.some((file) => file.id === dictation.recommended),
      minMemoryGb: card?.memory.min_gb ?? null,
      license: card?.license.id ?? first.license,
      dictation: files,
      sourceUrl: card?.source_url ?? null,
    };
  });
  return [...entries].sort((a, b) => Number(b.recommendedHere) - Number(a.recommendedHere));
}

/** The last segment of a path, whichever separator the system writes. */
export function fileName(path: string): string {
  const parts = path.split(/[\\/]/);
  return parts[parts.length - 1] ?? path;
}

/**
 * One file of a dictation model as a variant of its card: its footprint and
 * its verdict come from the registry, and it is on this computer when a
 * dictation model of the index carries its file. Its `level` is never read:
 * the row shows the quantisation as published.
 */
export function dictationVariant(
  model: DictationModel,
  recommended: string | null,
  installed: readonly InstalledModel[],
): ModelVariant {
  return {
    repo_id: model.repo,
    format: "gguf",
    level: "q8",
    quantization: model.quantization,
    origin_file: model.file,
    files: [model.file],
    size_bytes: model.size_bytes,
    weights_bytes: model.size_bytes,
    sha256: model.sha256,
    memory_gb: model.footprint_gb,
    compatibility: model.fits ? "fits" : "too_large",
    downloaded: installed.some((m) => m.category === "dictation" && fileName(m.path) === model.file),
    recommended: model.id === recommended,
  };
}

/**
 * The rows the selection shows: the chosen category, the entries that fit the
 * machine alone when asked, the most recent first under that sort. An entry
 * whose memory nobody states is left out of "fits", since nothing says it does.
 */
export function shownSelection(
  entries: readonly DiscoverEntry[],
  options: { fitsOnly: boolean; capacityGb: number | null; sort: SearchSort },
): DiscoverEntry[] {
  const { fitsOnly, capacityGb, sort } = options;
  const kept =
    fitsOnly && capacityGb !== null
      ? entries.filter((e) => e.minMemoryGb !== null && e.minMemoryGb <= capacityGb)
      : [...entries];
  if (sort !== "updated") return kept;
  return [...kept].sort((a, b) => (b.updatedAt ?? "").localeCompare(a.updatedAt ?? ""));
}

/** Whether the words typed start a search rather than show the selection. */
export function isSearching(query: string): boolean {
  return query.trim().length > 0;
}

/** Whether an error of the search says the network is unreachable. */
export function isOfflineError(message: string): boolean {
  return /offline|unreachable/i.test(message);
}

const PUBLISHER_KEYS: Record<"official" | "converter", { label: string; explanation: string }> = {
  official: {
    label: "ai.discover.publisher.official",
    explanation: "ai.discover.publisher.official_explanation",
  },
  converter: {
    label: "ai.discover.publisher.converter",
    explanation: "ai.discover.publisher.converter_explanation",
  },
};

/** The catalogue keys of the sort of publisher, `null` when it is neither. */
export function publisherKindKeys(
  kind: PublisherKind,
): { label: string; explanation: string } | null {
  return kind ? PUBLISHER_KEYS[kind] : null;
}

const LEVEL_BITS: Record<QuantLevel, number> = { q4: 4, q5: 5, q6: 6, q8: 8, fp16: 16 };

/** The precision of a variant in bits per weight, the way the window says it. */
export function levelBits(level: QuantLevel): number {
  return LEVEL_BITS[level];
}

/** A parameter count split for display: `27` and the key of "billions". */
export interface ParameterLabel {
  value: string;
  unitKey: "ai.discover.params_billions" | "ai.discover.params_millions";
}

/** A parameter count in the reader's locale, `null` when unknown. */
export function parameterLabel(parameters: number | null, locale: string): ParameterLabel | null {
  if (parameters === null || !Number.isFinite(parameters) || parameters <= 0) return null;
  const billions = parameters >= 1e9;
  const value = new Intl.NumberFormat(locale, { maximumFractionDigits: 1 }).format(
    parameters / (billions ? 1e9 : 1e6),
  );
  return { value, unitKey: billions ? "ai.discover.params_billions" : "ai.discover.params_millions" };
}

/** A count in the reader's locale (`872 724`). */
export function countLabel(n: number, locale: string): string {
  return new Intl.NumberFormat(locale).format(n);
}

/** What the card in the centre reads once the repositories have answered. */
export interface CardDetail {
  downloads: number | null;
  likes: number | null;
  updatedAt: string | null;
  architecture: string | null;
  capabilities: HubCapabilities | null;
  /** The repository is restricted: its download needs the stored token. */
  gated: boolean;
  variants: ModelVariant[];
}

/**
 * The detail of an entry from the repositories that answered, in the order
 * they were asked: the figures of the first one, every variant of the format
 * the filter keeps.
 */
export function detailFromRepos(
  answers: readonly HfModelCard[],
  format: ModelFormat,
): CardDetail {
  const first = answers[0];
  const withCapabilities = answers.find((a) => a.capabilities);
  return {
    downloads: first?.downloads ?? null,
    likes: first?.likes ?? null,
    updatedAt: first?.last_modified ?? null,
    architecture: answers.find((a) => a.model_type)?.model_type ?? null,
    capabilities: withCapabilities?.capabilities ?? null,
    gated: answers.some((a) => a.gated),
    variants: answers.flatMap((a) => a.variants ?? []).filter((v) => v.format === format),
  };
}

/** The detail of a search result, which its generated card already holds. */
export function detailFromFound(found: FoundModel): CardDetail {
  return {
    downloads: found.downloads,
    likes: found.likes,
    updatedAt: found.card.last_modified ?? found.last_modified ?? null,
    architecture: found.card.architecture,
    capabilities: found.card.capabilities,
    gated: found.gated,
    variants: found.card.variants,
  };
}

/**
 * The one variant of a model of search by meaning: the file the registry
 * pins. Its `level` is never read: the row shows the name of the model, and
 * the registry states the quantisation as published.
 */
export function embeddingVariant(model: EmbeddingModel, downloaded: boolean): ModelVariant {
  return {
    repo_id: model.repo,
    format: "gguf",
    level: "q8",
    quantization: model.quantization,
    origin_file: model.file,
    files: [model.file],
    size_bytes: model.size_bytes,
    weights_bytes: model.size_bytes,
    sha256: model.sha256,
    memory_gb: null,
    compatibility: "fits",
    downloaded,
    recommended: true,
  };
}

/** The key of a variant among those of one card. */
export function variantKey(variant: ModelVariant): string {
  return `${variant.repo_id}:${variant.format}:${variant.origin_file}`;
}

/**
 * The variant the card acts on: the one the user picked, else the one
 * recommended for this machine, else the smallest.
 */
export function chosenVariant(
  variants: readonly ModelVariant[],
  pickedKey: string | null,
): ModelVariant | null {
  if (variants.length === 0) return null;
  const picked = pickedKey ? variants.find((v) => variantKey(v) === pickedKey) : undefined;
  if (picked) return picked;
  const recommended = variants.find((v) => v.recommended);
  if (recommended) return recommended;
  return [...variants].sort((a, b) => a.size_bytes - b.size_bytes)[0] ?? null;
}

/** What the download takes as its selector: a weights file, or the MLX folder. */
export function variantSelector(variant: ModelVariant): VariantSelector {
  return variant.format === "mlx" ? { kind: "mlx" } : { kind: "gguf", file: variant.origin_file };
}

/** What the action under the chosen variant does. */
export type DownloadAction =
  /** Nothing to choose from: the repositories could not be read. */
  | "unavailable"
  /** Already on this computer. */
  | "downloaded"
  /** A download of this variant started from this window. */
  | "downloading"
  /** The network is cut: the button is there, disabled, with its reason. */
  | "offline"
  /** Too large for this machine: the download asks first. */
  | "confirm"
  | "download";

/** The action of the chosen variant, the first reason that holds. */
export function downloadAction(
  variant: ModelVariant | null,
  state: { offline: boolean; started: ReadonlySet<string> },
): DownloadAction {
  if (!variant) return state.offline ? "offline" : "unavailable";
  if (variant.downloaded) return "downloaded";
  if (state.started.has(variantKey(variant))) return "downloading";
  if (state.offline) return "offline";
  return variant.compatibility === "too_large" ? "confirm" : "download";
}

const VERDICT_KEYS: Record<"fits" | "might_fit" | "too_large", { label: string; explanation: string }> = {
  fits: { label: "ai.discover.verdict.fits", explanation: "ai.discover.verdict.fits_explanation" },
  might_fit: {
    label: "ai.discover.verdict.might_fit",
    explanation: "ai.discover.verdict.might_fit_explanation",
  },
  too_large: {
    label: "ai.discover.verdict.too_large",
    explanation: "ai.discover.verdict.too_large_explanation",
  },
};

/** The catalogue keys of a verdict for this machine, `null` when none is given. */
export function verdictKeys(
  verdict: Compatibility | undefined,
): { label: string; explanation: string } | null {
  return verdict ? VERDICT_KEYS[verdict] : null;
}

/** The badge colour of a verdict. */
export function verdictVariant(verdict: Compatibility | undefined): "success" | "warning" | "neutral" {
  if (verdict === "fits") return "success";
  if (verdict === "might_fit" || verdict === "too_large") return "warning";
  return "neutral";
}
