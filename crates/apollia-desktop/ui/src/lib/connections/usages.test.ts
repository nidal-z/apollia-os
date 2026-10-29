import { describe, test, expect } from "vitest";
import { readFileSync } from "node:fs";

import en from "$lib/i18n/en.json";
import fr from "$lib/i18n/fr.json";
import { USAGES, catalogueUsage, categoryUsage, entryUsage } from "$lib/connections/usages";
import type { ConnectorEnrichmentView, RegistryServerView } from "$lib/types";

interface SelectionEntry {
  package_identifier: string;
  registry_names?: string[];
  operator_label: Record<string, string>;
  category: string;
  icon_name: string;
}

const selection = JSON.parse(
  readFileSync(new URL("../../../../src/mcp/enrichments.json", import.meta.url), "utf8"),
) as SelectionEntry[];

/** A selection entry as the add window receives it: named by its first registry name, else its package. */
function asView(entry: SelectionEntry): RegistryServerView {
  const enrichment: ConnectorEnrichmentView = {
    operator_label: entry.operator_label,
    category: entry.category,
    icon_name: entry.icon_name,
    registry_names: entry.registry_names,
    trust_level: "verified_official",
    auth_help_url: null,
    auth_help_text: null,
    auth_help_i18n_key: null,
    oauth_pre_registered_client_id_env: null,
  };
  return {
    name: entry.registry_names?.[0] ?? entry.package_identifier,
    title: null,
    description: null,
    version: "1.0.0",
    repository_url: null,
    website_url: null,
    packages: null,
    trust_level: "verified_official",
    category: entry.category,
    enrichment,
    is_installed: false,
    remotes: [],
  };
}

function publicEntry(category: string | null): RegistryServerView {
  return { ...asView(selection[0]), name: "io.github.someone/thing", enrichment: null, category };
}

describe("the usages of the add window", () => {
  test("the list is closed, named in English and in French, and other comes last", () => {
    // GIVEN the usages and both catalogues
    // WHEN each usage is looked up
    // THEN each has a name in both languages, the two differ where a word does, and "other" closes the list
    for (const usage of USAGES) {
      const english = (en.connections.usage as Record<string, string>)[usage];
      const french = (fr.connections.usage as Record<string, string>)[usage];
      expect(english, usage).toBeTruthy();
      expect(french, usage).toBeTruthy();
    }
    expect(en.connections.usage.all).toBeTruthy();
    expect(fr.connections.usage.all).toBeTruthy();
    expect(USAGES[USAGES.length - 1]).toBe("other");
    expect(Object.keys(en.connections.usage).sort()).toEqual([...USAGES, "all"].sort());
  });

  test("every service of the selection has a usage, none of them other", () => {
    // GIVEN the 56 services compiled into the application
    expect(selection).toHaveLength(56);
    // WHEN the usage of each is resolved
    const others = selection.filter((e) => catalogueUsage(asView(e)) === "other").map((e) => e.package_identifier);
    // THEN none falls under "other"
    expect(others).toEqual([]);
  });

  test("the accounts, the mailbox and the folder have their usage, the address none", () => {
    // GIVEN the built-in entries of the window
    // WHEN their usage is resolved
    // THEN Google is documents, Microsoft and the mailbox communication, the folder files
    expect(entryUsage({ key: "native:google", kind: "native", provider: "google" })).toBe("documents");
    expect(entryUsage({ key: "native:microsoft", kind: "native", provider: "microsoft" })).toBe("communication");
    expect(entryUsage({ key: "mailbox", kind: "mailbox" })).toBe("communication");
    expect(entryUsage({ key: "folder", kind: "folder" })).toBe("files");
    expect(entryUsage({ key: "address", kind: "address" })).toBeNull();
  });

  test("the services whose category says too little take the usage they serve", () => {
    // GIVEN Notion, a productivity tool for notes, and the local files, declared as data
    const byLabel = (label: string) => asView(selection.find((e) => e.operator_label.en === label) as SelectionEntry);
    // WHEN their usage is resolved
    // THEN each is where a person looks for it
    expect(catalogueUsage(byLabel("Notion"))).toBe("documents");
    expect(catalogueUsage(byLabel("Local Files"))).toBe("files");
    expect(catalogueUsage(byLabel("Linear"))).toBe("projects");
    expect(catalogueUsage(byLabel("Stripe"))).toBe("payments");
    expect(catalogueUsage(byLabel("Close"))).toBe("sales");
  });

  test("a public entry takes the usage of the category the shell inferred, else other", () => {
    // GIVEN public entries with each category `infer_category` can send, and one with none
    const inferred = ["data", "search", "communication", "development", "storage", "documents", "crm", "design", "productivity", "ai", "web", "media", "finance", "geo", "security", "analytics"];
    // WHEN their usage is resolved
    // THEN each inferred category names a usage, and no category is other
    for (const category of inferred) expect(categoryUsage(category), category).not.toBeNull();
    expect(catalogueUsage(publicEntry("documents"))).toBe("documents");
    expect(catalogueUsage(publicEntry("crm"))).toBe("sales");
    expect(catalogueUsage(publicEntry(null))).toBe("other");
    expect(catalogueUsage(publicEntry("astrology"))).toBe("other");
  });

  test("every category infer_category can return is one the window knows", () => {
    // GIVEN the Rust source of the inference
    const rust = readFileSync(new URL("../../../../src/commands/mcp/catalog.rs", import.meta.url), "utf8");
    const body = rust.slice(rust.indexOf("fn infer_category"), rust.indexOf("fn trust_level_str"));
    // WHEN the categories its rules return are read
    const returned = [...body.matchAll(/\],\s*"([a-z]+)",?\s*\)/g)].map((m) => m[1]);
    // THEN there are some, and each names a usage
    expect(returned.length).toBeGreaterThan(10);
    for (const category of returned) expect(categoryUsage(category), category).not.toBeNull();
  });
});
