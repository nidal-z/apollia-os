import { describe, test, expect, beforeEach, vi } from "vitest";
import { get } from "svelte/store";

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

// The screen a palette entry opens is read off the call it makes.
const opened = vi.hoisted(() => ({ routes: [] as string[] }));
vi.mock("$lib/stores/navigation", async (original) => ({
  ...(await original<typeof import("$lib/stores/navigation")>()),
  navigateTo: (route: string) => void opened.routes.push(route),
}));

import { buildPaletteActions } from "./actions";
import { paletteGroups, setPaletteActions } from "./paletteIndex";
import { developerMode } from "$lib/stores/developerMode";
import { clearRecentActions } from "$lib/stores/recentActionsStore";
import { visibleSidebarRoutes } from "$lib/components/app/Sidebar.svelte";

/** Every item id the palette offers, all groups together. */
function offeredIds(): string[] {
  return get(paletteGroups).flatMap((group) => group.items.map((item) => item.id));
}

const TECHNICAL = ["nav.developer", "nav.transcriptions", "nav.memory", "nav.observability"];
const FOR_EVERYONE = ["nav.chat", "nav.llm", "agents.create", "automations.create", "chat.new"];

describe("paletteIndex - the developer mode filters the technical actions", () => {
  beforeEach(() => {
    setPaletteActions(buildPaletteActions());
  });

  test("with the developer mode off, the technical pages are hidden and the rest is offered", () => {
    // GIVEN the real action catalogue and the developer mode off
    developerMode.set(false);

    // WHEN the palette computes its groups
    const ids = offeredIds();

    // THEN LLM, the chat and the creation actions are offered, the
    // technical pages are not
    for (const id of FOR_EVERYONE) expect(ids).toContain(id);
    for (const id of TECHNICAL) expect(ids).not.toContain(id);
  });

  test("with the developer mode on, the technical pages join the rest", () => {
    // GIVEN the real action catalogue and the developer mode on
    developerMode.set(true);

    // WHEN the palette computes its groups
    const ids = offeredIds();

    // THEN everything is offered, the technical pages included
    for (const id of [...FOR_EVERYONE, ...TECHNICAL]) expect(ids).toContain(id);
  });

  test("only the four technical pages carry the developer flag", () => {
    // GIVEN the real action catalogue
    const actions = buildPaletteActions();

    // WHEN the flagged actions are listed
    const flagged = actions.filter((action) => action.developer).map((action) => action.id);

    // THEN they are exactly the developer screen, Transcriptions, Memory and
    // Observability
    expect(flagged.sort()).toEqual([...TECHNICAL].sort());
  });
});

/** The screens the palette offers, in its order, each read by opening it. */
function screensOffered(): string[] {
  const pages = get(paletteGroups).find((group) =>
    group.items.some((item) => item.id === "nav.chat"),
  );
  opened.routes = [];
  for (const item of pages?.items ?? []) void item.action();
  return [...opened.routes];
}

describe("paletteIndex - the screens offered are the ones of the sidebar", () => {
  beforeEach(() => {
    clearRecentActions();
    setPaletteActions(buildPaletteActions());
  });

  test.each([false, true])(
    "with the developer mode at %s, the palette opens the screens the sidebar lists, in its order",
    (mode) => {
      // GIVEN the real action catalogue and a developer mode
      developerMode.set(mode);

      // WHEN every screen entry of the palette is opened
      const screens = screensOffered();

      // THEN they are the screens of the sidebar for that mode, no more, no less
      expect(screens).toEqual(visibleSidebarRoutes(mode));
    },
  );

  test("no retired or hidden screen is offered, in either mode", () => {
    for (const mode of [false, true]) {
      // GIVEN the real action catalogue and a developer mode
      clearRecentActions();
      developerMode.set(mode);

      // WHEN the screens offered are listed
      const screens = screensOffered();

      // THEN the home, the task list, the notifications, the projects and the
      // organisation policy are not among them
      for (const gone of ["dashboard", "tasks", "notifications", "projects", "policy"]) {
        expect(screens).not.toContain(gone);
      }
    }
  });

  test("the technical screens are offered with the developer mode on only", () => {
    // GIVEN the developer mode off
    developerMode.set(false);

    // WHEN the screens offered are listed, then listed again with the mode on
    const off = screensOffered();
    clearRecentActions();
    developerMode.set(true);
    const on = screensOffered();

    // THEN Memory, Observability and Transcriptions come with the mode on only
    for (const technical of ["memory", "observability", "transcriptions"]) {
      expect(off).not.toContain(technical);
      expect(on).toContain(technical);
    }
  });
});
