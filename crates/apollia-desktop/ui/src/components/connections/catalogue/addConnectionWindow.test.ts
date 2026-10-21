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
      onexploreall: noop, onbacktoselection: noop, onadd: noop, onmanage: noop, oninstalled: noop,
      ...props,
    } as never,
  }).body;
}

function text(html: string): string {
  return html.replace(/<!--[\s\S]*?-->/g, " ").replace(/<[^>]+>/g, " ").replace(/\s+/g, " ");
}

describe("the add window", () => {
  test("one search, then the four groups in order", async () => {
    // GIVEN the window open on the selection
    // WHEN it is rendered
    const html = await render({});

    // THEN one search field leads the groups, in their order
    const order = ["catalogue-search", "catalogue-group-apollia", "catalogue-group-computer", "catalogue-group-public", "catalogue-group-address"];
    const at = order.map((id) => html.indexOf(`data-testid="${id}"`));
    expect(at.every((i) => i >= 0)).toBe(true);
    expect([...at].sort((a, b) => a - b)).toEqual(at);
    expect(html.match(/type="search"/g) ?? []).toHaveLength(1);
    expect(text(html)).toContain(en.connections.add_window.groups.apollia);
    expect(text(html)).toContain(en.connections.add_window.groups.address);
  });

  test("the public catalogue, once read, is marked as not verified", async () => {
    // GIVEN the public catalogue read
    // WHEN the window is rendered
    const html = await render({ catalogueTier: "full", publicEntries: [WEATHER] });
    const publicGroup = html.slice(html.indexOf('data-testid="catalogue-group-public"'), html.indexOf('data-testid="catalogue-group-address"'));

    // THEN its entry is in the public group, which says it is not verified
    expect(publicGroup).toContain('data-testid="catalogue-entry-io.github.someone/weather"');
    expect(text(publicGroup)).toContain(en.connections.add_window.public_unverified);
    expect(text(publicGroup)).toContain(en.connections.add_window.public_note);
  });

  test("a chosen entry opens its card in the centre: what it is, what it asks, Add", async () => {
    // GIVEN the selection entry chosen
    // WHEN the window is rendered
    const html = await render({ chosen: "entry:com.notion/mcp" });
    const card = html.slice(html.indexOf('data-testid="catalogue-card"'));

    // THEN the card names it, says what it asks, and offers Add and Cancel
    expect(text(card)).toContain("Notion");
    expect(text(card)).toContain("Pages and databases");
    expect(card).toContain('data-testid="catalogue-card-needs"');
    expect(text(card)).toContain(en.connections.add_window.needs.online);
    expect(card).toContain('data-testid="catalogue-card-add"');
    expect(card).toContain('data-testid="catalogue-card-cancel"');
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
