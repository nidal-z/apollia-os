/**
 * Global chat streaming & approval stores for Apollia Desktop.
 *
 * This file is intentionally isolated from `sse.ts` and `chat.ts` to avoid
 * circular dependency issues.  Both `sse.ts` (producer) and `chat.ts` /
 * component code (consumer) may safely import from here.
 *
 * Token-buffer management was extracted to `./chatTokenBuffers.ts` in
 * to add LRU + TTL semantics; we re-export the public surface
 * unchanged so existing callers keep working.
 */
import { writable, derived, get } from "svelte/store";
import type { PendingChatApproval, PendingUserInputView } from "$lib/types";

// ─── Global streaming state ──────────────────────────────────────────────────

export {
  globalTokenBuffers,
  globalStreamingSessions,
  appendGlobalToken,
  clearGlobalBuffer,
  closeSessionBuffer,
  getBuffer,
} from "./chatTokenBuffers";

// ─── Global chat approval state ──────────────────────────────────────────────

/**
 * Global pending chat tool approvals - populated by the SSE event dispatcher
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

// ─── Global plan-approval state ──────────────────────────────────────────────

/** A chat plan awaiting the operator's approval, surfaced in the inbox. */
export interface PendingPlanApproval {
  sessionId: string;
  planId: string;
  stepCount: number;
  /** Short label (first step title, or a step count fallback). */
  summary: string;
  submittedAt: string;
}

/**
 * Global pending plan approvals - populated by the SSE event dispatcher on
 * `PlanSubmitted` so a plan awaiting approval is visible in the inbox even when
 * the user is not on the originating chat session.
 */
export const pendingPlanApprovals = writable<PendingPlanApproval[]>([]);

/** Number of plans awaiting approval. */
export const pendingPlanApprovalCount = derived(
  pendingPlanApprovals,
  ($approvals) => $approvals.length,
);

/** Add or replace the pending plan approval for a session (one per session). */
export function addPendingPlanApproval(approval: PendingPlanApproval): void {
  pendingPlanApprovals.update((list) => {
    const without = list.filter((a) => a.sessionId !== approval.sessionId);
    return [...without, approval];
  });
}

/** Remove the pending plan approval for a session (approved, rejected, done). */
export function removePendingPlanApproval(sessionId: string): void {
  pendingPlanApprovals.update((list) =>
    list.filter((a) => a.sessionId !== sessionId),
  );
}

// ─── Global ask_user / user-input state ──────────────────────────────────────

/**
 * Global pending ask_user requests - populated by the SSE event dispatcher
 * so they remain visible even when the user navigates away from the chat page.
 */
export const pendingUserInputs = writable<PendingUserInputView[]>([]);

/** Number of pending ask_user requests. */
export const pendingUserInputCount = derived(
  pendingUserInputs,
  ($inputs) => $inputs.length,
);

/** Add or replace a pending user input entry (idempotent by request_id). */
export function addPendingUserInput(input: PendingUserInputView): void {
  pendingUserInputs.update((list) => {
    const without = list.filter((i) => i.request_id !== input.request_id);
    return [...without, input];
  });
}

/** Remove a resolved or rejected pending user input entry. */
export function removePendingUserInput(requestId: string): void {
  pendingUserInputs.update((list) =>
    list.filter((i) => i.request_id !== requestId),
  );
}

/** Get the pending user input for a specific session (first match). */
export function getPendingUserInputForSession(
  sessionId: string,
): PendingUserInputView | null {
  const list = get(pendingUserInputs);
  // Match by session_id, or return empty-session entries to any session.
  return list.find((i) => i.session_id === sessionId || i.session_id === "") ?? null;
}
