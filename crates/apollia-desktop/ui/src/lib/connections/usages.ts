/**
 * What a service of the add window is for: a closed list of usages, the one
 * of each entry, and the order the window shows them in.
 *
 * A person who knows what they need but no service by its name browses by
 * usage. Every entry of the selection has one: its category names it, and the
 * few whose category says too little (a "productivity" that is really notes,
 * a "development" that is really the web) are named here one by one. An entry
 * of the public catalogue takes the usage of the category the shell inferred
 * for it (`infer_category`), and falls under "other" when none was.
 *
 * Pure and i18n-agnostic: labels are catalogue keys the caller resolves.
 */
import type { AddEntry } from "$lib/connections/addWindow";
import type { RegistryServerView } from "$lib/types";

/** The usages, in the order of their chips; "other" is always last. */
export const USAGES = [
  "communication",
  "documents",
  "projects",
  "sales",
  "payments",
  "development",
  "data",
  "design",
  "analytics",
  "files",
  "web",
  "other",
] as const;

export type Usage = (typeof USAGES)[number];

const USAGE_KEYS: Readonly<Record<Usage, string>> = {
  communication: "connections.usage.communication",
  documents: "connections.usage.documents",
  projects: "connections.usage.projects",
  sales: "connections.usage.sales",
  payments: "connections.usage.payments",
  development: "connections.usage.development",
  data: "connections.usage.data",
  design: "connections.usage.design",
  analytics: "connections.usage.analytics",
  files: "connections.usage.files",
  web: "connections.usage.web",
  other: "connections.usage.other",
};

/** The catalogue key naming `usage`. */
export function usageKey(usage: Usage): string {
  return USAGE_KEYS[usage];
}

/** The usage of each category a selection entry declares or the shell infers. */
const CATEGORY_USAGE: Readonly<Record<string, Usage>> = {
  communication: "communication",
  support: "communication",
  documents: "documents",
  productivity: "projects",
  crm: "sales",
  marketing: "sales",
  finance: "payments",
  development: "development",
  infrastructure: "development",
  utility: "development",
  security: "development",
  ai: "development",
  data: "data",
  design: "design",
  analytics: "analytics",
  storage: "files",
  media: "files",
  search: "web",
  web: "web",
  geo: "web",
};

/**
 * The entries of the selection whose category names the wrong usage, by the
 * name the window gives them (their first registry name, else their package).
 */
const SELECTION_USAGE: Readonly<Record<string, Usage>> = {
  "com.notion/mcp": "documents",
  "@modelcontextprotocol/server-filesystem": "files",
  "@modelcontextprotocol/server-puppeteer": "web",
  "@playwright/mcp": "web",
  "com.typeform/typeform": "sales",
  "com.microsoft/learn-mcp": "development",
};

/** The usage a category names, or null for a category of no usage. */
export function categoryUsage(category: string | null | undefined): Usage | null {
  return category ? (CATEGORY_USAGE[category] ?? null) : null;
}

/** The usage of a catalogue entry: its own, else its category's, else "other". */
export function catalogueUsage(entry: RegistryServerView): Usage {
  const name = entry.enrichment?.registry_names?.[0] ?? entry.name;
  return (
    SELECTION_USAGE[name] ??
    SELECTION_USAGE[entry.name] ??
    categoryUsage(entry.enrichment?.category ?? entry.category) ??
    "other"
  );
}

/**
 * The usage of an entry of the window. The address of a service has none: it
 * is what remains when no usage answers, and stays apart from them.
 */
export function entryUsage(entry: AddEntry): Usage | null {
  switch (entry.kind) {
    case "native":
      return entry.provider === "google" ? "documents" : "communication";
    case "mailbox":
      return "communication";
    case "folder":
      return "files";
    case "address":
      return null;
    case "catalogue":
      return catalogueUsage(entry.entry);
  }
}
