import { describe, test, expect } from "vitest";

import {
  browseCatalogue,
  effectiveSort,
  entryAuth,
  isKeyless,
  sortsFor,
  sourceShowsPublic,
} from "$lib/connections/addBrowse";
import { BROWSE_DEFAULT, addWindowGroups, type AddEntry, type BrowseState } from "$lib/connections/addWindow";
import type { RegistryServerView } from "$lib/types";

function entry(name: string, title: string, category: string, over: Partial<RegistryServerView> = {}): RegistryServerView {
  return {
    name,
    title,
    description: null,
    version: "1.0.0",
    repository_url: null,
    website_url: null,
    packages: null,
    trust_level: "verified_official",
    category,
    enrichment: null,
    is_installed: false,
    remotes: [{ type: "streamable-http", url: `https://${name}.example/mcp`, headers: [] }],
    ...over,
  };
}

const KEY = [{ name: "Authorization", description: null, isRequired: true, isSecret: true, format: null, default: null, choices: null }];
const SLACK = entry("com.slack/mcp", "Slack", "communication");
const STRIPE = entry("com.stripe/mcp", "Stripe", "finance", {
  remotes: [{ type: "streamable-http", url: "https://mcp.stripe.com", headers: KEY }],
} as Partial<RegistryServerView>);
const ATTIO = entry("com.attio/mcp", "Attio", "crm");
const ADYEN = entry("io.github.someone/adyen", "Adyen", "finance", { trust_level: "community" });
const ALPHA_PAY = entry("io.github.someone/alpha-pay", "Alpha pay", "finance", { trust_level: "community" });

const WORDS: Record<string, string[]> = {
  "native:google": ["Google Workspace"],
  "native:microsoft": ["Microsoft 365"],
  folder: ["Folder"],
  mailbox: ["Mailbox"],
};

function nameOf(e: AddEntry): string {
  if (e.kind === "catalogue") return e.entry.title ?? e.entry.name;
  return WORDS[e.key]?.[0] ?? e.key;
}

function view(state: Partial<BrowseState>, query = "", publicEntries = [ADYEN, ALPHA_PAY]) {
  const groups = addWindowGroups({
    selection: [STRIPE, SLACK, ATTIO],
    publicEntries,
    query,
    wordsOf: (key) => WORDS[key] ?? [],
  });
  return browseCatalogue({ groups, state: { ...BROWSE_DEFAULT, ...state }, searched: query.trim() !== "", nameOf, locale: "en" });
}

const keysOf = (entries: AddEntry[]) => entries.map((e) => e.key);
const usagesOf = (v: ReturnType<typeof view>) => v.groups.map((g) => g.usage);

describe("the list gathered by usage", () => {
  test("by default the verified entries, gathered by usage in the order of the usages", () => {
    // GIVEN the selection, the accounts and the public catalogue read
    // WHEN the window is browsed as it opens
    const v = view({});
    // THEN only verified entries show, each under its usage, the usages in their order
    expect(usagesOf(v)).toEqual(["communication", "documents", "sales", "payments", "files"]);
    expect(keysOf(v.groups[0].entries)).toEqual(["native:microsoft", "entry:com.slack/mcp", "mailbox"]);
    expect(v.groups.flatMap((g) => g.entries).some((e) => e.kind === "catalogue" && e.source === "public")).toBe(false);
    expect(v.community).toEqual([]);
    expect(v.empty).toBe(false);
  });

  test("a usage chip keeps that usage alone", () => {
    // GIVEN the usage payments chosen
    // WHEN the window is browsed
    const v = view({ usage: "payments" });
    // THEN one group, Stripe in it
    expect(usagesOf(v)).toEqual(["payments"]);
    expect(keysOf(v.groups[0].entries)).toEqual(["entry:com.stripe/mcp"]);
  });

  test("everything mixes the public entries into the usages, the verified ones first", () => {
    // GIVEN the source "everything"
    // WHEN the window is browsed by payments
    const v = view({ source: "all", usage: "payments" });
    // THEN Stripe leads, then the public payment services
    expect(keysOf(v.groups[0].entries)).toEqual([
      "entry:com.stripe/mcp",
      "entry:io.github.someone/adyen",
      "entry:io.github.someone/alpha-pay",
    ]);
  });

  test("the public catalogue alone drops the selection and sorts by name, the only order it has", () => {
    // GIVEN the public catalogue as source, the recommended order still held
    const state = { ...BROWSE_DEFAULT, source: "public" as const };
    // WHEN the window is browsed
    const v = view(state);
    // THEN only public entries show, by name, and the order offered is the name alone
    expect(sortsFor("public")).toEqual(["name"]);
    expect(sortsFor("verified")).toEqual(["recommended", "name"]);
    expect(effectiveSort(state)).toBe("name");
    expect(keysOf(v.groups.flatMap((g) => g.entries))).toEqual([
      "entry:io.github.someone/adyen",
      "entry:io.github.someone/alpha-pay",
    ]);
  });

  test("the order by name sorts each part, the verified entries still first", () => {
    // GIVEN everything, by name, in communication
    // WHEN the window is browsed
    const v = view({ source: "all", sort: "name", usage: "communication" });
    // THEN Mailbox, Microsoft 365, Slack: alphabetical, whatever the recommended order was
    expect(keysOf(v.groups[0].entries)).toEqual(["mailbox", "native:microsoft", "entry:com.slack/mcp"]);
  });

  test("no setup keeps what connects without a key, a password or a value to type", () => {
    // GIVEN Stripe asking a key, the mailbox a password, Slack and the accounts nothing typed
    // WHEN "no setup" is ticked
    const v = view({ keyless: true });
    const shown = keysOf(v.groups.flatMap((g) => g.entries));
    // THEN Stripe and the mailbox are gone, Slack, the accounts and the folder stay
    expect(shown).not.toContain("entry:com.stripe/mcp");
    expect(shown).not.toContain("mailbox");
    expect(shown).toEqual(expect.arrayContaining(["entry:com.slack/mcp", "native:google", "native:microsoft", "folder"]));
    expect(isKeyless({ key: "address", kind: "address" })).toBe(false);
  });

  test("words typed under the selection show what the registry found apart, filtered like the rest", () => {
    // GIVEN words typed that a public entry answers
    // WHEN the window is browsed on the selection, then with payments, then with "no setup"
    const found = view({}, "adyen");
    const otherUsage = view({ usage: "communication" }, "adyen");
    // THEN the public entry is in the community part, not in the groups,
    //      and a usage it does not serve leaves it out
    expect(keysOf(found.community)).toEqual(["entry:io.github.someone/adyen"]);
    expect(found.groups).toEqual([]);
    expect(found.empty).toBe(false);
    expect(otherUsage.community).toEqual([]);
    expect(otherUsage.empty).toBe(true);
  });

  test("search, usage, source, sort and filter combine, and nothing left is an empty view", () => {
    // GIVEN words, a usage, a source and the filter together
    // WHEN nothing answers all of them
    const v = view({ source: "all", usage: "payments", keyless: true, sort: "name" }, "stripe");
    const kept = view({ source: "all", usage: "payments", sort: "name" }, "pay");
    // THEN the view is empty; loosened, the words and the usage still narrow it, sorted
    expect(v.empty).toBe(true);
    expect(keysOf(kept.groups.flatMap((g) => g.entries))).toEqual(["entry:io.github.someone/alpha-pay"]);
  });

  test("only a source that includes the public catalogue reads it", () => {
    // GIVEN the three sources
    // WHEN each is asked whether it shows the public catalogue
    // THEN the selection alone does not
    expect(sourceShowsPublic("verified")).toBe(false);
    expect(sourceShowsPublic("public")).toBe(true);
    expect(sourceShowsPublic("all")).toBe(true);
  });
});

describe("how an entry connects", () => {
  test("an account, a key, a folder, a mailbox, or nothing to type", () => {
    // GIVEN one entry of each kind
    // WHEN how each connects is read
    // THEN each names its own
    expect(entryAuth({ key: "native:google", kind: "native", provider: "google" })).toBe("account");
    expect(entryAuth({ key: "entry:com.stripe/mcp", kind: "catalogue", entry: STRIPE, source: "selection" })).toBe("key");
    expect(entryAuth({ key: "entry:com.slack/mcp", kind: "catalogue", entry: SLACK, source: "selection" })).toBe("keyless");
    expect(entryAuth({ key: "folder", kind: "folder" })).toBe("folder");
    expect(entryAuth({ key: "mailbox", kind: "mailbox" })).toBe("mailbox");
  });
});
