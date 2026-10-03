import { describe, test, expect, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => null) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

// The theme store reads the browser storage at import; node has none.
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

import { get } from "svelte/store";
import { DEFAULT_ROUTE, DEVELOPER_ROUTES, reachableRoute } from "./developerRoutes";
import { currentRoute } from "$lib/stores/navigation";
import { buildPaletteActions } from "$lib/command-palette/actions";

describe("developerRoutes - the router keeps the developer screens out of reach", () => {
  test.each(["observability", "transcriptions"] as const)(
    "with the developer mode off, %s resolves to the default route",
    (route) => {
      // GIVEN the developer mode off
      const developerMode = false;

      // WHEN the router resolves a request for a developer screen
      const shown = reachableRoute(route, developerMode);

      // THEN it renders the default route instead
      expect(shown).toBe(DEFAULT_ROUTE);
    },
  );

  test.each(["observability", "transcriptions"] as const)(
    "with the developer mode on, %s resolves to itself",
    (route) => {
      // GIVEN the developer mode on
      const developerMode = true;

      // WHEN the router resolves a request for a developer screen
      const shown = reachableRoute(route, developerMode);

      // THEN the screen is rendered
      expect(shown).toBe(route);
    },
  );

  test("with the developer mode off, an everyday screen is untouched", () => {
    // GIVEN the developer mode off
    // WHEN the router resolves the AI screen and the inbox
    const shown = [reachableRoute("llm", false), reachableRoute("inbox", false)];

    // THEN both render as asked
    expect(shown).toEqual(["llm", "inbox"]);
  });

  test("the default route is the one the application opens on", () => {
    // GIVEN the navigation store at import
    // WHEN its first value is read
    const first = get(currentRoute);

    // THEN it is the route the router falls back to
    expect(first).toBe(DEFAULT_ROUTE);
  });

  test("the palette flags every developer route, so it does not list them with the mode off", () => {
    // GIVEN the real palette catalogue
    const flagged = buildPaletteActions()
      .filter((action) => action.developer)
      .map((action) => action.id);

    // WHEN each developer route is looked up among the flagged pages
    const missing = DEVELOPER_ROUTES.filter((route) => !flagged.includes(`nav.${route}`));

    // THEN none is offered to a user with the mode off
    expect(missing).toEqual([]);
  });
});
