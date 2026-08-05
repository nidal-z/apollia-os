// Typed IPC wrappers for the Inbox surface (HITL approvals, ask_user, activity,
// sent-notification log). Keeping every `invoke` here removes direct Tauri
// command usage from the `.svelte` files and gives each call a typed contract.
//
// Failures are never swallowed: every wrapper rejects with the runtime error
// string so the route can humanize it (`reportError`) instead of hiding it.

import { invoke } from "@tauri-apps/api/core";
import type { AlwaysScope } from "$lib/components/operator/HITLCard.svelte";
import type {
  AskUserAnswer,
  NotificationChannel,
  NotificationLogEntry,
  PendingApproval,
  PendingUserInputView,
  ResolvedChatApproval,
} from "$lib/types";

/** Lists HITL task approvals suspended and awaiting an operator decision. */
export async function listPendingApprovals(): Promise<PendingApproval[]> {
  return invoke<PendingApproval[]>("list_pending_approvals");
}

/** Lists pending `ask_user` requests carried over across navigation. */
export async function listPendingUserInputs(): Promise<PendingUserInputView[]> {
  return invoke<PendingUserInputView[]>("list_pending_user_inputs");
}

/** Reads the resolved HITL chat-approval history (accept / refuse / always). */
export async function listChatApprovalHistory(
  limit = 50,
  days = 14,
): Promise<ResolvedChatApproval[]> {
  return invoke<ResolvedChatApproval[]>("list_chat_approval_history", { limit, days });
}

/**
 * One resolved HITL **task** approval, as stored in the `task_approvals` table.
 *
 * Shape differs from {@link ResolvedChatApproval}: the decision is a boolean,
 * there is no tool name, and the timestamp can be absent on legacy rows.
 */
export interface ResolvedTaskApproval {
  /** Identifier of the approved / rejected task. */
  task_id: string;
  /** Agent that requested the approval. */
  agent_name: string;
  /** `true` when approved, `false` when rejected. */
  approved: boolean;
  /** Operator reason (rejection only). */
  reason: string | null;
  /** Time the operator took to decide, in milliseconds. */
  wait_duration_ms: number | null;
  /** ISO-8601 decision timestamp, absent on rows written before it existed. */
  responded_at: string | null;
}

/** Reads the resolved HITL task-approval history (agent tasks, not chat). */
export async function listResolvedApprovals(
  limit = 50,
  days = 14,
): Promise<ResolvedTaskApproval[]> {
  return invoke<ResolvedTaskApproval[]>("list_resolved_approvals", { limit, days });
}

/** Reads recent runtime activity events (failures, degradations, LLM down). */
export async function listRuntimeActivity(days = 14): Promise<NotificationLogEntry[]> {
  return invoke<NotificationLogEntry[]>("list_runtime_activity", { days });
}

/** Reads the sent-notification log (desktop, webhook, ...). */
export async function getNotificationLogs(limit = 50): Promise<NotificationLogEntry[]> {
  return invoke<NotificationLogEntry[]>("get_notification_logs", { limit });
}

/** Lists configured notification channels (used to label the sent log). */
export async function listNotificationChannels(): Promise<NotificationChannel[]> {
  return invoke<NotificationChannel[]>("list_notification_channels");
}

/** Approves or rejects a suspended HITL task, forwarding an optional reason. */
export async function resumeTask(
  taskId: string,
  approved: boolean,
  reason: string | null = null,
): Promise<void> {
  return invoke<void>("resume_task", { taskId, approved, reason });
}

/** Parameters accepted by {@link authorizeChatTool}. */
export interface AuthorizeChatToolArgs {
  sessionId: string;
  messageId: string;
  toolCallId: string;
  toolName: string;
  decision: "accept" | "refuse" | "always_accept";
  reason?: string | null;
  scope?: AlwaysScope;
}

/** Resolves a suspended chat tool call (accept / refuse / always-accept). */
export async function authorizeChatTool(args: AuthorizeChatToolArgs): Promise<void> {
  return invoke<void>("authorize_chat_tool", { ...args });
}

/** Sends the structured answers for an `ask_user` request back to the agent. */
export async function respondUserInput(
  requestId: string,
  answers: AskUserAnswer[],
): Promise<void> {
  return invoke<void>("respond_user_input", { requestId, answers });
}

/** Rejects an `ask_user` request with a reason forwarded to the agent. */
export async function respondUserInputRejected(
  requestId: string,
  reason: string,
): Promise<void> {
  return invoke<void>("respond_user_input_rejected", { requestId, reason });
}
