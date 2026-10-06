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

import { buildPaletteActions } from "./actions";
import { paletteGroups, setPaletteActions } from "./paletteIndex";
import { developerMode } from "$lib/stores/developerMode";

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

  test("only the technical pages carry the developer flag", () => {
    // GIVEN the real action catalogue
    const actions = buildPaletteActions();

    // WHEN the flagged actions are listed
    const flagged = actions.filter((action) => action.developer).map((action) => action.id);

    // THEN they are exactly Developer, Transcriptions, Memory and Observability
    expect(flagged.sort()).toEqual([...TECHNICAL].sort());
  });
});
