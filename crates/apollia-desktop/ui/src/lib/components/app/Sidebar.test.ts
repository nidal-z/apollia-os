import { describe, test, expect, vi } from "vitest";
import { render as renderToString } from "svelte/server";
import { locale, waitLocale } from "svelte-i18n";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => null) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

// The layout and theme stores read the browser storage at import; node has none.
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
import Sidebar, { visibleDeveloperNav, visiblePrimaryNav } from "./Sidebar.svelte";
import { developerMode } from "$lib/stores/developerMode";
import { DEVELOPER_ROUTES } from "$lib/navigation/developerRoutes";

// The entry lists are locked through the module helpers, and the placement of
// the developer group through Svelte's server renderer, the only renderer the
// node environment of this suite offers.

function routes(developerMode: boolean): string[] {
  return visiblePrimaryNav(developerMode).map((entry) => entry.route);
}

function developerRoutes(developerMode: boolean): string[] {
  return visibleDeveloperNav(developerMode).map((entry) => entry.route);
}

async function renderSidebar(on: boolean): Promise<string> {
  locale.set("en");
  await waitLocale("en");
  developerMode.set(on);
  return renderToString(Sidebar).body;
}

describe("Sidebar - primary entries and the developer mode", () => {
  test("with the developer mode off, the AI screen is listed and no technical screen is", () => {
    // GIVEN the developer mode off
    const developerMode = false;

    // WHEN the sidebar computes its entries
    const shown = [...routes(developerMode), ...developerRoutes(developerMode)];

    // THEN the AI screen is there for every user, Transcriptions and
    // Observability are not
    expect(shown).toContain("llm");
    expect(shown).not.toContain("transcriptions");
    expect(shown).not.toContain("observability");
  });

  test("Observability and Transcriptions never sit among the primary entries", () => {
    // GIVEN the developer mode on, where every primary entry is shown
    const developerMode = true;

    // WHEN the sidebar computes its primary entries
    const shown = routes(developerMode);

    // THEN the two technical screens are not part of them
    expect(shown).toContain("llm");
    expect(shown).not.toContain("observability");
    expect(shown).not.toContain("transcriptions");
  });

  test("with the developer mode on, the developer group carries Transcriptions, Observability, then Memory", () => {
    // GIVEN the developer mode on
    const developerMode = true;

    // WHEN the sidebar computes its developer group
    const group = developerRoutes(developerMode);

    // THEN it holds the three technical screens, in that order, and exactly the
    // routes the router keeps out of reach with the mode off
    expect(group).toEqual(["transcriptions", "observability", "memory"]);
    expect(group).toEqual([...DEVELOPER_ROUTES]);
  });

  test("the Memory screen is never a primary entry, mode off or on", () => {
    // GIVEN the developer mode off, then on
    // WHEN the sidebar computes its primary entries
    const off = routes(false);
    const on = routes(true);

    // THEN the Memory screen is among neither: it only lives in the developer
    // group, and with the mode off it is nowhere in the sidebar
    expect(off).not.toContain("memory");
    expect(on).not.toContain("memory");
    expect(developerRoutes(false)).not.toContain("memory");
  });

  test("the developer group holds no developer server entry", () => {
    // GIVEN the developer mode on
    // WHEN the developer group is computed
    const group = developerRoutes(true);

    // THEN the Developer screen is not one of its entries: it belongs to the
    // primary rank
    expect(group).not.toContain("developer");
  });

  test("the developer mode only adds entries, it never removes one", () => {
    // GIVEN the entries shown with the mode off
    const off = routes(false);

    // WHEN the mode is turned on
    const on = routes(true);

    // THEN every entry of the default sidebar is still there
    expect(off.every((route) => on.includes(route))).toBe(true);
  });
});

describe("Sidebar - the rendered developer group", () => {
  test("with the developer mode off, the group and its three entries are absent", async () => {
    // GIVEN the developer mode off
    // WHEN the sidebar is rendered
    const html = await renderSidebar(false);

    // THEN neither the group nor its entries are in the markup
    expect(html).toContain('data-testid="nav-settings"');
    expect(html).not.toContain('data-testid="sidebar-developer-group"');
    expect(html).not.toContain('data-testid="nav-observability"');
    expect(html).not.toContain('data-testid="nav-transcriptions"');
    expect(html).not.toContain('data-testid="nav-memory"');
  });

  test("with the developer mode on, the group follows the secondary entries", async () => {
    // GIVEN the developer mode on
    // WHEN the sidebar is rendered
    const html = await renderSidebar(true);

    // THEN the group comes after Connections and Settings, and holds
    // Transcriptions, Observability, then Memory
    const settings = html.indexOf('data-testid="nav-settings"');
    const group = html.indexOf('data-testid="sidebar-developer-group"');
    const transcriptions = html.indexOf('data-testid="nav-transcriptions"');
    const observability = html.indexOf('data-testid="nav-observability"');
    expect(settings).toBeGreaterThan(-1);
    expect(group).toBeGreaterThan(settings);
    expect(transcriptions).toBeGreaterThan(group);
    expect(observability).toBeGreaterThan(transcriptions);
    expect(html.indexOf('data-testid="nav-memory"')).toBeGreaterThan(observability);
    expect(html).toContain('aria-label="Developer mode"');
  });
});
