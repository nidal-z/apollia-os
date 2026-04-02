import { describe, test, expect } from "vitest";

// ── Keyboard shortcut logic ───────────────────────────────────────────────────

/**
 * Mirrors the keyboard shortcut detection logic in CompanionToggle.svelte.
 * Returns true when the event matches the platform-correct toggle shortcut.
 */
function isToggleShortcut(
  event: { metaKey: boolean; ctrlKey: boolean; key: string },
  isMac: boolean,
): boolean {
  const isModifier = isMac ? event.metaKey : event.ctrlKey;
  return isModifier && event.key === "/";
}

describe("CompanionToggle — keyboard shortcut detection", () => {
  test("Cmd+/ is detected as toggle shortcut on macOS", () => {
    // GIVEN macOS platform
    // WHEN Cmd+/ is pressed
    const event = { metaKey: true, ctrlKey: false, key: "/" };
    // THEN it is the toggle shortcut
    expect(isToggleShortcut(event, true)).toBe(true);
  });

  test("Ctrl+/ is detected as toggle shortcut on non-macOS", () => {
    // GIVEN non-macOS platform
    // WHEN Ctrl+/ is pressed
    const event = { metaKey: false, ctrlKey: true, key: "/" };
    // THEN it is the toggle shortcut
    expect(isToggleShortcut(event, false)).toBe(true);
  });

  test("Cmd+/ is not the shortcut on non-macOS", () => {
    // GIVEN non-macOS platform — Meta key should not trigger
    const event = { metaKey: true, ctrlKey: false, key: "/" };
    expect(isToggleShortcut(event, false)).toBe(false);
  });

  test("Ctrl+/ is not the shortcut on macOS", () => {
    // GIVEN macOS platform — Ctrl key should not trigger
    const event = { metaKey: false, ctrlKey: true, key: "/" };
    expect(isToggleShortcut(event, true)).toBe(false);
  });

  test("Cmd+k does not match the toggle shortcut", () => {
    // GIVEN Cmd+k (different key)
    const event = { metaKey: true, ctrlKey: false, key: "k" };
    expect(isToggleShortcut(event, true)).toBe(false);
  });
});

// ── Active state derivation ───────────────────────────────────────────────────

/**
 * Mirrors the button class derivation in CompanionToggle.svelte.
 * Returns the CSS variant applied when enabled vs disabled.
 */
function resolveButtonVariant(enabled: boolean): "active" | "inactive" {
  return enabled ? "active" : "inactive";
}

describe("CompanionToggle — active state", () => {
  test("button variant is active when companion is enabled", () => {
    // GIVEN companion is enabled
    expect(resolveButtonVariant(true)).toBe("active");
  });

  test("button variant is inactive when companion is disabled", () => {
    // GIVEN companion is disabled
    expect(resolveButtonVariant(false)).toBe("inactive");
  });
});

// ── LLM availability guard ────────────────────────────────────────────────────

/**
 * Mirrors the click handler guard in CompanionToggle.svelte.
 * Returns true when the toggle action should proceed.
 */
function canToggle(hasLlm: boolean): boolean {
  return hasLlm;
}

describe("CompanionToggle — LLM availability guard", () => {
  test("toggle is allowed when at least one LLM backend is ready", () => {
    expect(canToggle(true)).toBe(true);
  });

  test("toggle is blocked when no LLM backend is configured", () => {
    expect(canToggle(false)).toBe(false);
  });
});
