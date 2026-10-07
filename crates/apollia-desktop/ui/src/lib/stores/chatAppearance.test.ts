import { describe, test, expect } from "vitest";
import { get } from "svelte/store";
import {
  CHAT_APPEARANCE_DEFAULTS,
  CHAT_FONT_SIZE_MAX,
  CHAT_FONT_SIZE_MIN,
  CHAT_APPEARANCE_STORAGE_KEY,
  applyChatAppearance,
  chatAppearance,
  chatMessageStyle,
  chatViewMode,
  generationInfoScope,
  navPosition,
  reasoningVignette,
  chatDensity,
  chatFullWidth,
  parseChatAppearance,
  reasoningExpandedByDefault,
  resetChatFontSize,
  setChatAppearance,
  paneScrollbar,
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
    expect(a).toEqual({ ...CHAT_APPEARANCE_DEFAULTS, ...chosen });
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

describe("chatAppearance - navigation, display and scroll choices", () => {
  test("a first launch reads the defaults of the Appearance screen", () => {
    // GIVEN nothing stored
    // WHEN the preference is read
    const a = parseChatAppearance(null);

    // THEN the navigation is on the left, answers are rendered, the figures sit
    // under the last message only, both scroll switches and the vignette are
    // on, and messages are bubbles
    expect(a.navPosition).toBe("left");
    expect(a.viewMode).toBe("markdown");
    expect(a.generationInfo).toBe("last");
    expect(a.scrollSentToTop).toBe(true);
    expect(a.followGeneration).toBe(true);
    expect(a.messageStyle).toBe("bubble");
    expect(a.reasoningVignette).toBe(true);
  });

  test("every choice survives a restart", () => {
    // GIVEN every choice moved off its default and written the way the store
    // writes it
    const chosen = {
      navPosition: "right",
      viewMode: "plain",
      tabScrollbar: true,
      generationInfo: "all",
      scrollSentToTop: false,
      followGeneration: false,
      messageStyle: "block",
      reasoningVignette: false,
    } as const;
    const written = new Map<string, string>();
    const real = globalThis.localStorage;
    globalThis.localStorage = {
      getItem: (k: string) => written.get(k) ?? null,
      setItem: (k: string, v: string) => void written.set(k, v),
    } as Storage;
    applyChatAppearance({ ...CHAT_APPEARANCE_DEFAULTS, ...chosen });
    globalThis.localStorage = real;

    // WHEN the stored entry is read at the next start
    const a = parseChatAppearance(written.get(CHAT_APPEARANCE_STORAGE_KEY) ?? null);

    // THEN each one reads back as chosen
    expect(a).toEqual({ ...CHAT_APPEARANCE_DEFAULTS, ...chosen });
  });

  test("a value the screen does not offer falls back alone", () => {
    // GIVEN a stored entry with a side and a style that do not exist, and a
    // valid view mode
    const raw = JSON.stringify({ navPosition: "top", messageStyle: "card", viewMode: "plain" });

    // WHEN read
    const a = parseChatAppearance(raw);

    // THEN the two unknown values return to their defaults, the valid one stays
    expect(a.navPosition).toBe("left");
    expect(a.messageStyle).toBe("bubble");
    expect(a.viewMode).toBe("plain");
  });

  test("the projections follow the store", () => {
    // GIVEN a reader who moves the navigation, the view mode, the figures, the
    // style and the vignette
    setChatAppearance({
      navPosition: "right",
      viewMode: "plain",
      generationInfo: "all",
      messageStyle: "block",
      reasoningVignette: false,
    });

    // WHEN each reader is asked
    // THEN each answers the new value
    expect(get(navPosition)).toBe("right");
    expect(get(chatViewMode)).toBe("plain");
    expect(get(generationInfoScope)).toBe("all");
    expect(get(chatMessageStyle)).toBe("block");
    expect(get(reasoningVignette)).toBe(false);
    chatAppearance.set({ ...CHAT_APPEARANCE_DEFAULTS });
  });
});

describe("chatAppearance - the scroll bar of the split chat", () => {
  test("it is off until the reader asks for it, and the reader follows the store", () => {
    // GIVEN nothing stored
    const fresh = parseChatAppearance(null);

    // WHEN the reader turns the bar on
    const before = get(paneScrollbar);
    setChatAppearance({ tabScrollbar: true });
    const after = get(paneScrollbar);
    chatAppearance.set({ ...CHAT_APPEARANCE_DEFAULTS });

    // THEN it was off by default and the projection answers the new value
    expect(fresh.tabScrollbar).toBe(false);
    expect(before).toBe(false);
    expect(after).toBe(true);
  });

  test("a stored value that is not a switch falls back to off", () => {
    // GIVEN a stored entry whose bar is a word
    const raw = JSON.stringify({ tabScrollbar: "always" });

    // WHEN read
    // THEN the bar is off
    expect(parseChatAppearance(raw).tabScrollbar).toBe(false);
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
