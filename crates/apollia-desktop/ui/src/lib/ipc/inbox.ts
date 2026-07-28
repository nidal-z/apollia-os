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
