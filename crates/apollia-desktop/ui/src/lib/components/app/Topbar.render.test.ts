import { describe, test, expect, vi, afterEach } from "vitest";
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
import Topbar from "./Topbar.svelte";
import { keycapsFor } from "$lib/navigation/shortcuts";
import { tasks } from "$lib/stores/sse";

// The top bar is drawn by Svelte's server renderer, the only renderer the node
// environment of this suite offers; the system is simulated through the
// `navigator` the component reads.

async function renderTopbar(platform: string): Promise<string> {
  vi.stubGlobal("navigator", { platform, userAgent: "" });
  locale.set("en");
  await waitLocale("en");
  return renderToString(Topbar).body;
}

/** The keys drawn in the search bar, in order. */
function searchKeys(html: string): string[] {
  const kbd = /<kbd[^>]*data-testid="topbar-search-shortcut"[^>]*>([\s\S]*?)<\/kbd>/.exec(html);
  return [...(kbd?.[1] ?? "").matchAll(/<span[^>]*>([^<]*)<\/span>/g)].map((m) => m[1]);
}

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("Topbar - what the bar holds", () => {
  test("the review icon and the settings wheel are gone, the rest stays", async () => {
    // GIVEN the top bar on any system, with one agent at work
    tasks.set([{ status: "working" }] as never);

    // WHEN it is drawn
    const html = await renderTopbar("MacIntel");
    tasks.set([]);

    // THEN neither duplicate of a sidebar entry is drawn, and the search, the
    // activity indicator, the help icon, the detail sidebar icon and the user
    // menu are
    expect(html).not.toContain('data-testid="topbar-inbox"');
    expect(html).not.toContain('data-testid="topbar-settings"');
    for (const kept of [
      "topbar-search",
      "topbar-agents-at-work",
      "topbar-help",
      "topbar-detail-sidebar-toggle",
      "topbar-user-menu",
    ]) {
      expect(html).toContain(`data-testid="${kept}"`);
    }
  });
});

describe("Topbar - the palette shortcut follows the system", () => {
  test("on macOS the search bar shows ⌘ K", async () => {
    // GIVEN a Mac
    // WHEN the top bar is drawn
    const html = await renderTopbar("MacIntel");

    // THEN the search bar shows the command key
    expect(searchKeys(html)).toEqual(["⌘", "K"]);
  });

  test.each(["Win32", "Linux x86_64"])("on %s the search bar shows Ctrl K", async (platform) => {
    // GIVEN a system that is not a Mac
    // WHEN the top bar is drawn
    const html = await renderTopbar(platform);

    // THEN the search bar shows the control key, never the command key
    expect(searchKeys(html)).toEqual(["Ctrl", "K"]);
    expect(html).not.toContain("⌘");
  });

  test("the keys come from the shortcut registry, per system", () => {
    // GIVEN the registered shortcut that opens the palette
    // WHEN its keys are asked for each system
    // THEN they differ by the modifier only, and an unknown id has none
    expect(keycapsFor("palette.open", true)).toEqual(["⌘", "K"]);
    expect(keycapsFor("palette.open", false)).toEqual(["Ctrl", "K"]);
    expect(keycapsFor("no.such.shortcut", true)).toEqual([]);
  });
});
