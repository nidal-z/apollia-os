/**
 * Global keyboard shortcuts for the command palette.
 *
 * Mounts a single `window` keydown listener that opens the palette on
 * Cmd/Ctrl+K. The palette already supports text filtering for narrowing
 * to actions, so a second entry point would be redundant.
 *
 * Returns a disposer that components must call on unmount.
 */
import { commandPaletteOpen } from "$lib/stores/commandPalette";

const isMac = typeof navigator !== "undefined" && navigator.platform.includes("Mac");

function isModKey(event: KeyboardEvent): boolean {
  return isMac ? event.metaKey : event.ctrlKey;
}

export function installGlobalShortcuts(): () => void {
  function onKey(event: KeyboardEvent): void {
    const mod = isModKey(event);
    if (!mod) return;

    if (!event.shiftKey && !event.altKey && event.key.toLowerCase() === "k") {
      event.preventDefault();
      commandPaletteOpen.update((v) => !v);
    }
  }

  window.addEventListener("keydown", onKey);
  return () => window.removeEventListener("keydown", onKey);
}
