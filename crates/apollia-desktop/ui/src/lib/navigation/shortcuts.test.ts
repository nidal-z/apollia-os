/**
 * Keyboard shortcut registry tests.
 *
 * These tests lock in the structural invariants consumed by
 * `routes/settings/Shortcuts.svelte` and the Command Palette:
 *   - every entry has a platform combo string on both mac and win
 *   - every entry belongs to one of the known categories
 *   - `filterShortcuts` narrows the list correctly by description and
 *     by combo (e.g. searching "Cmd+K" isolates the palette row)
 *   - ids are unique across the registry
 */
import { describe, expect, it } from "vitest";
import {
  SHORTCUTS,
  SHORTCUT_CATEGORIES,
  filterShortcuts,
  splitCombo,
  comboFor,
  type ShortcutCategory,
} from "./shortcuts";

const identity = (key: string) => key;

describe("SHORTCUTS registry", () => {
  it("has a combo for every platform", () => {
    for (const entry of SHORTCUTS) {
      expect(entry.combo.mac, `entry ${entry.id}`).toBeTruthy();
      expect(entry.combo.win, `entry ${entry.id}`).toBeTruthy();
    }
  });

  it("every entry belongs to a known category", () => {
    const known = new Set<ShortcutCategory>(SHORTCUT_CATEGORIES);
    for (const entry of SHORTCUTS) {
      expect(known.has(entry.category), `unknown cat: ${entry.category}`).toBe(true);
    }
  });

  it("ids are unique", () => {
    const ids = SHORTCUTS.map((s) => s.id);
    expect(new Set(ids).size).toBe(ids.length);
  });
});

describe("filterShortcuts", () => {
  it("returns the full list for an empty query", () => {
    expect(filterShortcuts(SHORTCUTS, "", identity)).toEqual(SHORTCUTS);
    expect(filterShortcuts(SHORTCUTS, "   ", identity)).toEqual(SHORTCUTS);
  });

  it("matches on the raw combo string", () => {
    const result = filterShortcuts(SHORTCUTS, "⌘+K", identity, true);
    const ids = result.map((s) => s.id);
    expect(ids).toContain("palette.open");
  });

  it("Cmd+K query on windows matches Ctrl+K rows", () => {
    const result = filterShortcuts(SHORTCUTS, "Ctrl+K", identity, false);
    expect(result.map((r) => r.id)).toContain("palette.open");
  });

  it("matches description via translator", () => {
    const translator = (key: string) =>
      key.endsWith("chat_send") ? "Send the current message" : key;
    const result = filterShortcuts(SHORTCUTS, "send", translator);
    expect(result.map((r) => r.id)).toContain("chat.send");
  });

  it("requires every term to match (AND semantics)", () => {
    const result = filterShortcuts(SHORTCUTS, "palette something-that-will-not-match", identity);
    expect(result).toHaveLength(0);
  });
});

describe("splitCombo / comboFor", () => {
  it("splits multi-key chord", () => {
    expect(splitCombo("⌘+⇧+P")).toEqual(["⌘", "⇧", "P"]);
    expect(splitCombo("Ctrl+Shift+A")).toEqual(["Ctrl", "Shift", "A"]);
  });

  it("returns mac or win depending on platform flag", () => {
    const entry = SHORTCUTS.find((s) => s.id === "palette.open")!;
    expect(comboFor(entry, true)).toBe("⌘+K");
    expect(comboFor(entry, false)).toBe("Ctrl+K");
  });
});
