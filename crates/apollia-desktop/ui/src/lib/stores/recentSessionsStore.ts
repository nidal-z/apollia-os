/**
 * Recently-opened chat sessions.
 *
 * Tracks the last 10 chat session ids the user opened so the command
 * palette can list them under the "Sessions" group. Persisted to
 * localStorage so it survives reloads.
 */
import { writable, get, derived, type Readable } from "svelte/store";
import { chatSessions } from "./sse";
import type { ChatSessionSummary } from "$lib/types";

const STORAGE_KEY = "apollia.commandPalette.recentSessions";
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
    /* quota - ignore */
  }
}

export const recentSessionIds = writable<string[]>(load());

recentSessionIds.subscribe(persist);

/** Push a session id to the front of the queue; dedupe and bound. */
export function touchRecentSession(id: string): void {
  const current = get(recentSessionIds).filter((x) => x !== id);
  current.unshift(id);
  recentSessionIds.set(current.slice(0, MAX_ENTRIES));
}

/**
 * Resolved summaries for the recent ids, in MRU order. Falls back to the
 * freshest `chatSessions` entries when no ids have been tracked yet so
 * the palette is useful from first launch.
 */
export const recentSessions: Readable<ChatSessionSummary[]> = derived(
  [recentSessionIds, chatSessions],
  ([$ids, $sessions]) => {
    const byId = new Map($sessions.map((s) => [s.id, s]));
    const ordered = $ids
      .map((id) => byId.get(id))
      .filter((s): s is ChatSessionSummary => !!s);
    if (ordered.length > 0) return ordered.slice(0, MAX_ENTRIES);
    return [...$sessions]
      .sort((a, b) => (b.created_at > a.created_at ? 1 : -1))
      .slice(0, MAX_ENTRIES);
  },
);
