import { vi, describe, test, expect, beforeEach, afterEach } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue(""),
}));

import { get } from "svelte/store";
import { registerGlobalShortcutHandlers } from "$lib/navigation/globalShortcutHandlers";
import { dispatchShortcutEvent } from "$lib/navigation/shortcutDispatcher";
import { SHORTCUTS } from "$lib/navigation/shortcuts";
import { detailSidebar, guideVisible } from "$lib/stores/detailSidebar";
import { openNewChatRequested } from "$lib/stores/chat";
import { currentRoute } from "$lib/stores/navigation";
import fr from "$lib/i18n/fr.json";
import en from "$lib/i18n/en.json";

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

describe("globalShortcutHandlers - Cmd+T starts a new conversation", () => {
  beforeEach(() => {
    currentRoute.set("agents");
    openNewChatRequested.set(0);
  });

  test("the catalog names the chord and what it does, in both languages", () => {
    // GIVEN the shortcut catalog and the two catalogues of the interface
    // WHEN reading the entry of Cmd+T
    const entry = SHORTCUTS.find((s) => s.id === "chat.new_conversation");

    // THEN it is global, Cmd+T on macOS and Ctrl+T elsewhere, and it reads
    // as a new conversation
    expect(entry?.combo).toEqual({ mac: "⌘+T", win: "Ctrl+T" });
    expect(entry?.scope).toBe("global");
    expect(entry?.descriptionKey).toBe("settings.shortcuts.items.chat_new_conversation");
    expect(fr.settings.shortcuts.items.chat_new_conversation).toBe("Nouvelle conversation");
    expect(en.settings.shortcuts.items.chat_new_conversation).toBe("New conversation");
  });

  test("no entry of the catalog creates a task any more", () => {
    // GIVEN the shortcut catalog
    // WHEN looking for a chord bound to T
    const onT = SHORTCUTS.filter((s) => /\+T$/.test(s.combo.mac));

    // THEN the new conversation is the only one
    expect(onT.map((s) => s.id)).toEqual(["chat.new_conversation"]);
  });

  test("Cmd+T on macOS shows the chat and asks for a new conversation", () => {
    // GIVEN macOS, the Agents screen shown
    vi.stubGlobal("navigator", { platform: "MacIntel", userAgent: "Macintosh" });

    // WHEN Cmd+T is pressed
    const event = keydown({ key: "t", metaKey: true });
    dispatchShortcutEvent(event);

    // THEN the chat is shown, a new conversation is asked for, and the
    // default of the webview is suppressed
    expect(get(currentRoute)).toBe("chat");
    expect(get(openNewChatRequested)).toBeGreaterThan(0);
    expect(event.preventDefault).toHaveBeenCalled();
  });

  test("Ctrl+T outside macOS does the same", () => {
    // GIVEN a non-Apple platform, the Agents screen shown
    vi.stubGlobal("navigator", { platform: "Win32", userAgent: "Windows" });

    // WHEN Ctrl+T is pressed
    dispatchShortcutEvent(keydown({ key: "t", ctrlKey: true }));

    // THEN the chat is shown and a new conversation is asked for
    expect(get(currentRoute)).toBe("chat");
    expect(get(openNewChatRequested)).toBeGreaterThan(0);
  });

  test("the chord is left to a text field that has the focus", () => {
    // GIVEN macOS, focus in a text field of the Agents screen
    vi.stubGlobal("navigator", { platform: "MacIntel", userAgent: "Macintosh" });

    // WHEN Cmd+T is pressed
    dispatchShortcutEvent(keydown({ key: "t", metaKey: true, targetTag: "INPUT" }));

    // THEN the screen stays and no conversation is asked for
    expect(get(currentRoute)).toBe("agents");
    expect(get(openNewChatRequested)).toBe(0);
  });
});
