import { vi, describe, test, expect, beforeEach } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue(""),
}));

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { get } from "svelte/store";
import { detailSidebar, guideVisible } from "$lib/stores/detailSidebar";
import { guideContext, resetGuideSubscreen } from "$lib/stores/guideContext";
import { currentRoute } from "$lib/stores/navigation";
import { onboardingModalOpen } from "$lib/stores/onboarding";

// The top bar is rendered by the gestural layer (vitest runs in `node`). These
// tests lock what the help icon does and that it is written outside any
// condition, so it is on every screen.

/** What the help icon runs, as written in the top bar. */
const HELP_ACTION = "onclick={() => detailSidebar.toggleGuide()}";

function onTopbarHelp(): void {
  detailSidebar.toggleGuide();
}

const TOPBAR_SOURCE = readFileSync(
  fileURLToPath(new URL("./Topbar.svelte", import.meta.url)),
  "utf8",
);

beforeEach(() => {
  detailSidebar.reset();
  resetGuideSubscreen();
  onboardingModalOpen.set(false);
});

describe("Topbar - help icon", () => {
  test("opens the Guide tab on the current route", () => {
    // GIVEN the user on the Memory screen with the detail sidebar collapsed
    currentRoute.set("memory");
    expect(get(guideVisible)).toBe(false);

    // WHEN the help icon is pressed
    onTopbarHelp();

    // THEN the detail sidebar shows the Guide tab, on the Memory route
    expect(get(detailSidebar)).toEqual({ open: true, tab: "guide" });
    expect(get(guideVisible)).toBe(true);
    expect(get(guideContext).route).toBe("memory");
  });

  test("a second press collapses the Guide tab", () => {
    // GIVEN the Guide tab open from the help icon
    onTopbarHelp();

    // WHEN the icon is pressed again
    onTopbarHelp();

    // THEN the detail sidebar is collapsed
    expect(get(guideVisible)).toBe(false);
  });

  test("the icon carries a stable test id and is rendered unconditionally", () => {
    // GIVEN the top bar markup
    const markup = TOPBAR_SOURCE.slice(TOPBAR_SOURCE.indexOf("<header"));
    const button = markup.indexOf('data-testid="topbar-help"');

    // WHEN locating the help icon and the blocks opened before it
    const before = markup.slice(0, button);
    const opened = (before.match(/\{#if /g) ?? []).length;
    const closed = (before.match(/\{\/if\}/g) ?? []).length;

    // THEN its id is stable, it runs the help action, and no condition wraps it
    const element = markup.slice(markup.lastIndexOf("<button", button), button);
    expect(button).toBeGreaterThan(0);
    expect(element).toContain(HELP_ACTION);
    expect(opened).toBe(closed);
  });
});
