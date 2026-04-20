/**
 * Scoped keyboard shortcuts — registered globally but only active while
 * the application's current route matches a given predicate.
 *
 * Introduced for US-SP42-074 (Cmd+S / Ctrl+S inside `/settings/*`).
 */

import { get } from "svelte/store";
import { currentRoute, type Route } from "$lib/stores/navigation";

export interface ScopedShortcut {
  /** Combo description for debugging (e.g. `"cmd+s"`). */
  combo: string;
  /** Matcher against the raw KeyboardEvent. */
  match: (e: KeyboardEvent) => boolean;
  /** Predicate to decide whether the shortcut is currently active. */
  scope: (route: Route) => boolean;
  /** Handler. Return `true` if the event was consumed (default). */
  handler: (e: KeyboardEvent) => void | boolean;
}

const isMac = typeof navigator !== "undefined" && /Mac|iPod|iPhone|iPad/.test(navigator.platform);

/** Cross-platform "save" key-combo: Cmd+S on macOS, Ctrl+S elsewhere. */
export function isSaveCombo(e: KeyboardEvent): boolean {
  if (e.key !== "s" && e.key !== "S") return false;
  const primary = isMac ? e.metaKey : e.ctrlKey;
  const other = isMac ? e.ctrlKey : e.metaKey;
  return primary && !other && !e.altKey;
}

const shortcuts: ScopedShortcut[] = [];
let attached = false;

function onGlobalKeydown(e: KeyboardEvent): void {
  const route = get(currentRoute);
  for (const s of shortcuts) {
    if (!s.scope(route)) continue;
    if (!s.match(e)) continue;
    const consumed = s.handler(e);
    if (consumed !== false) {
      e.preventDefault();
      e.stopPropagation();
    }
    return;
  }
}

/** Register a scoped shortcut. Returns an unregister function. */
export function registerScopedShortcut(shortcut: ScopedShortcut): () => void {
  shortcuts.push(shortcut);
  if (!attached && typeof document !== "undefined") {
    document.addEventListener("keydown", onGlobalKeydown, true);
    attached = true;
  }
  return () => {
    const idx = shortcuts.indexOf(shortcut);
    if (idx !== -1) shortcuts.splice(idx, 1);
    if (shortcuts.length === 0 && attached && typeof document !== "undefined") {
      document.removeEventListener("keydown", onGlobalKeydown, true);
      attached = false;
    }
  };
}

/** Convenience scope predicate: `currentRoute === "settings"`. */
export const inSettingsScope = (route: Route): boolean => route === "settings";
