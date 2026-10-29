import { describe, test, expect, vi } from "vitest";
import { render as renderToString } from "svelte/server";
import { locale, waitLocale } from "svelte-i18n";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => null) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

vi.hoisted(() => {
  const map = new Map<string, string>();
  globalThis.localStorage = {
    getItem: (k: string) => map.get(k) ?? null,
    setItem: (k: string, v: string) => void map.set(k, v),
    removeItem: (k: string) => void map.delete(k),
    clear: () => map.clear(),
    key: () => null,
    length: 0,
  } as Storage;
});

import "$lib/i18n";
import en from "$lib/i18n/en.json";
import { developerMode } from "$lib/stores/developerMode";
import AddConnectionWindow from "./AddConnectionWindow.svelte";
import type { RegistryServerView } from "$lib/types";

const noop = () => {};

function entry(over: Partial<RegistryServerView> = {}): RegistryServerView {
  return {
    name: "com.notion/mcp",
    title: "Notion",
    description: "Pages and databases",
    version: "1.0.0",
    repository_url: null,
    website_url: null,
    packages: null,
    trust_level: "verified_official",
    category: "productivity",
    enrichment: null,
    is_installed: false,
    remotes: [{ type: "streamable-http", url: "https://mcp.notion.com/mcp", headers: [] }],
    ...over,
  };
}

const WEATHER = entry({ name: "io.github.someone/weather", title: "Weather", trust_level: "community", remotes: [] });

async function render(props: Record<string, unknown>): Promise<string> {
  locale.set("en");
  await waitLocale("en");
  developerMode.set(false);
  return renderToString(AddConnectionWindow as never, {
    props: {
      open: true, query: "", chosen: null, selection: [entry()], publicEntries: [], failures: [],
      catalogueTier: "curated", loading: false, agents: [], installedNames: new Set(),
      installedByPackage: new Map(), onclose: noop, onchoose: noop, onback: noop,
      onexploreall: noop, onadd: noop, onmanage: noop, oninstalled: noop,
      ...props,
    } as never,
  }).body;
}

function text(html: string): string {
  return html.replace(/<!--[\s\S]*?-->/g, " ").replace(/<[^>]+>/g, " ").replace(/\s+/g, " ");
}

/** Whether the button `testid` is rendered pressed. */
function pressed(html: string, testid: string): boolean {
  const tag = html.match(new RegExp(`<button[^>]*data-testid="${testid}"[^>]*>`))?.[0] ?? "";
  return tag.includes('aria-pressed="true"');
}

function section(html: string, testid: string): string {
  const at = html.indexOf(`data-testid="${testid}"`);
  if (at < 0) return "";
  const close = html.indexOf("</section>", at);
  return html.slice(at, close);
}

describe("the add window", () => {
  test("usages and filters across the top, then the groups by usage, the public catalogue apart, the address last", async () => {
    // GIVEN the window open on the selection
    // WHEN it is rendered
    const html = await render({});

    // THEN the usages, the search, the source, the order and "no setup" lead,
    //      then each usage the built-in entries and the selection fill, in
    //      the order of the usages, then the public catalogue, then the address
    const order = [
      "catalogue-usages",
      "catalogue-search",
      "catalogue-source-verified",
      "catalogue-sort",
      "catalogue-keyless",
      "catalogue-usage-communication",
      "catalogue-usage-documents",
      "catalogue-usage-files",
      "catalogue-group-public",
      "catalogue-group-address",
    ];
    const at = order.map((id) => html.indexOf(`data-testid="${id}"`));
    expect(at.every((i) => i >= 0)).toBe(true);
    expect([...at].sort((a, b) => a - b)).toEqual(at);
    expect(html.match(/type="search"/g) ?? []).toHaveLength(1);
    expect(pressed(html, "catalogue-usage-chip-all")).toBe(true);
    expect(pressed(html, "catalogue-source-verified")).toBe(true);
    expect(html).toContain('data-testid="catalogue-source-verified"');
    // AND Notion sits under documents beside Google, Microsoft and the mailbox under communication
    const documents = section(html, "catalogue-usage-documents");
    expect(documents).toContain('data-testid="catalogue-native-google"');
    expect(documents).toContain('data-testid="catalogue-entry-com.notion/mcp"');
    expect(documents).toContain('data-testid="catalogue-badge-verified"');
    const communication = section(html, "catalogue-usage-communication");
    expect(communication).toContain('data-testid="catalogue-native-microsoft"');
    expect(communication).toContain('data-testid="catalogue-native-mailbox"');
    expect(section(html, "catalogue-usage-files")).toContain('data-testid="catalogue-folder"');
    expect(text(html)).toContain(en.connections.usage.documents);
    expect(text(html)).toContain(en.connections.add_window.groups.address);
  });

  test("the Google account shows its logo, Microsoft its generic icon", async () => {
    // GIVEN the window open on the selection
    // WHEN it is rendered
    const html = await render({});
    // THEN only Google carries a brand logo among the accounts
    expect(html).toContain('data-testid="catalogue-logo-native-google"');
    expect(html).not.toContain('data-testid="catalogue-logo-native-microsoft"');
  });

  test("under the selection, the public catalogue read stays apart and is not mixed into the usages", async () => {
    // GIVEN the public catalogue read, the source back on the selection, no words typed
    // WHEN the window is rendered
    const html = await render({ catalogueTier: "full", publicEntries: [WEATHER] });

    // THEN its entry is nowhere, and the public section offers to browse it
    expect(html).not.toContain('data-testid="catalogue-entry-io.github.someone/weather"');
    expect(section(html, "catalogue-group-public")).toContain('data-testid="catalogue-explore-all"');
  });

  test("with the public catalogue as source, its entries are gathered by usage and marked as community", async () => {
    // GIVEN the public catalogue read and chosen as source
    const browse = { usage: "all", source: "public", sort: "recommended", keyless: false };
    // WHEN the window is rendered
    const html = await render({ catalogueTier: "full", publicEntries: [{ ...WEATHER, category: null }], browse });

    // THEN the selection is gone, the entry is under "other", marked community,
    //      the note says nobody checked it, and the order is by name alone
    expect(html).not.toContain('data-testid="catalogue-entry-com.notion/mcp"');
    const other = section(html, "catalogue-usage-other");
    expect(other).toContain('data-testid="catalogue-entry-io.github.someone/weather"');
    expect(other).toContain('data-testid="catalogue-badge-community"');
    expect(html).toContain('data-testid="catalogue-public-note"');
    expect(html).not.toContain('data-testid="catalogue-sort"');
    expect(html).not.toContain('data-testid="catalogue-group-public"');
  });

  test("a usage nothing answers says so, with the public catalogue and the address to turn to", async () => {
    // GIVEN the usage "payments" chosen on the selection, which holds Notion alone
    const browse = { usage: "payments", source: "verified", sort: "recommended", keyless: false };
    // WHEN the window is rendered
    const html = await render({ browse });

    // THEN the empty state names the usage and offers both ways out
    const empty = html.slice(html.indexOf('data-testid="catalogue-empty"'));
    expect(text(empty)).toContain(en.connections.add_window.browse.empty_usage);
    expect(empty).toContain('data-testid="catalogue-empty-public"');
    expect(empty).toContain('data-testid="catalogue-empty-address"');
    expect(pressed(html, "catalogue-usage-chip-payments")).toBe(true);
  });

  test("a chosen entry opens its card beside the list: what it is, its usage, how it connects, what it asks, Add", async () => {
    // GIVEN the selection entry chosen
    // WHEN the window is rendered
    const html = await render({ chosen: "entry:com.notion/mcp" });
    const card = html.slice(html.indexOf('data-testid="catalogue-card"'));

    // THEN the card names it, says its usage and how it connects, what it asks, and offers Add and Cancel
    expect(text(card)).toContain("Notion");
    expect(text(card)).toContain("Pages and databases");
    expect(card).toContain('data-testid="catalogue-card-verified"');
    expect(text(card)).toContain(en.connections.usage.documents);
    expect(card).toContain('data-auth="keyless"');
    expect(card).toContain('data-testid="catalogue-card-needs"');
    expect(text(card)).toContain(en.connections.add_window.needs.online);
    expect(card).toContain('data-testid="catalogue-card-add"');
    expect(card).toContain('data-testid="catalogue-card-cancel"');
  });

  test("the card of an account says what the agents will be able to do with it", async () => {
    // GIVEN the Google account chosen
    // WHEN the window is rendered
    const html = await render({ chosen: "native:google" });
    const card = html.slice(html.indexOf('data-testid="catalogue-card"'));

    // THEN its services are listed, it connects by the account, and its logo is drawn
    expect(card).toContain('data-testid="catalogue-card-abilities"');
    expect(text(card)).toContain(en.connections.add_window.card.abilities_title);
    expect(card).toContain('data-auth="account"');
    expect(card).toContain('data-testid="catalogue-card-logo"');
  });
});

describe("a source that does not answer is a banner", () => {
  test("a failed selection shows the banner, not an empty catalogue", async () => {
    // GIVEN the selection could not be read
    // WHEN the window is rendered
    const html = await render({ selection: [], failures: ["selection"] });

    // THEN the banner says so, the built-in entries remain, nothing reads as empty
    expect(html).toContain('data-testid="catalogue-failure"');
    expect(text(html)).toContain(en.connections.add_window.failed_selection);
    expect(html).toContain('data-testid="catalogue-folder"');
    expect(text(html)).not.toContain("No suggestions yet");
    expect(text(html)).not.toContain("End of catalogue");
  });

  test("a failed public catalogue shows the banner and keeps Explore all to try again", async () => {
    // GIVEN Explore all answered by a failure
    // WHEN the window is rendered
    const html = await render({ failures: ["public"] });

    // THEN the banner says so and the button stays
    expect(text(html)).toContain(en.connections.add_window.failed_public);
    expect(html).toContain('data-testid="catalogue-explore-all"');
    expect(text(html)).not.toContain("End of catalogue");
  });
});
