/**
 * Where the add-a-connection catalogue reads its entries from, and how a
 * search narrows them.
 *
 * Opening the catalogue reads the selection Apollia ships, which needs no
 * network. The public catalogue is asked only through `exploreAll`, the
 * handler of the "Explore all" button: nothing leaves the machine before the
 * user asks for it.
 */
import { fetchMcpCurated, fetchMcpRegistry } from "$lib/ipc/connections";
import { vendorLabel } from "$lib/connections/vendor";
import type { RegistryServerView } from "$lib/types";

/** What the catalogue shows: nothing yet, the selection, or everything. */
export type CatalogueTier = "none" | "curated" | "full";

export interface CatalogueView {
  tier: CatalogueTier;
  entries: RegistryServerView[];
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
 * not kept, so the next gesture tries again; a failed "Explore all" falls back
 * to the selection.
 */
export function createCatalogueSource(fetchers: CatalogueFetchers = IPC_FETCHERS): CatalogueSource {
  let curated: RegistryServerView[] | null = null;
  let full: RegistryServerView[] | null = null;

  async function selection(): Promise<CatalogueView> {
    if (curated === null) {
      curated = await fetchers.curated().catch(() => null);
    }
    return { tier: "curated", entries: curated ?? [] };
  }

  async function exploreAll(): Promise<CatalogueView> {
    if (full === null) {
      full = await fetchers.registry().catch(() => null);
    }
    return full === null ? selection() : { tier: "full", entries: full };
  }

  return { selection, exploreAll };
}

/** Lower case, without diacritics, so a search typed without accents finds an accented word. */
function fold(text: string): string {
  return text.normalize("NFD").replace(/\p{Diacritic}/gu, "").toLowerCase();
}

/**
 * Whether `entry` answers a search: every word of `query` appears in its
 * label, title, name, publisher, description or category. An empty query
 * matches everything.
 */
export function matchesCatalogueQuery(entry: RegistryServerView, query: string): boolean {
  const words = fold(query).split(/\s+/).filter(Boolean);
  if (words.length === 0) return true;
  const haystack = fold(
    [
      entry.enrichment?.operator_label,
      entry.title,
      entry.name,
      vendorLabel(entry),
      entry.description,
      entry.category,
      entry.enrichment?.category,
    ]
      .filter((part): part is string => typeof part === "string" && part.length > 0)
      .join(" "),
  );
  return words.every((word) => haystack.includes(word));
}
