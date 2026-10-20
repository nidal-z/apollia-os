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
import { chatSessions } from "./sse";
import type { ConversationStatsView } from "$lib/types";

export { chatSessions } from "./sse";

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
  pendingUserInputs,
  pendingUserInputCount,
  addPendingUserInput,
  removePendingUserInput,
  getPendingUserInputForSession,
} from "./chat-global";

/** Currently viewed session detail (set when navigating to a conversation). */
export const currentSession = writable<
  import("$lib/types").ChatSessionDetail | null
>(null);

/** Session ID to open automatically when navigating to Chat (set by other pages like Agents). */
export const pendingChatSessionId = writable<string | null>(null);

/**
 * What another screen hands the composer of a conversation it opened: the
 * text of the request, written but not sent, and the files joined to it.
 * The composer of that conversation takes it once; nothing is sent until the
 * person sends it.
 */
export interface PendingComposer {
  sessionId: string;
  text: string;
  files: File[];
}

export const pendingComposer = writable<PendingComposer | null>(null);

/**
 * Cross-component signal: sidebar / command palette flips this to
 * `Date.now()`, `Chat.svelte` watches it and opens the new-chat picker.
 */
export const openNewChatRequested = writable<number>(0);

/**
 * Profile switch of the current session, as the session stores it: whether
 * the built-in assistant receives the profile of the conversation.
 */
export const useUserMemory = writable<boolean>(true);

/** Number of user memory entries injected into the current session. */
export const memoryEntryCount = writable<number>(0);

/** Conversation statistics for the current session. */
export const chatConversationStats = writable<ConversationStatsView | null>(
  null,
);

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
