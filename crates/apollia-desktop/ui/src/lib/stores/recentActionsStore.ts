/**
 * Recently-used command palette actions (US-SP42-078).
 *
 * Persists the last 10 action ids executed from the palette so the
 * "Recently used" header can surface them when the input is empty.
 * The queue is bounded and deduplicated on push — touching an action
 * moves it to the front rather than creating a duplicate entry.
 */
import { writable, get } from "svelte/store";

const STORAGE_KEY = "apollia.commandPalette.recentActions";
const MAX_ENTRIES = 10;

function load(): string[] {
  if (typeof localStorage === "undefined") return [];
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return [];
    const parsed = JSON.parse(raw);
    if (!Array.isArray(parsed)) return [];
    return parsed.filter((x): x is string => typeof x === "string").slice(0, MAX_ENTRIES);
  } catch {
    return [];
  }
}

function persist(values: string[]): void {
  if (typeof localStorage === "undefined") return;
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(values));
  } catch {
    /* quota — ignore */
  }
}

export const recentActionIds = writable<string[]>(load());

recentActionIds.subscribe(persist);

/** Push an action id to the front of the queue; dedupe and bound. */
export function touchRecentAction(id: string): void {
  const current = get(recentActionIds).filter((x) => x !== id);
  current.unshift(id);
  recentActionIds.set(current.slice(0, MAX_ENTRIES));
}

/** Clear the recently-used list (debug / user-initiated reset). */
export function clearRecentActions(): void {
  recentActionIds.set([]);
}
