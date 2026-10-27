import { describe, test, expect, vi, beforeEach } from "vitest";

// A wide window: every query of the layout answers as a 1440 px viewport. Set
// before the layout store is imported, since it reads the viewport at once.
vi.hoisted(() => {
  Object.defineProperty(globalThis, "matchMedia", {
    configurable: true,
    value: (query: string) => ({
      matches: /min-width/.test(query),
      media: query,
      addEventListener: () => {},
      removeEventListener: () => {},
    }),
  });
});

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => null) }));

import { flushSync, mount } from "svelte";
import { get } from "svelte/store";
import { locale, waitLocale } from "svelte-i18n";
import "$lib/i18n";
import { currentRoute } from "$lib/stores/navigation";
import { sidebarState } from "$lib/stores/layout";
import Topbar from "./Topbar.svelte";

let target: HTMLElement;

beforeEach(async () => {
  locale.set("en");
  await waitLocale("en");
  target = document.createElement("div");
  document.body.appendChild(target);
  mount(Topbar, { target });
  flushSync();
});

const fold = () => target.querySelector<HTMLButtonElement>('[data-testid="topbar-sidebar-fold"]');

function press(): void {
  fold()?.click();
  flushSync();
}

describe("the top bar folds the menu of a wide window", () => {
  test("on the chat as elsewhere, the menu folds and unfolds at the user's choice", () => {
    // GIVEN a wide window showing the chat, its menu unfolded
    currentRoute.set("chat");
    flushSync();
    expect(get(sidebarState)).toBe("expanded");
    expect(fold()?.getAttribute("aria-label")).toBe("Fold the menu");

    // WHEN the fold button is pressed
    press();

    // THEN the menu stands as a rail, and the button offers to unfold it
    expect(get(sidebarState)).toBe("icon");
    expect(fold()?.getAttribute("aria-label")).toBe("Unfold the menu");

    // AND the choice holds on another screen, and is undone by a second press
    currentRoute.set("agents");
    flushSync();
    expect(get(sidebarState)).toBe("icon");
    press();
    expect(get(sidebarState)).toBe("expanded");
  });
});
