import { describe, test, expect } from "vitest";
import { get } from "svelte/store";
import { conversationRowPad } from "./ConversationRow.svelte";
import { sidebarDensity } from "$lib/stores/chatSessions";
import { CHAT_APPEARANCE_DEFAULTS, setChatAppearance } from "$lib/stores/chatAppearance";

// The list density used to be persisted and read by nobody. These tests hold
// its one reader: the conversation row pads itself from it.

describe("ConversationRow - list density", () => {
  test("a compact chat density tightens the conversation rows", () => {
    // GIVEN the reader chose the compact density in Appearance
    setChatAppearance({ density: "compact" });

    // WHEN the row reads the list density
    const pad = conversationRowPad(get(sidebarDensity));

    // THEN the row takes the compact padding
    expect(get(sidebarDensity)).toBe("sm");
    expect(pad).toBe("compact");
  });

  test("the comfortable density keeps the snug padding", () => {
    // GIVEN the default density
    setChatAppearance({ density: CHAT_APPEARANCE_DEFAULTS.density });

    // WHEN the row reads the list density
    const pad = conversationRowPad(get(sidebarDensity));

    // THEN the row keeps its snug padding
    expect(get(sidebarDensity)).toBe("md");
    expect(pad).toBe("snug");
  });
});
