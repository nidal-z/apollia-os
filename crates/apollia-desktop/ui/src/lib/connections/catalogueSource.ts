/**
 * Where the add-a-connection catalogue reads its entries from, and how a
 * search narrows them.
 *
 * Opening the catalogue reads the selection Apollia offers: the one compiled
 * into the application, refreshed from the signed online index when it
 * answers in time, else the copy already kept. The public registry is asked
 * only through `exploreAll`, the handler of the "Explore all" button, and
 * through `search`, when a text is typed: the registry's own search then
 * finds what the first pages it gave do not hold.
 */
import { fetchMcpCurated, fetchMcpRegistry } from "$lib/ipc/connections";
import { vendorLabel } from "$lib/connections/vendor";
import { allLocales } from "$lib/connections/catalogueText";
import type { RegistryServerView } from "$lib/types";

/** What the catalogue shows: nothing yet, the selection, or everything. */
export type CatalogueTier = "none" | "curated" | "full";

/** A source that did not answer: the selection Apollia offers, or the public catalogue. */
export type CatalogueFailure = "selection" | "public";

export interface CatalogueView {
  tier: CatalogueTier;
  /** The selection Apollia offers. */
  selection: RegistryServerView[];
  /** The public catalogue beyond the selection; empty until "Explore all" answers. */
  publicEntries: RegistryServerView[];
  /** The sources that failed at the last gesture, each told apart from an empty answer. */
  failures: CatalogueFailure[];
}

export interface CatalogueFetchers {
  /** The selection Apollia ships, read without network. */
  curated: () => Promise<RegistryServerView[]>;
  /** The selection and the public catalogue, or what its search finds for `search`. */
  registry: (search?: string) => Promise<RegistryServerView[]>;
}

/** Fewer letters than this are not sent to the registry's search. */
export const MIN_SEARCH_LENGTH = 2;

export interface CatalogueSource {
  /** The selection; never asks the public catalogue. */
  selection: () => Promise<CatalogueView>;
  /** The selection and the public catalogue. */
  exploreAll: () => Promise<CatalogueView>;
  /**
   * What the registry's search finds for `term`, joined to the public
   * catalogue the window shows at `tier`. A term shorter than
   * `MIN_SEARCH_LENGTH` asks nothing.
   */
  search: (term: string, tier: CatalogueTier) => Promise<CatalogueView>;
}

const IPC_FETCHERS: CatalogueFetchers = {
  curated: fetchMcpCurated,
  registry: fetchMcpRegistry,
};

/**
 * A catalogue source that reads each list once per screen. A failed read is
 * not kept, so the next gesture tries again, and it is named in `failures`
 * rather than shown as a catalogue with nothing in it; a failed "Explore all"
 * falls back to the selection.
 */
export function createCatalogueSource(fetchers: CatalogueFetchers = IPC_FETCHERS): CatalogueSource {
  let curated: RegistryServerView[] | null = null;
  let curatedRead: Promise<RegistryServerView[] | null> | null = null;
  let full: RegistryServerView[] | null = null;
  const found = new Map<string, RegistryServerView[]>();
  let lastTerm = "";

  async function readSelection(): Promise<CatalogueFailure[]> {
    if (curated === null) {
      curatedRead ??= fetchers.curated().catch(() => null);
      curated = await curatedRead;
      curatedRead = null;
    }
    return curated === null ? ["selection"] : [];
  }

  /** The public entries to show: the catalogue read, then what the last search found, the selection left out. */
  function publicOf(entries: readonly RegistryServerView[]): RegistryServerView[] {
    const offered = new Set((curated ?? []).map((e) => e.name));
    const seen = new Set<string>();
    const merged: RegistryServerView[] = [];
    for (const e of [...entries, ...(found.get(lastTerm) ?? [])]) {
      if (offered.has(e.name) || seen.has(e.name)) continue;
      seen.add(e.name);
      merged.push(e);
    }
    return merged;
  }

  async function selection(): Promise<CatalogueView> {
    const failures = await readSelection();
    return { tier: "curated", selection: curated ?? [], publicEntries: [], failures };
  }

  async function exploreAll(): Promise<CatalogueView> {
    const failures = await readSelection();
    if (full === null) full = await fetchers.registry().catch(() => null);
    const shown = curated ?? [];
    if (full === null) {
      return { tier: "curated", selection: shown, publicEntries: publicOf([]), failures: [...failures, "public"] };
    }
    return { tier: "full", selection: shown, publicEntries: publicOf(full), failures };
  }

  async function search(term: string, tier: CatalogueTier): Promise<CatalogueView> {
    const failures = await readSelection();
    lastTerm = term.trim();
    if (lastTerm.length >= MIN_SEARCH_LENGTH && !found.has(lastTerm)) {
      const answer = await fetchers.registry(lastTerm).catch(() => null);
      if (answer === null) failures.push("public");
      else found.set(lastTerm, answer);
    }
    const shownTier: CatalogueTier = tier === "full" && full !== null ? "full" : "curated";
    return {
      tier: shownTier,
      selection: curated ?? [],
      publicEntries: publicOf(shownTier === "full" ? (full ?? []) : []),
      failures,
    };
  }

  return { selection, exploreAll, search };
}

/** Lower case, without diacritics, so a search typed without accents finds an accented word. */
export function fold(text: string): string {
  return text.normalize("NFD").replace(/\p{Diacritic}/gu, "").toLowerCase();
}

/** Whether every word of `query` appears in one of `texts`; an empty query matches everything. */
export function matchesWords(texts: readonly (string | null | undefined)[], query: string): boolean {
  const words = fold(query).split(/\s+/).filter(Boolean);
  if (words.length === 0) return true;
  const haystack = fold(
    texts.filter((part): part is string => typeof part === "string" && part.length > 0).join(" "),
  );
  return words.every((word) => haystack.includes(word));
}

/**
 * Whether `entry` answers a search: every word of `query` appears in its
 * label, title, name, publisher, description or category, in any locale the
 * selection gives them. An empty query matches everything.
 */
export function matchesCatalogueQuery(entry: RegistryServerView, query: string): boolean {
  return matchesWords(
    [
      ...allLocales(entry.enrichment?.operator_label),
      ...allLocales(entry.enrichment?.description),
      entry.title,
      entry.name,
      vendorLabel(entry),
      entry.description,
      entry.category,
      entry.enrichment?.category,
    ],
    query,
  );
}
