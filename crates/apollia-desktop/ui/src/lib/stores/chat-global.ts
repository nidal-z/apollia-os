/**
 * Global chat streaming & approval stores for Apollia Desktop.
 *
 * This file is intentionally isolated from `sse.ts` and `chat.ts` to avoid
 * circular dependency issues.  Both `sse.ts` (producer) and `chat.ts` /
 * component code (consumer) may safely import from here.
 */
import { writable, derived, get } from "svelte/store";
import type { PendingChatApproval } from "$lib/types";

// ─── Global streaming state ──────────────────────────────────────────────────

/**
 * Per-session token buffer — accumulates streamed tokens even when the
 * ChatConversation component is not mounted.
 *
 * Key = session ID, value = accumulated token text.
 */
export const globalTokenBuffers = writable<Record<string, string>>({});

/**
 * Per-session streaming flag — true while the backend is actively streaming
 * tokens for a given session.
 */
export const globalStreamingSessions = writable<Set<string>>(new Set());

/** Append a token to a session's global buffer. */
export function appendGlobalToken(sessionId: string, token: string): void {
  globalTokenBuffers.update((buffers) => ({
    ...buffers,
    [sessionId]: (buffers[sessionId] ?? "") + token,
  }));
  globalStreamingSessions.update((s) => {
    const next = new Set(s);
    next.add(sessionId);
    return next;
  });
}

/** Clear a session's global buffer (call when streaming completes or component consumes it). */
export function clearGlobalBuffer(sessionId: string): void {
  globalTokenBuffers.update((buffers) => {
    const next = { ...buffers };
    delete next[sessionId];
    return next;
  });
  globalStreamingSessions.update((s) => {
    const next = new Set(s);
    next.delete(sessionId);
    return next;
  });
}

// ─── Global chat approval state ──────────────────────────────────────────────

/**
 * Global pending chat tool approvals — populated by the SSE event dispatcher
 * so they remain visible even when the user is not on the Chat page.
 */
export const pendingChatApprovals = writable<PendingChatApproval[]>([]);

/** Number of pending chat tool approvals. */
export const pendingChatApprovalCount = derived(
  pendingChatApprovals,
  ($approvals) => $approvals.length,
);

/** Add a pending chat approval. */
export function addPendingChatApproval(approval: PendingChatApproval): void {
  pendingChatApprovals.update((list) => {
    const exists = list.some(
      (a) =>
        a.sessionId === approval.sessionId &&
        a.messageId === approval.messageId &&
        a.toolName === approval.toolName,
    );
    return exists ? list : [...list, approval];
  });
}

/** Remove a pending chat approval (resolved or timed out). */
export function removePendingChatApproval(
  sessionId: string,
  messageId?: string,
  toolName?: string,
): void {
  pendingChatApprovals.update((list) =>
    list.filter((a) => {
      if (a.sessionId !== sessionId) return true;
      if (messageId && a.messageId !== messageId) return true;
      if (toolName && a.toolName !== toolName) return true;
      return false;
    }),
  );
}

/** Get the pending approval for a specific session (if any). */
export function getPendingChatApprovalForSession(
  sessionId: string,
): PendingChatApproval | null {
  const list = get(pendingChatApprovals);
  return list.find((a) => a.sessionId === sessionId) ?? null;
}
