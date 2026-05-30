/**
 * Tests for the chord-matching primitive that underpins every chat
 * shortcut. They cover the layout-safety promise (`event.code` instead
 * of `event.key`) and the "no silent extra modifiers" rule.
 *
 * Stays DOM-free so the suite runs under the default `node` Vitest
 * environment (no jsdom dependency).
 */
import { describe, it, expect } from "vitest";
import { chordMatches } from "./chatShortcuts";
import type { Chord } from "./chatShortcuts";

interface FakeKeyEvent {
  code: string;
  key?: string;
  metaKey?: boolean;
  ctrlKey?: boolean;
  shiftKey?: boolean;
  altKey?: boolean;
}

function evt(init: FakeKeyEvent): KeyboardEvent {
  // Cast through unknown — `chordMatches` only reads the listed fields.
  return init as unknown as KeyboardEvent;
}

// navigator.platform is deprecated but still the most reliable Mac signal.
const MOD_FIELD: keyof FakeKeyEvent =
  typeof navigator !== "undefined" &&
  ((navigator as Navigator & { platform?: string }).platform ?? "").toLowerCase().includes("mac") // NOSONAR typescript:S1874
    ? "metaKey"
    : "ctrlKey";

const EMPTY_EVT: Readonly<FakeKeyEvent> = { code: "" };
function mod(extra: FakeKeyEvent = EMPTY_EVT): FakeKeyEvent {
  return { ...extra, [MOD_FIELD]: true };
}

describe("chordMatches", () => {
  it("matches when the code and required modifiers are present", () => {
    const chord: Chord = { code: "KeyK", modifiers: ["mod"] };
    expect(chordMatches(evt(mod({ code: "KeyK" })), chord)).toBe(true);
  });

  it("ignores event.key differences (AZERTY/QWERTY safety)", () => {
    const chord: Chord = { code: "KeyK", modifiers: ["mod"] };
    expect(
      chordMatches(evt(mod({ code: "KeyK", key: "k" })), chord),
    ).toBe(true);
  });

  it("rejects when a required modifier is missing", () => {
    expect(
      chordMatches(evt({ code: "KeyK" }), {
        code: "KeyK",
        modifiers: ["mod"],
      }),
    ).toBe(false);
  });

  it("rejects when an extra modifier is pressed", () => {
    expect(
      chordMatches(evt(mod({ code: "KeyK", shiftKey: true })), {
        code: "KeyK",
        modifiers: ["mod"],
      }),
    ).toBe(false);
  });

  it("respects explicit `forbid` set", () => {
    expect(
      chordMatches(evt({ code: "KeyJ" }), {
        code: "KeyJ",
        forbid: ["mod", "shift", "alt"],
      }),
    ).toBe(true);
    expect(
      chordMatches(evt({ code: "KeyJ", shiftKey: true }), {
        code: "KeyJ",
        forbid: ["mod", "shift", "alt"],
      }),
    ).toBe(false);
  });

  it("matches combined modifiers in any pressing order", () => {
    expect(
      chordMatches(evt(mod({ code: "KeyA", shiftKey: true })), {
        code: "KeyA",
        modifiers: ["shift", "mod"],
      }),
    ).toBe(true);
  });

  it("rejects different codes even when modifiers match", () => {
    expect(
      chordMatches(evt(mod({ code: "KeyL" })), {
        code: "KeyK",
        modifiers: ["mod"],
      }),
    ).toBe(false);
  });
});
