/**
 * Chat session stores for Apollia Desktop.
 *
 * Re-exports the base chatSessions store from sse.ts and provides
 * derived stores for filtered views (active sessions, closed sessions).
 */
import { derived, writable } from "svelte/store";
import { chatSessions } from "./sse";

export { chatSessions } from "./sse";

/** Currently viewed session detail (set when navigating to a conversation). */
export const currentSession = writable<import("$lib/types").ChatSessionDetail | null>(null);

/** Token buffer for streaming responses in the active conversation. */
export const chatTokenBuffer = writable<string>("");

/** Active chat sessions (status !== 'closed'), most recent first. */
export const activeChatSessions = derived(chatSessions, ($sessions) =>
  $sessions.filter((s) => s.status !== "closed"),
);

/** Closed chat sessions. */
export const closedChatSessions = derived(chatSessions, ($sessions) =>
  $sessions.filter((s) => s.status === "closed"),
);

/** Total number of active chat sessions. */
export const activeChatCount = derived(
  activeChatSessions,
  ($active) => $active.length,
);
