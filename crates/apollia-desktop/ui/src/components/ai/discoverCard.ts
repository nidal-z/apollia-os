/**
 * The decisions of the card of the discovery window once a model is chosen,
 * kept out of its components so they are tested in `node`: how a variant
 * reads in the list of download options, what the settings of its author
 * read, which repository a README is asked from, which page "See the source"
 * opens, and whether a conversation can start on the chosen variant.
 *
 * Pure and i18n-agnostic: a text is a catalogue key the caller resolves.
 */
import type { InstalledModel, ModelVariant } from "$lib/ipc/modelHub";
import {
  fileName,
  levelBits,
  type AuthorField,
  type CardDefaults,
  type DiscoverCategory,
  type DiscoverEntry,
} from "./discover";

/** The host every repository of the catalogue lives on. */
const HUB = "https://huggingface.co";

/** How the precision of a variant reads in its row. */
export type PrecisionLabel =
  /** A language model: bits per weight, the way the window says it. */
  | { kind: "bits"; bits: number }
  /** Any other model: the quantisation as its registry entry states it. */
  | { kind: "published"; text: string };

/** The precision of `variant` for a card of `category`. */
export function precisionLabel(variant: ModelVariant, category: DiscoverCategory): PrecisionLabel {
  return category === "language"
    ? { kind: "bits", bits: levelBits(variant.level) }
    : { kind: "published", text: variant.quantization };
}

/**
 * Whether a row states the storage format. A dictation file is stored the
 * way its engine reads it, which the filter of formats does not cover.
 */
export function showsFormat(category: DiscoverCategory): boolean {
  return category !== "dictation";
}

/** The catalogue key of what a model that does not converse is for. */
export function purposeKey(category: DiscoverCategory): string | null {
  if (category === "embeddings") return "ai.kind.embeddings_explanation";
  if (category === "dictation") return "ai.kind.dictation_explanation";
  return null;
}

/** The catalogue key of the domain of a card. */
export function domainKey(category: DiscoverCategory): string {
  if (category === "embeddings") return "ai.discover.category_embeddings";
  if (category === "dictation") return "ai.category.dictation";
  return "ai.kind.language";
}

/** One setting of the author, ready to read. */
export interface AuthorFieldRow {
  name: string;
  /** The catalogue key of its type: a list of values, or yes or no. */
  typeKey: "ai.discover.author_fields.type_list" | "ai.discover.author_fields.type_boolean";
  /** The values of a list, as published; empty for a yes or no. */
  values: string[];
  /** Its default: a catalogue key for yes or no, the published value otherwise. */
  defaultValue: { key: string } | { text: string };
  explanation: string;
}

/** The settings the author defines, in the order of the card. */
export function authorFieldRows(fields: readonly AuthorField[]): AuthorFieldRow[] {
  return fields.map((field): AuthorFieldRow => {
    const boolean = field.type === "booleen";
    return {
      name: field.name,
      typeKey: boolean ? "ai.discover.author_fields.type_boolean" : "ai.discover.author_fields.type_list",
      values: boolean ? [] : (field.values ?? []).map((value) => String(value)),
      defaultValue:
        typeof field.default === "boolean"
          ? { key: field.default ? "ai.discover.author_fields.yes" : "ai.discover.author_fields.no" }
          : { text: String(field.default) },
      explanation: field.explanation,
    };
  });
}

/** The request settings a card can advise, in display order, each with its name. */
const DEFAULT_PARAMETER_KEYS = {
  temperature: "ai.discover.defaults.temperature",
  top_p: "ai.discover.defaults.top_p",
  top_k: "ai.discover.defaults.top_k",
  min_p: "ai.discover.defaults.min_p",
  repetition_penalty: "ai.discover.defaults.repetition_penalty",
  presence_penalty: "ai.discover.defaults.presence_penalty",
} as const;

type DefaultParameter = keyof typeof DEFAULT_PARAMETER_KEYS;

const DEFAULT_PARAMETERS = Object.keys(DEFAULT_PARAMETER_KEYS) as DefaultParameter[];

/** The value at which a penalty changes nothing. */
const NEUTRAL: Partial<Record<DefaultParameter, number>> = { repetition_penalty: 1, presence_penalty: 0 };

/** One advised setting, ready to read. */
export interface DefaultParameterRow {
  id: DefaultParameter;
  labelKey: string;
  /** The value in the reader's locale; null when the setting is advised off. */
  value: string | null;
}

/** The settings a card advises, the ones it states and no other. */
export function defaultParameterRows(defaults: CardDefaults | null, locale: string): DefaultParameterRow[] {
  if (!defaults) return [];
  const format = new Intl.NumberFormat(locale, { maximumFractionDigits: 2 });
  return DEFAULT_PARAMETERS.flatMap((id): DefaultParameterRow[] => {
    const value = defaults[id];
    if (typeof value !== "number" || !Number.isFinite(value)) return [];
    return [{ id, labelKey: DEFAULT_PARAMETER_KEYS[id], value: value === NEUTRAL[id] ? null : format.format(value) }];
  });
}

/** The repository a page of the Hub belongs to, `owner/name`; null for another address. */
export function repoOfSourceUrl(url: string | null): string | null {
  if (!url || !url.startsWith(`${HUB}/`)) return null;
  const [owner, name] = url.slice(HUB.length + 1).split(/[/?#]/);
  return owner && name ? `${owner}/${name}` : null;
}

/**
 * The repository whose README the card reads: the one its author wrote, so
 * the model a conversion declares before the conversion, then the page of
 * the card, then the repository of the chosen variant.
 */
export function readmeRepo(entry: DiscoverEntry, chosen: ModelVariant | null): string | null {
  return (
    entry.found?.card.origin_repo ??
    repoOfSourceUrl(entry.sourceUrl) ??
    entry.embedding?.repo ??
    entry.dictation[0]?.repo ??
    chosen?.repo_id ??
    entry.found?.repo_id ??
    null
  );
}

/**
 * The page "See the source" opens: the one the card or the result states,
 * else the repository of the chosen variant. Null when nothing names a page,
 * which only happens while the variants of an uncarded entry are unread.
 */
export function sourcePage(entry: DiscoverEntry, chosen: ModelVariant | null): string | null {
  if (entry.sourceUrl) return entry.sourceUrl;
  const repo = entry.embedding?.repo ?? entry.dictation[0]?.repo ?? chosen?.repo_id ?? entry.repos[0] ?? null;
  return repo ? `${HUB}/${repo}` : null;
}

/** The text of a README without the metadata block that opens it. */
export function readmeBody(text: string): string {
  const lines = text.split(/\r?\n/);
  if (lines[0]?.trim() !== "---") return text;
  const end = lines.findIndex((line, index) => index > 0 && line.trim() === "---");
  return end === -1 ? text : lines.slice(end + 1).join("\n").trimStart();
}

/**
 * What the route said when it refused a README. The shell hands the status
 * and the body of the answer; the message is what the reader needs.
 */
export function readmeErrorText(err: unknown): string {
  const raw = err instanceof Error ? err.message : String(err);
  const body = raw.replace(/^API error \(\d+\):\s*/, "");
  try {
    const parsed: unknown = JSON.parse(body);
    if (parsed && typeof parsed === "object") {
      const message = (parsed as Record<string, unknown>).error;
      if (typeof message === "string" && message.trim() !== "") return message;
    }
  } catch {
    // The answer was plain text: it is the message.
  }
  return body;
}

/**
 * The model of this computer a variant became once downloaded: the language
 * model of the index whose weights are the file of the variant, or, for a
 * folder of weights, the one downloaded from its repository.
 */
export function installedMatch(
  variant: ModelVariant,
  installed: readonly InstalledModel[],
): InstalledModel | null {
  const language = installed.filter((m) => m.category === "language" && m.format === variant.format);
  if (variant.format === "gguf") {
    return language.find((m) => fileName(m.path) === variant.origin_file) ?? null;
  }
  const page = `${HUB}/${variant.repo_id}`.toLowerCase();
  return language.find((m) => m.source_url?.toLowerCase() === page) ?? null;
}

/** What "Use in a new conversation" does for the chosen variant. */
export type ConversationAction =
  /** A conversation opens on `route`, the name the selector of a conversation routes by. */
  | { kind: "ready"; route: string }
  /** The button is there, inactive, with its reason on hover. */
  | { kind: "blocked"; reasonKey: string };

/** The action of the chosen variant, the first reason that holds. */
export function conversationAction(
  entry: DiscoverEntry,
  chosen: ModelVariant | null,
  installed: readonly InstalledModel[],
): ConversationAction {
  if (entry.category !== "language") {
    return { kind: "blocked", reasonKey: "ai.discover.conversation.reason.not_language" };
  }
  if (!chosen?.downloaded) {
    return { kind: "blocked", reasonKey: "ai.discover.conversation.reason.not_downloaded" };
  }
  const model = installedMatch(chosen, installed);
  return model
    ? { kind: "ready", route: model.id }
    : { kind: "blocked", reasonKey: "ai.discover.conversation.reason.not_listed" };
}
