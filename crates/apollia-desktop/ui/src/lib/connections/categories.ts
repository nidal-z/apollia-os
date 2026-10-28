/**
 * The categories of the selection, by the key of their name in the catalogue.
 *
 * An entry of the selection declares one of these slugs as its `category`; the
 * add window finds it by the name of its category in the interface language,
 * as well as by the slug itself. A slug missing here is found by the slug
 * alone.
 */
export const CATEGORY_KEYS: Readonly<Record<string, string>> = {
  productivity: "connections.category.productivity",
  search: "connections.category.search",
  data: "connections.category.data",
  development: "connections.category.development",
  communication: "connections.category.communication",
  finance: "connections.category.finance",
  design: "connections.category.design",
  infrastructure: "connections.category.infrastructure",
  utility: "connections.category.utility",
  crm: "connections.category.crm",
  support: "connections.category.support",
  documents: "connections.category.documents",
  storage: "connections.category.storage",
  analytics: "connections.category.analytics",
  marketing: "connections.category.marketing",
};

/** The catalogue key naming `category`, or null for a slug with no name. */
export function categoryKey(category: string | null | undefined): string | null {
  return category ? (CATEGORY_KEYS[category] ?? null) : null;
}
