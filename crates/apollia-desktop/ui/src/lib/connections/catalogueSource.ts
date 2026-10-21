/**
 * Where the add-a-connection catalogue reads its entries from, and how a
 * search narrows them.
 *
 * Opening the catalogue reads the selection Apollia offers: the one compiled
 * into the application, refreshed from the signed online index when it
 * answers in time, else the copy already kept. The public registry is asked
 * only through `exploreAll`, the handler of the "Explore all" button.
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
  /** The selection and the public catalogue. */
  registry: () => Promise<RegistryServerView[]>;
}

export interface CatalogueSource {
  /** The selection; never asks the public catalogue. */
  selection: () => Promise<CatalogueView>;
  /** The selection and the public catalogue: the only path that asks it. */
  exploreAll: () => Promise<CatalogueView>;
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
  let full: RegistryServerView[] | null = null;

  async function readSelection(): Promise<CatalogueFailure[]> {
    if (curated === null) curated = await fetchers.curated().catch(() => null);
    return curated === null ? ["selection"] : [];
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
      return { tier: "curated", selection: shown, publicEntries: [], failures: [...failures, "public"] };
    }
    const offered = new Set(shown.map((e) => e.name));
    return {
      tier: "full",
      selection: shown,
      publicEntries: full.filter((e) => !offered.has(e.name)),
      failures,
    };
  }

  return { selection, exploreAll };
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
