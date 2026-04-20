/**
 * Chat session stores for Apollia Desktop.
 *
 * Re-exports the base chatSessions store from sse.ts and provides
 * derived stores for filtered views (active sessions, closed sessions).
 *
 * Global streaming & approval stores live in `chat-global.ts` (to avoid
 * circular dependency with `sse.ts`) and are re-exported here for convenience.
 */
import { derived, writable } from "svelte/store";
import { chatSessions, extractedInsights, rejectedInsights } from "./sse";
import type {
  ConversationStatsView,
  InsightEntry,
  RejectedInsightEntry,
} from "$lib/types";

export { chatSessions } from "./sse";
export { extractedInsights, rejectedInsights } from "./sse";

/**
 * Move an insight from the pending list into the rejected list with an
 * explicit reason (US-SP42-042). The entry remains browseable in the
 * "Rejected" tab for audit.
 */
export function recordRejectedInsight(
  insight: InsightEntry,
  reason: string,
): void {
  const entry: RejectedInsightEntry = {
    ...insight,
    rejected_reason: reason,
    rejected_at: new Date().toISOString(),
  };
  rejectedInsights.update((list) => [entry, ...list]);
  extractedInsights.update((list) => list.filter((i) => i.id !== insight.id));
}

// Re-export global chat stores from the cycle-free module
export {
  globalTokenBuffers,
  globalStreamingSessions,
  pendingChatApprovals,
  pendingChatApprovalCount,
  appendGlobalToken,
  clearGlobalBuffer,
  closeSessionBuffer,
  getBuffer,
  addPendingChatApproval,
  removePendingChatApproval,
  getPendingChatApprovalForSession,
} from "./chat-global";

/** Currently viewed session detail (set when navigating to a conversation). */
export const currentSession = writable<
  import("$lib/types").ChatSessionDetail | null
>(null);

/** Session ID to open automatically when navigating to Chat (set by other pages like Agents). */
export const pendingChatSessionId = writable<string | null>(null);

/**
 * Cross-component signal: sidebar / command palette flips this to
 * `Date.now()`, `Chat.svelte` watches it and opens the new-chat picker.
 */
export const openNewChatRequested = writable<number>(0);

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
