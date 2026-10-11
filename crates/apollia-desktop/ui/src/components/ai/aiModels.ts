/**
 * The decisions of the AI screen, kept out of its components so they are
 * tested in `node`: which models a category and a filter show, in which
 * order, what the footer counts, and which actions a model offers.
 *
 * Pure and i18n-agnostic: labels are catalogue keys the caller resolves.
 */
import type { InstalledModel, ModelCategory } from "$lib/ipc/modelHub";
import type { LoadedModel } from "$lib/types";

/** The categories of the left column, in order; `all` lists them grouped. */
export type CategoryFilter = "all" | ModelCategory;

/** The categories of the index, in the order the screen shows them. */
export const CATEGORY_ORDER: readonly ModelCategory[] = [
  "language",
  "embeddings",
  "dictation",
  "draft",
];

/** The entries of the category column. */
export const CATEGORY_FILTERS: readonly CategoryFilter[] = ["all", ...CATEGORY_ORDER];

/** The operating system the window runs on, which names the file manager. */
export type HostPlatform = "mac" | "windows" | "linux";

/** The host platform, read from the webview. */
export function hostPlatform(platform: string = navigatorPlatform()): HostPlatform {
  const p = platform.toLowerCase();
  if (p.includes("mac")) return "mac";
  if (p.includes("win")) return "windows";
  return "linux";
}

function navigatorPlatform(): string {
  // NOSONAR typescript:S1874 - navigator.platform is the one reading every webview of Tauri gives.
  return typeof navigator === "undefined" ? "" : navigator.platform; // NOSONAR typescript:S1874
}

/** How many models a category entry counts. */
export function categoryCount(models: readonly InstalledModel[], filter: CategoryFilter): number {
  return filter === "all" ? models.length : models.filter((m) => m.category === filter).length;
}

/** Whether a model answers the text typed in the filter field. */
export function matchesQuery(model: InstalledModel, query: string): boolean {
  const q = query.trim().toLowerCase();
  if (!q) return true;
  return [model.name, model.id, model.parameters ?? ""].some((v) => v.toLowerCase().includes(q));
}

/**
 * The name a row of the list shows: the name of the model, or the name of
 * the file (the folder, for an MLX model) of its weights when Settings >
 * General asks for the full file name.
 */
export function shownModelName(model: InstalledModel, fullFileName: boolean): string {
  return fullFileName && model.name !== "" ? model.name : model.id;
}

/**
 * The models of one category in screen order: the pinned ones first, the most
 * recently pinned on top, then the others by name.
 */
export function sortModels(models: readonly InstalledModel[]): InstalledModel[] {
  return [...models].sort((a, b) => {
    if (a.pinned_at && b.pinned_at) return b.pinned_at.localeCompare(a.pinned_at);
    if (a.pinned_at) return -1;
    if (b.pinned_at) return 1;
    return a.name.localeCompare(b.name);
  });
}

/** One group of rows of the table. */
export interface ModelGroup {
  category: ModelCategory;
  models: InstalledModel[];
}

/**
 * The rows the table shows for a category entry and a filter: one group per
 * category under `all`, empty groups left out; the chosen category alone
 * otherwise, even empty, so the screen can say the category is empty.
 */
export function groupModels(
  models: readonly InstalledModel[],
  filter: CategoryFilter,
  query: string,
): ModelGroup[] {
  const visible = models.filter((m) => matchesQuery(m, query));
  const categories = filter === "all" ? CATEGORY_ORDER : [filter];
  return categories
    .map((category) => ({
      category,
      models: sortModels(visible.filter((m) => m.category === category)),
    }))
    .filter((g) => filter !== "all" || g.models.length > 0);
}

/** The place every installed model takes on disk, in bytes. */
export function totalBytes(models: readonly InstalledModel[]): number {
  return models.reduce((sum, m) => sum + m.size_bytes, 0);
}

/** Gigabytes to the tenth, in the reader's locale (`16,1` in French). */
export function gigabytes(bytes: number, locale: string): string {
  return new Intl.NumberFormat(locale, {
    minimumFractionDigits: 1,
    maximumFractionDigits: 1,
  }).format(bytes / 1e9);
}

/** The memory a loaded model takes, in bytes: the guardrail estimate, else its weights. */
export function loadedMemoryBytes(model: LoadedModel): number | null {
  if (typeof model.footprintGb === "number") return model.footprintGb * 1e9;
  return typeof model.sizeBytes === "number" ? model.sizeBytes : null;
}

/** The kind label of a loaded model. */
export function loadedKindKey(model: LoadedModel): string {
  return model.kind === "embeddings" ? "ai.kind.embeddings" : "ai.kind.language";
}

/** What the kind of a loaded model means, shown on hover. */
export function loadedKindExplanationKey(model: LoadedModel): string {
  return model.kind === "embeddings"
    ? "ai.kind.embeddings_explanation"
    : "ai.kind.language_explanation";
}

/** The format of a loaded model, read from the engine that serves it. */
export function loadedFormat(model: LoadedModel): "gguf" | "mlx" {
  return model.engine === "vllm-mlx" ? "mlx" : "gguf";
}

/** The keyboard hint of the filter field. */
export function filterShortcutLabel(platform: HostPlatform): string {
  return platform === "mac" ? "⌘ F" : "Ctrl F";
}

/** Whether a key press is the filter shortcut: ⌘F on macOS, Ctrl+F elsewhere. */
export function isFilterShortcut(
  event: { key: string; metaKey: boolean; ctrlKey: boolean; altKey?: boolean; shiftKey?: boolean },
  platform: HostPlatform,
): boolean {
  if (event.key.toLowerCase() !== "f" || event.altKey || event.shiftKey) return false;
  return platform === "mac" ? event.metaKey && !event.ctrlKey : event.ctrlKey && !event.metaKey;
}

/** What the main action of a row does, if it has one. */
export type DefaultAction = "use_default" | "use_search";

/**
 * The default this model can become: chat for a language model, search for an
 * embeddings model the registry knows. An embeddings model is never offered
 * for chat, and a model that already is the default offers nothing.
 */
export function defaultAction(model: InstalledModel): DefaultAction | null {
  if (model.category === "language") return model.default_chat ? null : "use_default";
  if (model.category === "embeddings" && model.embedding_entry) {
    return model.default_search ? null : "use_search";
  }
  return null;
}

/** The identifier of an entry of the actions menu. */
export type MenuEntryId =
  | "reveal"
  | "pin"
  | "copy_id"
  | "copy_path"
  | "source"
  | "use_default"
  | "use_search"
  | "delete";

/** One entry of the actions menu of a row. */
export interface MenuEntry {
  id: MenuEntryId;
  labelKey: string;
  /** Why the entry cannot be used, shown on hover; absent when it can. */
  reasonKey?: string;
  destructive?: boolean;
  /** Entries of one section; a separator stands between two sections. */
  section: 0 | 1 | 2;
}

const REVEAL_KEYS: Record<HostPlatform, string> = {
  mac: "ai.menu.reveal_mac",
  windows: "ai.menu.reveal_windows",
  linux: "ai.menu.reveal_linux",
};

/** The label that shows a file in the file manager of the platform. */
export function revealLabelKey(platform: HostPlatform): string {
  return REVEAL_KEYS[platform];
}

/** The entries of the actions menu of `model`, in order. */
export function menuEntries(model: InstalledModel, platform: HostPlatform): MenuEntry[] {
  const entries: MenuEntry[] = [
    { id: "reveal", labelKey: revealLabelKey(platform), section: 0 },
    {
      id: "pin",
      labelKey: model.pinned_at ? "ai.menu.unpin" : "ai.menu.pin",
      section: 0,
    },
    { id: "copy_id", labelKey: "ai.menu.copy_id", section: 0 },
    { id: "copy_path", labelKey: "ai.menu.copy_path", section: 0 },
    {
      id: "source",
      labelKey: "ai.menu.source",
      reasonKey: model.source_url ? undefined : "ai.menu.source_none",
      section: 1,
    },
  ];
  const action = defaultAction(model);
  if (action === "use_default") {
    entries.push({ id: "use_default", labelKey: "ai.menu.use_default", section: 1 });
  } else if (action === "use_search") {
    entries.push({ id: "use_search", labelKey: "ai.menu.use_search", section: 1 });
  }
  entries.push({
    id: "delete",
    labelKey: "ai.menu.delete",
    // The runtime refuses the file of a model a configuration still names;
    // the entry says so rather than offering a deletion that will be refused.
    reasonKey: model.in_use ? "ai.menu.delete_in_use" : undefined,
    destructive: true,
    section: 2,
  });
  return entries;
}

const CATEGORY_LABEL_KEYS: Record<CategoryFilter, string> = {
  all: "ai.category.all",
  language: "ai.category.language",
  embeddings: "ai.category.embeddings",
  dictation: "ai.category.dictation",
  draft: "ai.category.draft",
};

const EMPTY_CATEGORY_KEYS: Record<ModelCategory, string> = {
  language: "ai.installed.empty_language",
  embeddings: "ai.installed.empty_embeddings",
  dictation: "ai.installed.empty_dictation",
  draft: "ai.installed.empty_draft",
};

/** The catalogue key of a category entry. */
export function categoryLabelKey(filter: CategoryFilter): string {
  return CATEGORY_LABEL_KEYS[filter];
}

/** The catalogue key that explains an empty category. */
export function emptyCategoryKey(category: ModelCategory): string {
  return EMPTY_CATEGORY_KEYS[category];
}

/** A parameter count read from the index, split for display. */
export interface ParameterCount {
  /** The number, in the reader's locale (`1,5` in French). */
  value: string;
  /** The catalogue key of its unit. */
  unitKey: "ai.installed.params_billions" | "ai.installed.params_millions";
}

/**
 * The parameter count of the index (`"8B"`, `"1.5B"`, `"137M"`) for display;
 * `null` for a label of another shape, shown as written.
 */
export function parameterCount(label: string | null, locale: string): ParameterCount | null {
  const match = /^\s*(\d+(?:\.\d+)?)\s*([BM])\s*$/i.exec(label ?? "");
  if (!match) return null;
  const value = new Intl.NumberFormat(locale, { maximumFractionDigits: 1 }).format(
    Number(match[1]),
  );
  const unitKey =
    match[2]?.toUpperCase() === "B" ? "ai.installed.params_billions" : "ai.installed.params_millions";
  return { value, unitKey };
}
