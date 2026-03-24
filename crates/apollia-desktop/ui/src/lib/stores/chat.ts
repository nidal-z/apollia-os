/**
 * Chat session stores for Apollia Desktop.
 *
 * Re-exports the base chatSessions store from sse.ts and provides
 * derived stores for filtered views (active sessions, closed sessions).
 */
import { derived, writable } from "svelte/store";
import { chatSessions, extractedInsights } from "./sse";
import type { ConversationStatsView } from "$lib/types";

export { chatSessions } from "./sse";
export { extractedInsights } from "./sse";

/** Currently viewed session detail (set when navigating to a conversation). */
export const currentSession = writable<
  import("$lib/types").ChatSessionDetail | null
>(null);

/** Session ID to open automatically when navigating to Chat (set by other pages like Agents). */
export const pendingChatSessionId = writable<string | null>(null);

/** Token buffer for streaming responses in the active conversation. */
export const chatTokenBuffer = writable<string>("");

/** Whether user memory injection is enabled for the current session. */
export const useUserMemory = writable<boolean>(true);

/** Number of user memory entries injected into the current session. */
export const memoryEntryCount = writable<number>(0);

/** Conversation statistics for the current session. */
export const chatConversationStats = writable<ConversationStatsView | null>(
  null,
);

/** Reset extracted insights to empty. */
export function clearInsights(): void {
  extractedInsights.set([]);
}

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
