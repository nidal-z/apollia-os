import { describe, test, expect, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => null) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

import { visiblePrimaryNav } from "./Sidebar.svelte";

// DOM rendering is exercised by the gestural E2E layer (vitest runs in `node`).
// These tests lock which primary entries the sidebar shows for each value of
// the developer mode.

function routes(developerMode: boolean): string[] {
  return visiblePrimaryNav(developerMode).map((entry) => entry.route);
}

describe("Sidebar - primary entries and the developer mode", () => {
  test("with the developer mode off, the AI screen is listed and Transcriptions is not", () => {
    // GIVEN the developer mode off
    const developerMode = false;

    // WHEN the sidebar computes its primary entries
    const shown = routes(developerMode);

    // THEN the AI screen is there for every user, and Transcriptions is hidden
    expect(shown).toContain("llm");
    expect(shown).not.toContain("transcriptions");
  });

  test("with the developer mode on, both the AI screen and Transcriptions are listed", () => {
    // GIVEN the developer mode on
    const developerMode = true;

    // WHEN the sidebar computes its primary entries
    const shown = routes(developerMode);

    // THEN Transcriptions joins the AI screen, right after it
    expect(shown).toContain("llm");
    expect(shown.indexOf("transcriptions")).toBe(shown.indexOf("llm") + 1);
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
