/**
 * Global keyboard shortcuts for the command palette (US-SP42-078).
 *
 * Mounts a single `window` keydown listener that understands the two
 * palette entry points:
 *   - Cmd/Ctrl+K        → open palette in "all" mode (pages + actions)
 *   - Cmd/Ctrl+Shift+P  → open palette in "actions" mode (commands only)
 *
 * Returns a disposer that components must call on unmount. The listener
 * ignores key events whose target is an editable field unless the combo
 * is one of the palette triggers (the palette is always reachable).
 */
import { commandPaletteOpen } from "$lib/stores/commandPalette";
import { paletteMode } from "$lib/command-palette/paletteIndex";

const isMac = typeof navigator !== "undefined" && navigator.platform.includes("Mac");

function isModKey(event: KeyboardEvent): boolean {
  return isMac ? event.metaKey : event.ctrlKey;
}

export function installGlobalShortcuts(): () => void {
  function onKey(event: KeyboardEvent): void {
    const mod = isModKey(event);
    if (!mod) return;

    // Cmd/Ctrl+K — toggle palette in "all" mode.
    if (!event.shiftKey && !event.altKey && event.key.toLowerCase() === "k") {
      event.preventDefault();
      paletteMode.set("all");
      commandPaletteOpen.update((v) => !v);
      return;
    }

    // Cmd/Ctrl+Shift+P — open palette in "actions" mode.
    if (event.shiftKey && !event.altKey && event.key.toLowerCase() === "p") {
      event.preventDefault();
      paletteMode.set("actions");
      commandPaletteOpen.set(true);
      return;
    }
  }

  window.addEventListener("keydown", onKey);
  return () => window.removeEventListener("keydown", onKey);
}
