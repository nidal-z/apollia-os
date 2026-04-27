/**
 * Chat sessions sidebar state.
 *
 * Owns the UI-level state for the refonted sessions sidebar: search query,
 * filter segment, sort order, density, and the per-session flags (pinned,
 * archived, last read) that back the pin/archive/unread features.
 *
 * Persistence is local-first via localStorage — backend SQLite columns
 * (`pinned_at`, `archived_at`, `last_read_at`) are deferred to a follow-up
 * story. UI-side persistence survives reloads and keeps the same UX.
 */
import { derived, writable, type Readable } from "svelte/store";
import { chatSessions } from "./sse";
import type { ChatSessionSummary } from "$lib/types";

export type SessionFilter = "active" | "closed" | "archived" | "pinned";
export type SessionSort = "recent" | "oldest" | "alphabetical" | "most-tokens";
export type SidebarDensity = "sm" | "md" | "lg";

const STORAGE_KEYS = {
  filter: "chat.sidebar.filter",
  sort: "chat.sidebar.sort",
  density: "chat.sidebar.density",
  pinned: "chat.sidebar.pinned",
  archived: "chat.sidebar.archived",
  lastRead: "chat.sidebar.lastRead",
  projectFilter: "chat.sidebar.projectFilter",
} as const;

function loadJson<T>(key: string, fallback: T): T {
  if (typeof localStorage === "undefined") return fallback;
  try {
    const raw = localStorage.getItem(key);
    if (!raw) return fallback;
    return JSON.parse(raw) as T;
  } catch {
    return fallback;
  }
}

function saveJson(key: string, value: unknown): void {
  if (typeof localStorage === "undefined") return;
  try {
    localStorage.setItem(key, JSON.stringify(value));
  } catch {
    /* quota — ignore */
  }
}

function loadString<T extends string>(key: string, fallback: T, allowed: readonly T[]): T {
  if (typeof localStorage === "undefined") return fallback;
  try {
    const raw = localStorage.getItem(key);
    if (!raw) return fallback;
    return (allowed as readonly string[]).includes(raw) ? (raw as T) : fallback;
  } catch {
    return fallback;
  }
}

function saveString(key: string, value: string): void {
  if (typeof localStorage === "undefined") return;
  try {
    localStorage.setItem(key, value);
  } catch {
    /* ignore */
  }
}

// ─── Writable state ──────────────────────────────────────────────────────────

export const sessionSearchQuery = writable<string>("");

export const sessionFilter = writable<SessionFilter>(
  loadString<SessionFilter>(STORAGE_KEYS.filter, "active", [
    "active",
    "closed",
    "archived",
    "pinned",
  ]),
);
sessionFilter.subscribe((v) => saveString(STORAGE_KEYS.filter, v));

export const sessionSort = writable<SessionSort>(
  loadString<SessionSort>(STORAGE_KEYS.sort, "recent", [
    "recent",
    "oldest",
    "alphabetical",
    "most-tokens",
  ]),
);
sessionSort.subscribe((v) => saveString(STORAGE_KEYS.sort, v));

export const sidebarDensity = writable<SidebarDensity>(
  loadString<SidebarDensity>(STORAGE_KEYS.density, "md", ["sm", "md", "lg"]),
);
sidebarDensity.subscribe((v) => saveString(STORAGE_KEYS.density, v));

/** Project filter applied to the sidebar (null = all projects). */
export const sessionProjectFilter = writable<string | null>(
  loadJson<string | null>(STORAGE_KEYS.projectFilter, null),
);
sessionProjectFilter.subscribe((v) => saveJson(STORAGE_KEYS.projectFilter, v));

export const pinnedSessionIds = writable<Set<string>>(
  new Set(loadJson<string[]>(STORAGE_KEYS.pinned, [])),
);
pinnedSessionIds.subscribe((s) => saveJson(STORAGE_KEYS.pinned, Array.from(s)));

export const archivedSessionIds = writable<Set<string>>(
  new Set(loadJson<string[]>(STORAGE_KEYS.archived, [])),
);
archivedSessionIds.subscribe((s) =>
  saveJson(STORAGE_KEYS.archived, Array.from(s)),
);

/** `sessionId → ISO timestamp` of the last time the user viewed the session. */
export const sessionLastReadAt = writable<Record<string, string>>(
  loadJson<Record<string, string>>(STORAGE_KEYS.lastRead, {}),
);
sessionLastReadAt.subscribe((m) => saveJson(STORAGE_KEYS.lastRead, m));

// ─── Mutators ────────────────────────────────────────────────────────────────

export function togglePinned(sessionId: string): void {
  pinnedSessionIds.update((s) => {
    const next = new Set(s);
    if (next.has(sessionId)) next.delete(sessionId);
    else next.add(sessionId);
    return next;
  });
}

export function toggleArchived(sessionId: string): void {
  archivedSessionIds.update((s) => {
    const next = new Set(s);
    if (next.has(sessionId)) next.delete(sessionId);
    else next.add(sessionId);
    return next;
  });
}

export function markSessionRead(sessionId: string): void {
  sessionLastReadAt.update((m) => ({ ...m, [sessionId]: new Date().toISOString() }));
}

// ─── Derived views ───────────────────────────────────────────────────────────

/** Session + UI-derived flags used by cards. */
export interface DecoratedSession extends ChatSessionSummary {
  pinned: boolean;
  archived: boolean;
  unread: boolean;
  lastActivityAt: string;
}

function lastActivity(s: ChatSessionSummary): string {
  // `closed_at` when closed, otherwise created_at. The runtime does not yet
  // surface a `last_message_at`; callers can refine this when available.
  return s.closed_at ?? s.created_at;
}

export const decoratedSessions: Readable<DecoratedSession[]> = derived(
  [chatSessions, pinnedSessionIds, archivedSessionIds, sessionLastReadAt],
  ([$sessions, $pinned, $archived, $lastRead]) =>
    $sessions.map((s) => {
      const activity = lastActivity(s);
      const readAt = $lastRead[s.id];
      const unread = !readAt || new Date(readAt).getTime() < new Date(activity).getTime();
      return {
        ...s,
        pinned: $pinned.has(s.id),
        archived: $archived.has(s.id),
        unread: unread && s.status !== "closed",
        lastActivityAt: activity,
      };
    }),
);

function fuzzyMatch(needle: string, haystack: string): boolean {
  if (!needle) return true;
  return haystack.toLowerCase().includes(needle.toLowerCase());
}

function matchSession(s: DecoratedSession, query: string): boolean {
  if (!query.trim()) return true;
  const hay = [
    s.title ?? "",
    s.agent_name ?? "",
    s.last_message_preview ?? "",
    s.mode,
  ].join(" ");
  return fuzzyMatch(query.trim(), hay);
}

function compareSessions(a: DecoratedSession, b: DecoratedSession, order: SessionSort): number {
  switch (order) {
    case "recent":
      return new Date(b.lastActivityAt).getTime() - new Date(a.lastActivityAt).getTime();
    case "oldest":
      return new Date(a.lastActivityAt).getTime() - new Date(b.lastActivityAt).getTime();
    case "alphabetical": {
      const na = (a.title ?? a.agent_name ?? a.mode).toLowerCase();
      const nb = (b.title ?? b.agent_name ?? b.mode).toLowerCase();
      return na.localeCompare(nb);
    }
    case "most-tokens":
      return (b.message_count ?? 0) - (a.message_count ?? 0);
  }
}

/** Sessions passing the current filter + search + project scope, sorted. */
export const visibleSessions: Readable<DecoratedSession[]> = derived(
  [
    decoratedSessions,
    sessionFilter,
    sessionSort,
    sessionSearchQuery,
    sessionProjectFilter,
  ],
  ([$decorated, $filter, $sort, $query, $project]) => {
    return $decorated
      .filter((s) => {
        if ($project !== null && s.project_id !== $project) return false;
        switch ($filter) {
          case "active":
            return !s.archived && s.status !== "closed";
          case "closed":
            return !s.archived && s.status === "closed";
          case "archived":
            return s.archived;
          case "pinned":
            return s.pinned && !s.archived;
        }
      })
      .filter((s) => matchSession(s, $query))
      .sort((a, b) => compareSessions(a, b, $sort));
  },
);

/** Pinned sessions (rendered separately at the top of the list). */
export const pinnedVisibleSessions: Readable<DecoratedSession[]> = derived(
  [visibleSessions, sessionFilter],
  ([$visible, $filter]) => {
    if ($filter === "pinned") return [];
    return $visible.filter((s) => s.pinned);
  },
);

/** Non-pinned sessions (main list). */
export const unpinnedVisibleSessions: Readable<DecoratedSession[]> = derived(
  [visibleSessions, sessionFilter],
  ([$visible, $filter]) => {
    if ($filter === "pinned") return $visible;
    return $visible.filter((s) => !s.pinned);
  },
);

/** Count of unread sessions (used for badges elsewhere). */
export const unreadSessionCount: Readable<number> = derived(
  decoratedSessions,
  ($d) => $d.filter((s) => s.unread).length,
);
