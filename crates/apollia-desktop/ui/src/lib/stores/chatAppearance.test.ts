import { describe, test, expect } from "vitest";
import { get } from "svelte/store";
import {
  CHAT_APPEARANCE_DEFAULTS,
  CHAT_FONT_SIZE_MAX,
  CHAT_FONT_SIZE_MIN,
  chatAppearance,
  chatDensity,
  chatFullWidth,
  parseChatAppearance,
  reasoningExpandedByDefault,
  resetChatFontSize,
  setChatAppearance,
} from "./chatAppearance";

describe("chatAppearance - defaults", () => {
  test("a first launch reads the defaults the Appearance mockup sets", () => {
    // GIVEN nothing stored
    const stored = null;

    // WHEN the preference is read
    const a = parseChatAppearance(stored);

    // THEN width and reasoning are off, density comfortable, size on the middle rung
    expect(a).toEqual(CHAT_APPEARANCE_DEFAULTS);
    expect(a.fullWidth).toBe(false);
    expect(a.expandReasoning).toBe(false);
    expect(a.density).toBe("comfortable");
    expect(a.fontWeight).toBe("normal");
    expect(a.fontSize).toBe(3);
  });

  test("the size slider has at least three rungs", () => {
    // GIVEN the declared bounds
    // WHEN counting the rungs between them
    const rungs = CHAT_FONT_SIZE_MAX - CHAT_FONT_SIZE_MIN + 1;

    // THEN there are at least three
    expect(rungs).toBeGreaterThanOrEqual(3);
  });
});

describe("chatAppearance - persistence", () => {
  test("a stored choice reads back unchanged", () => {
    // GIVEN every field moved off its default, serialized as the store writes it
    const chosen = {
      fontSize: 5,
      fontWeight: "medium",
      fullWidth: true,
      density: "compact",
      expandReasoning: true,
    } as const;

    // WHEN the serialized value is read at the next start
    const a = parseChatAppearance(JSON.stringify(chosen));

    // THEN every choice survives
    expect(a).toEqual(chosen);
  });

  test("a corrupt or out-of-range field falls back alone", () => {
    // GIVEN a size outside the rungs, an unknown weight, and a valid density
    const raw = JSON.stringify({ fontSize: 9, fontWeight: "black", density: "compact" });

    // WHEN read
    const a = parseChatAppearance(raw);

    // THEN only the bad fields return to their defaults
    expect(a.fontSize).toBe(CHAT_APPEARANCE_DEFAULTS.fontSize);
    expect(a.fontWeight).toBe(CHAT_APPEARANCE_DEFAULTS.fontWeight);
    expect(a.density).toBe("compact");
    expect(parseChatAppearance("{not json")).toEqual(CHAT_APPEARANCE_DEFAULTS);
  });
});

describe("chatAppearance - setters", () => {
  test("the projections follow the store, and Reset restores the size alone", () => {
    // GIVEN a reader who moved every control
    setChatAppearance({
      fontSize: 1,
      fontWeight: "medium",
      fullWidth: true,
      density: "compact",
      expandReasoning: true,
    });

    // WHEN Reset is pressed on the size
    resetChatFontSize();

    // THEN the size is back on its default and nothing else moved
    const a = get(chatAppearance);
    expect(a.fontSize).toBe(CHAT_APPEARANCE_DEFAULTS.fontSize);
    expect(a.fontWeight).toBe("medium");
    expect(get(chatDensity)).toBe("compact");
    expect(get(chatFullWidth)).toBe(true);
    expect(get(reasoningExpandedByDefault)).toBe(true);
    chatAppearance.set({ ...CHAT_APPEARANCE_DEFAULTS });
  });
});
