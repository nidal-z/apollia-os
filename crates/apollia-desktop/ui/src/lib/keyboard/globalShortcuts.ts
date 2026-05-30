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

// navigator.platform is deprecated but still the most reliable Mac signal.
const isMac =
  typeof navigator !== "undefined" &&
  ((navigator as Navigator & { platform?: string }).platform ?? "").includes("Mac"); // NOSONAR typescript:S1874

function isModKey(event: KeyboardEvent): boolean {
  return isMac ? event.metaKey : event.ctrlKey;
}

function handleGlobalKey(event: KeyboardEvent): void {
  const mod = isModKey(event);
  if (!mod) return;

  if (!event.shiftKey && !event.altKey && event.key.toLowerCase() === "k") {
    event.preventDefault();
    commandPaletteOpen.update((v) => !v);
  }
}

export function installGlobalShortcuts(): () => void {
  globalThis.addEventListener("keydown", handleGlobalKey);
  return () => globalThis.removeEventListener("keydown", handleGlobalKey);
}
