/**
 * Chat sessions sidebar state.
 *
 * Owns the UI-level state for the refonted sessions sidebar: search query,
 * filter segment, sort order, the list density (read from the chat
 * density of `chatAppearance.ts`), and the per-session flags (pinned,
 * archived, last read) that back the pin/archive/unread features.
 *
 * Persistence is local-first via localStorage - backend SQLite columns
 * (`pinned_at`, `archived_at`, `last_read_at`) do not exist yet. UI-side
 * persistence survives reloads and keeps the same UX.
 */
import { derived, writable, type Readable } from "svelte/store";
import { chatSessions } from "./sse";
import { chatDensity, type ChatDensity } from "./chatAppearance";
import type { ChatSessionSummary } from "$lib/types";

export type SessionFilter = "active" | "closed" | "archived" | "pinned";
/**
 * Order of the conversation list, criterion and direction in one choice:
 * newest or oldest created first, most recent activity first, name A to Z,
 * most generated tokens first.
 */
export type SessionSort = "newest" | "oldest" | "activity" | "name" | "tokens";

/** The choices in the order the sort menu offers them; the first is the default. */
export const SESSION_SORTS: readonly SessionSort[] = [
  "newest",
  "oldest",
  "activity",
  "name",
  "tokens",
];
export type SidebarDensity = "sm" | "md";

const STORAGE_KEYS = {
  filter: "chat.sidebar.filter",
  sort: "chat.sidebar.sort",
  pinned: "chat.sidebar.pinned",
  archived: "chat.sidebar.archived",
  lastRead: "chat.sidebar.lastRead",
  activity: "chat.sidebar.activity",
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
    /* quota - ignore */
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
  loadString<SessionSort>(STORAGE_KEYS.sort, SESSION_SORTS[0], SESSION_SORTS),
);
sessionSort.subscribe((v) => saveString(STORAGE_KEYS.sort, v));

/** The conversation list follows the chat density chosen in Appearance. */
export function sidebarDensityFor(density: ChatDensity): SidebarDensity {
  return density === "compact" ? "sm" : "md";
}

export const sidebarDensity: Readable<SidebarDensity> = derived(
  chatDensity,
  sidebarDensityFor,
);

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

/**
 * `sessionId → ISO timestamp` of the last message sent in the session from
 * this window. The list the runtime returns carries no time of last message,
 * so the activity order reads this, and the creation time where it is absent.
 */
export const sessionActivityAt = writable<Record<string, string>>(
  loadJson<Record<string, string>>(STORAGE_KEYS.activity, {}),
);
sessionActivityAt.subscribe((m) => saveJson(STORAGE_KEYS.activity, m));

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

export function noteSessionActivity(sessionId: string, at: Date = new Date()): void {
  sessionActivityAt.update((m) => ({ ...m, [sessionId]: at.toISOString() }));
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

function time(iso: string | null | undefined): number {
  if (!iso) return Number.NaN;
  return new Date(iso).getTime();
}

/** The latest of the noted activity, the closing and the creation of a session. */
export function lastActivity(s: ChatSessionSummary, noted?: string): string {
  let latest = s.created_at;
  for (const candidate of [s.closed_at, noted]) {
    if (candidate && time(candidate) > time(latest)) latest = candidate;
  }
  return latest;
}

export const decoratedSessions: Readable<DecoratedSession[]> = derived(
  [chatSessions, pinnedSessionIds, archivedSessionIds, sessionLastReadAt, sessionActivityAt],
  ([$sessions, $pinned, $archived, $lastRead, $activity]) =>
    $sessions.map((s) => {
      const activity = lastActivity(s, $activity[s.id]);
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

/** Orders two times, newest or oldest first; an unreadable time goes last. */
function byTime(a: string, b: string, newest: boolean): number {
  const ta = time(a);
  const tb = time(b);
  if (Number.isNaN(ta)) return Number.isNaN(tb) ? 0 : 1;
  if (Number.isNaN(tb)) return -1;
  return newest ? tb - ta : ta - tb;
}

function displayName(s: DecoratedSession): string {
  return s.title ?? s.agent_name ?? s.mode;
}

export function compareSessions(
  a: DecoratedSession,
  b: DecoratedSession,
  order: SessionSort,
): number {
  switch (order) {
    case "newest":
      return byTime(a.created_at, b.created_at, true);
    case "oldest":
      return byTime(a.created_at, b.created_at, false);
    case "activity":
      return byTime(a.lastActivityAt, b.lastActivityAt, true);
    case "name":
      return displayName(a).localeCompare(displayName(b), undefined, { sensitivity: "base" });
    case "tokens":
      // The tokens generated for the conversation, never its message count;
      // two conversations of the same weight keep the newest first.
      return (
        (b.output_tokens ?? 0) - (a.output_tokens ?? 0) ||
        byTime(a.created_at, b.created_at, true)
      );
  }
}

/**
 * The conversations the Chat list shows: archived ones left out, the search
 * applied to the title, the agent, the last message and the mode, in `order`.
 */
export function sidebarSessions(
  sessions: readonly DecoratedSession[],
  query: string,
  order: SessionSort,
): DecoratedSession[] {
  return sessions
    .filter((s) => !s.archived)
    .filter((s) => matchSession(s, query))
    .sort((a, b) => compareSessions(a, b, order));
}

/** Sessions passing the current filter + search, sorted. */
export const visibleSessions: Readable<DecoratedSession[]> = derived(
  [
    decoratedSessions,
    sessionFilter,
    sessionSort,
    sessionSearchQuery,
  ],
  ([$decorated, $filter, $sort, $query]) => {
    return $decorated
      .filter((s) => {
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
