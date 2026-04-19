/**
 * Keyboard shortcut registry (US-SP42-007 — E.45).
 *
 * Every component that owns a global hotkey registers a `Shortcut`
 * entry here so the `?` overlay can list them. Registration is
 * reactive: entries added to the store appear immediately.
 *
 * The registry does NOT wire the listener itself — components keep
 * ownership of their own `window.addEventListener("keydown", …)`
 * and call `registerShortcuts(...)` in their `onMount` hook.
 */
import { writable, derived, get } from "svelte/store";

export interface Shortcut {
  /** Stable id (e.g. `nav.back`) used to deduplicate registrations. */
  id: string;
  /** Human-readable keys, e.g. `["Cmd", "["]`. Rendered as `<kbd>` chips. */
  keys: string[];
  /** Short description — translation key or literal label. */
  descriptionKey: string;
  /** Logical grouping — `navigation`, `editor`, `panels`, … */
  group: string;
}

const registry = writable<Record<string, Shortcut>>({});

export const shortcuts = derived(registry, ($reg) => Object.values($reg));

export function registerShortcuts(entries: Shortcut[]): () => void {
  registry.update((current) => {
    const next = { ...current };
    for (const entry of entries) next[entry.id] = entry;
    return next;
  });
  return () => {
    registry.update((current) => {
      const next = { ...current };
      for (const entry of entries) delete next[entry.id];
      return next;
    });
  };
}

export function listShortcuts(): Shortcut[] {
  return Object.values(get(registry));
}
