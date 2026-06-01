/**
 * Auto-naming helper for chat sessions.
 *
 * Fires a one-shot LLM call to generate a short title from the user's first
 * message, then optimistically updates the sidebar list so the row swaps
 * "Nouvelle conversation" → generated title without waiting for an SSE refresh.
 *
 * Fully fire-and-forget - never blocks the caller, never surfaces failures
 * to the UI (the placeholder title remains usable, manual rename still works).
 */
import { invoke } from "@tauri-apps/api/core";
import { chatSessions } from "$lib/stores/sse";

const attempted = new Set<string>();

/**
 * Triggers auto-naming for `sessionId` from `firstMessage`. Idempotent:
 * subsequent calls for the same session id are no-ops.
 */
export function triggerAutoName(sessionId: string, firstMessage: string): void {
  if (!sessionId) return;
  const trimmed = firstMessage.trim();
  if (!trimmed) return;
  if (attempted.has(sessionId)) return;
  attempted.add(sessionId);

  void (async () => {
    try {
      const title = await invoke<string>("generate_chat_session_name", {
        sessionId,
        firstMessage: trimmed,
      });
      chatSessions.update((sessions) =>
        sessions.map((s) => (s.id === sessionId ? { ...s, title } : s)),
      );
    } catch (err: unknown) {
      // Silent - the placeholder title remains, user can rename manually.
      console.warn("generate_chat_session_name failed:", err);
      // Allow a future retry (e.g. operator types another first message after
      // the LLM error) by clearing the attempted marker.
      attempted.delete(sessionId);
    }
  })();
}
