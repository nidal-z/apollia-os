import { vi, describe, test, expect, beforeEach, afterEach } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue(""),
}));

import { get } from "svelte/store";
import { registerGlobalShortcutHandlers } from "$lib/navigation/globalShortcutHandlers";
import { dispatchShortcutEvent } from "$lib/navigation/shortcutDispatcher";
import { SHORTCUTS } from "$lib/navigation/shortcuts";
import { detailSidebar, guideVisible } from "$lib/stores/detailSidebar";

interface Chord {
  key: string;
  metaKey?: boolean;
  ctrlKey?: boolean;
  targetTag?: string;
}

/** A keydown as the dispatcher reads it, without a DOM. */
function keydown(chord: Chord): KeyboardEvent {
  return {
    key: chord.key,
    metaKey: chord.metaKey ?? false,
    ctrlKey: chord.ctrlKey ?? false,
    altKey: false,
    shiftKey: false,
    target: chord.targetTag ? { tagName: chord.targetTag, isContentEditable: false } : null,
    preventDefault: vi.fn(),
  } as unknown as KeyboardEvent;
}

let dispose: () => void = () => {};

beforeEach(() => {
  detailSidebar.reset();
  dispose = registerGlobalShortcutHandlers();
});

afterEach(() => {
  dispose();
  vi.unstubAllGlobals();
});

describe("globalShortcutHandlers - Cmd+/ opens the guide", () => {
  test("the chord is declared for both platforms", () => {
    // GIVEN the shortcut catalog
    // WHEN reading the guide entry
    const entry = SHORTCUTS.find((s) => s.id === "companion.toggle");

    // THEN it is Cmd+/ on macOS and Ctrl+/ elsewhere
    expect(entry?.combo).toEqual({ mac: "⌘+/", win: "Ctrl+/" });
  });

  test("Ctrl+/ outside macOS opens the Guide tab", () => {
    // GIVEN a non-Apple platform and the detail sidebar collapsed
    vi.stubGlobal("navigator", { platform: "Win32", userAgent: "Windows" });
    expect(get(guideVisible)).toBe(false);

    // WHEN Ctrl+/ is pressed
    const event = keydown({ key: "/", ctrlKey: true });
    dispatchShortcutEvent(event);

    // THEN the Guide tab shows and the browser default is suppressed
    expect(get(guideVisible)).toBe(true);
    expect(event.preventDefault).toHaveBeenCalled();
  });

  test("Cmd+/ on macOS opens the Guide tab, even from a text field", () => {
    // GIVEN macOS, focus in the chat input
    vi.stubGlobal("navigator", { platform: "MacIntel", userAgent: "Macintosh" });

    // WHEN Cmd+/ is pressed
    dispatchShortcutEvent(keydown({ key: "/", metaKey: true, targetTag: "TEXTAREA" }));

    // THEN the Guide tab shows
    expect(get(guideVisible)).toBe(true);
  });

  test("the chord collapses the Guide tab it opened", () => {
    // GIVEN the Guide tab open
    vi.stubGlobal("navigator", { platform: "MacIntel", userAgent: "Macintosh" });
    detailSidebar.openGuide();

    // WHEN Cmd+/ is pressed
    dispatchShortcutEvent(keydown({ key: "/", metaKey: true }));

    // THEN the detail sidebar is collapsed
    expect(get(guideVisible)).toBe(false);
  });

  test("a plain slash does nothing", () => {
    // GIVEN macOS
    vi.stubGlobal("navigator", { platform: "MacIntel", userAgent: "Macintosh" });

    // WHEN / is pressed without a modifier
    dispatchShortcutEvent(keydown({ key: "/" }));

    // THEN the detail sidebar stays collapsed
    expect(get(guideVisible)).toBe(false);
  });
});
