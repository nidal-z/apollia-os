/**
 * Unit tests for the hotkey capture parser.
 *
 * Covers: `KeyboardEvent` → canonical combo string, missing-modifier
 * detection, and collision detection against reserved OS/app shortcuts.
 */
import { describe, it, expect } from "vitest";
import {
  parseHotkeyEvent,
  isValidHotkey,
  detectCollision,
  formatCombo,
  hotkeyChips,
  RESERVED_COMBOS,
} from "./hotkeyCapture";

function makeEvent(init: {
  key: string;
  code?: string;
  ctrlKey?: boolean;
  shiftKey?: boolean;
  altKey?: boolean;
  metaKey?: boolean;
}): KeyboardEvent {
  // We run in a node environment — synthesize a plain object that satisfies
  // the subset of `KeyboardEvent` that the parser reads.
  return {
    key: init.key,
    code: init.code ?? init.key,
    ctrlKey: !!init.ctrlKey,
    shiftKey: !!init.shiftKey,
    altKey: !!init.altKey,
    metaKey: !!init.metaKey,
  } as unknown as KeyboardEvent;
}

describe("parseHotkeyEvent", () => {
  // GIVEN Cmd+Shift+P pressed
  // WHEN parsed
  // THEN combo = "meta+shift+p" and the hotkey is valid
  it("parses a modifier + letter combo", () => {
    const evt = makeEvent({ key: "p", code: "KeyP", metaKey: true, shiftKey: true });
    const parsed = parseHotkeyEvent(evt);
    expect(parsed.modifiers).toEqual(["shift", "meta"]);
    expect(parsed.key).toBe("p");
    expect(parsed.combo).toBe("shift+meta+p");
    expect(isValidHotkey(parsed)).toBe(true);
  });

  // GIVEN only a modifier held
  // WHEN parsed
  // THEN the hotkey is not valid (no main key yet)
  it("returns no main key while only a modifier is held", () => {
    const evt = makeEvent({ key: "Meta", code: "MetaLeft", metaKey: true });
    const parsed = parseHotkeyEvent(evt);
    expect(parsed.key).toBeNull();
    expect(isValidHotkey(parsed)).toBe(false);
  });

  // GIVEN a letter pressed with no modifier
  // WHEN parsed
  // THEN isValidHotkey is false (missing modifier)
  it("flags missing-modifier combos as invalid", () => {
    const evt = makeEvent({ key: "a", code: "KeyA" });
    const parsed = parseHotkeyEvent(evt);
    expect(parsed.modifiers).toEqual([]);
    expect(parsed.key).toBe("a");
    expect(isValidHotkey(parsed)).toBe(false);
  });

  it("normalizes Space / digits / arrows / function keys", () => {
    expect(parseHotkeyEvent(makeEvent({ key: " ", code: "Space", metaKey: true })).combo).toBe("meta+space");
    expect(parseHotkeyEvent(makeEvent({ key: "1", code: "Digit1", ctrlKey: true })).combo).toBe("ctrl+1");
    expect(parseHotkeyEvent(makeEvent({ key: "ArrowUp", code: "ArrowUp", altKey: true })).combo).toBe("alt+up");
    expect(parseHotkeyEvent(makeEvent({ key: "F5", code: "F5", ctrlKey: true })).combo).toBe("ctrl+f5");
  });
});

describe("detectCollision", () => {
  it("flags canonical reserved combos", () => {
    for (const combo of RESERVED_COMBOS) {
      expect(detectCollision(combo)).toBe(true);
    }
  });

  // GIVEN Cmd+C collision
  // WHEN checked
  // THEN collision detection returns true
  it("detects Cmd+C collision from a parsed event", () => {
    const evt = makeEvent({ key: "c", code: "KeyC", metaKey: true });
    const parsed = parseHotkeyEvent(evt);
    expect(detectCollision(parsed.combo)).toBe(true);
  });

  it("does not flag safe combos", () => {
    expect(detectCollision("meta+shift+space")).toBe(false);
    expect(detectCollision("ctrl+alt+p")).toBe(false);
    expect(detectCollision("")).toBe(false);
  });
});

describe("formatCombo / hotkeyChips", () => {
  it("formats on mac with glyphs and key names", () => {
    expect(formatCombo("meta+shift+space", "mac")).toBe("⌘⇧Space");
    expect(formatCombo("meta+c", "mac")).toBe("⌘C");
  });

  it("formats on non-mac with plus-separated names", () => {
    expect(formatCombo("ctrl+shift+p", "other")).toBe("Ctrl+Shift+P");
  });

  it("builds one chip per modifier plus the key", () => {
    const parsed = parseHotkeyEvent(
      makeEvent({ key: "p", code: "KeyP", ctrlKey: true, shiftKey: true }),
    );
    expect(hotkeyChips(parsed, "other")).toEqual(["Ctrl", "Shift", "P"]);
    expect(hotkeyChips(parsed, "mac")).toEqual(["⌃", "⇧", "P"]);
  });
});
