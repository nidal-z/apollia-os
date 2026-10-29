/**
 * How the add window is browsed: by usage, by source, in an order, with or
 * without what asks a key, and the list gathered by usage that results.
 *
 * It reads the groups the search left (`addWindowGroups`): the verified
 * entries (the native accounts, the selection, a folder, a mailbox) and the
 * public catalogue. Pure and DOM-free; names come from the caller, which owns
 * the translations.
 */
import {
  entryNeeds,
  type AddEntry,
  type AddGroup,
  type BrowseSort,
  type BrowseSource,
  type BrowseState,
} from "$lib/connections/addWindow";
import { USAGES, entryUsage, type Usage } from "$lib/connections/usages";

/**
 * The orders that mean something for `source`. The recommended order is the
 * selection's, the services the agents of this machine suit first; the public
 * catalogue alone has no such order, so it is sorted by name only.
 */
export function sortsFor(source: BrowseSource): BrowseSort[] {
  return source === "public" ? ["name"] : ["recommended", "name"];
}

/** The order applied: the one chosen, or the first that means something for the source. */
export function effectiveSort(state: BrowseState): BrowseSort {
  const sorts = sortsFor(state.source);
  return sorts.includes(state.sort) ? state.sort : sorts[0];
}

/** Whether the source shows the public catalogue, which is then read. */
export function sourceShowsPublic(source: BrowseSource): boolean {
  return source !== "verified";
}

/** What adding an entry asks that is typed by hand: a key, values, a password, an address. */
const TYPED = new Set(["key", "values", "mailbox", "address"]);

/** Whether `entry` is added without a key to type: a sign-in to an account, or nothing. */
export function isKeyless(entry: AddEntry): boolean {
  return !entryNeeds(entry).some((need) => TYPED.has(need));
}

export interface UsageGroup {
  usage: Usage;
  entries: AddEntry[];
}

export interface BrowseView {
  /** The entries gathered by usage, in the order of the usages, empty ones left out. */
  groups: UsageGroup[];
  /**
   * The public entries a search found while the source is the selection: shown
   * apart, after the groups, rather than mixed into them.
   */
  community: AddEntry[];
  /** Nothing answers the usage, the source, the filter and the words together. */
  empty: boolean;
  /** Some public entry is shown, in the groups or apart: the note on community entries is due. */
  communityShown: boolean;
}

export interface BrowseInputs {
  groups: readonly AddGroup[];
  state: BrowseState;
  /** Words were typed: what the registry's search found is shown even under the selection. */
  searched: boolean;
  /** The name of an entry in the interface language, for the order by name. */
  nameOf: (entry: AddEntry) => string;
  locale?: string | null;
}

/** Whether an entry is verified, that is, offered by Apollia rather than taken from the public catalogue. */
export function isVerified(entry: AddEntry): boolean {
  return entry.kind !== "catalogue" || entry.source === "selection";
}

/**
 * The list the window shows. Within each group the verified entries come
 * first, then the public ones; each part keeps the order of the selection
 * (recommended) or is sorted by name.
 */
export function browseCatalogue(inputs: BrowseInputs): BrowseView {
  const { groups, state, searched, nameOf } = inputs;
  const verified = groups.filter((g) => g.key === "apollia" || g.key === "computer").flatMap((g) => g.entries);
  const publicEntries = groups.filter((g) => g.key === "public").flatMap((g) => g.entries);
  const sort = effectiveSort(state);
  const collator = new Intl.Collator(inputs.locale ?? undefined, { sensitivity: "base" });
  const ordered = (entries: AddEntry[]): AddEntry[] =>
    sort === "name" ? [...entries].sort((a, b) => collator.compare(nameOf(a), nameOf(b))) : entries;
  const kept = (entry: AddEntry): boolean => {
    if (state.keyless && !isKeyless(entry)) return false;
    return state.usage === "all" || entryUsage(entry) === state.usage;
  };

  const listed: AddEntry[] = [
    ...(state.source === "public" ? [] : ordered(verified.filter(kept))),
    ...(state.source === "verified" ? [] : ordered(publicEntries.filter(kept))),
  ];
  const byUsage = new Map<Usage, AddEntry[]>();
  for (const entry of listed) {
    const usage = entryUsage(entry);
    if (usage === null) continue;
    const group = byUsage.get(usage);
    if (group) group.push(entry);
    else byUsage.set(usage, [entry]);
  }
  const usageGroups = USAGES.flatMap((usage): UsageGroup[] => {
    const entries = byUsage.get(usage);
    return entries ? [{ usage, entries }] : [];
  });
  const community =
    state.source === "verified" && searched ? ordered(publicEntries.filter(kept)) : [];
  const communityShown =
    community.length > 0 || usageGroups.some((group) => group.entries.some((entry) => !isVerified(entry)));
  return {
    groups: usageGroups,
    community,
    empty: usageGroups.length === 0 && community.length === 0,
    communityShown,
  };
}

/** How an entry is connected, as the card names it. */
export type EntryAuth = "account" | "account_or_key" | "key" | "values" | "folder" | "mailbox" | "keyless";

/**
 * How `entry` is connected: an account (with a key in reserve when it declares
 * one), a key, settings, a folder, a mailbox, or nothing to type.
 */
export function entryAuth(entry: AddEntry): EntryAuth {
  const needs = entryNeeds(entry);
  if (needs.includes("sign_in")) return needs.includes("key_fallback") ? "account_or_key" : "account";
  if (needs.includes("mailbox")) return "mailbox";
  if (needs.includes("folder")) return "folder";
  if (needs.includes("key")) return "key";
  if (needs.includes("values")) return "values";
  return "keyless";
}

/** Where a connection runs: a service reached online, or a program of this computer. */
export type EntryPlace = "remote" | "machine";

/** Where `entry` runs once added: online when adding it needs the network, else on this computer. */
export function entryPlace(entry: AddEntry): EntryPlace {
  return entryNeeds(entry).includes("online") ? "remote" : "machine";
}

/**
 * How many entries each usage chip would show under the source, the filter
 * and the words in force, and the total for "all usages".
 */
export function usageCounts(inputs: BrowseInputs): Record<Usage | "all", number> {
  const view = browseCatalogue({ ...inputs, state: { ...inputs.state, usage: "all" } });
  const counts = Object.fromEntries([...USAGES, "all"].map((usage) => [usage, 0])) as Record<Usage | "all", number>;
  for (const group of view.groups) {
    counts[group.usage] = group.entries.length;
    counts.all += group.entries.length;
  }
  return counts;
}
