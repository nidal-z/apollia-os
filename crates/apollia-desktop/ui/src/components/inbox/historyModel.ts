/**
 * History view-model for the Inbox: merges the two origins of a resolved HITL
 * decision into a single, date-sorted list.
 *
 * The runtime exposes them through two commands with two different shapes:
 * `list_chat_approval_history` (a tool call authorized inside a chat session)
 * and `list_resolved_approvals` (an agent task suspended on an approval).
 * Both end up as a `HistoryEntry` so the row rendering stays unique.
 *
 * Kept free of Svelte and i18n so it stays unit-testable: no store, no
 * `invoke`, no side effect.
 */
import type { ResolvedTaskApproval } from "$lib/ipc/inbox";
import type { ResolvedChatApproval } from "$lib/types";

/** Where the decision was taken. */
export type HistoryOrigin = "chat" | "task";

/** Normalized decision, shared by both origins. */
export type HistoryDecision = "accept" | "always_accept" | "refuse";

/** One resolved decision, whatever its origin. */
export interface HistoryEntry {
  /** Stable key for the `{#each}` block. */
  id: string;
  origin: HistoryOrigin;
  decision: HistoryDecision;
  /** Tool name for a chat decision, agent name for a task decision. */
  label: string;
  /** Operator reason, rejections only. */
  reason: string | null;
  /** ISO-8601 timestamp, empty string when the runtime recorded none. */
  resolvedAt: string;
  /** Session id (chat origin) or task id (task origin). */
  reference: string;
}

/** Maps the free-form `decision` column onto the three rendered states. */
export function normalizeChatDecision(raw: string): HistoryDecision {
  if (raw === "accept") return "accept";
  if (raw === "always_accept") return "always_accept";
  return "refuse";
}

/** Adapts a chat tool authorization into the unified entry. */
export function chatToHistoryEntry(row: ResolvedChatApproval): HistoryEntry {
  return {
    id: `chat:${row.session_id}:${row.message_id}:${row.tool_name}:${row.resolved_at}`,
    origin: "chat",
    decision: normalizeChatDecision(row.decision),
    label: row.tool_name,
    reason: row.reason,
    resolvedAt: row.resolved_at,
    reference: row.session_id,
  };
}

/**
 * Adapts a resolved task approval into the unified entry.
 *
 * A task decision has no "always accept" equivalent: the runtime persists a
 * plain boolean, so only `accept` and `refuse` can be produced here.
 */
export function taskToHistoryEntry(row: ResolvedTaskApproval): HistoryEntry {
  return {
    id: `task:${row.task_id}:${row.responded_at ?? ""}`,
    origin: "task",
    decision: row.approved ? "accept" : "refuse",
    label: row.agent_name,
    reason: row.reason,
    resolvedAt: row.responded_at ?? "",
    reference: row.task_id,
  };
}

/**
 * Merges both origins into one list sorted by decision date, most recent
 * first. Entries without a usable timestamp keep their relative order and are
 * pushed to the end rather than dropped, so a legacy row stays visible.
 */
export function mergeApprovalHistory(
  chat: ResolvedChatApproval[],
  tasks: ResolvedTaskApproval[],
): HistoryEntry[] {
  const entries = [...chat.map(chatToHistoryEntry), ...tasks.map(taskToHistoryEntry)];
  return entries.sort((a, b) => timestampOf(b) - timestampOf(a));
}

/** Milliseconds for sorting; an absent or unparsable date sorts last. */
function timestampOf(entry: HistoryEntry): number {
  if (!entry.resolvedAt) return Number.NEGATIVE_INFINITY;
  const ms = new Date(entry.resolvedAt).getTime();
  return Number.isNaN(ms) ? Number.NEGATIVE_INFINITY : ms;
}
