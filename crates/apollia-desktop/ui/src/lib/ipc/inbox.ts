// Typed IPC wrappers for the decisions asked where they are born (the pauses
// of tasks, the consents and the questions of a conversation) and for the
// sent-notification log. Keeping every `invoke` here removes direct Tauri
// command usage from the `.svelte` files and gives each call a typed contract.
//
// Failures are never swallowed: every wrapper rejects with the runtime error
// string so the route can humanize it (`reportError`) instead of hiding it.

import { invoke } from "@tauri-apps/api/core";
import type {
  AskUserAnswer,
  NotificationLogEntry,
  PendingApproval,
  PendingUserInputView,
} from "$lib/types";

/** Lists HITL task approvals suspended and awaiting an operator decision. */
export async function listPendingApprovals(): Promise<PendingApproval[]> {
  return invoke<PendingApproval[]>("list_pending_approvals");
}

/** Lists pending `ask_user` requests carried over across navigation. */
export async function listPendingUserInputs(): Promise<PendingUserInputView[]> {
  return invoke<PendingUserInputView[]>("list_pending_user_inputs");
}

/** Reads the sent-notification log (desktop, webhook, ...). */
export async function getNotificationLogs(limit = 50): Promise<NotificationLogEntry[]> {
  return invoke<NotificationLogEntry[]>("get_notification_logs", { limit });
}

/** Parameters accepted by {@link authorizeChatTool}. */
export interface AuthorizeChatToolArgs {
  sessionId: string;
  messageId: string;
  toolCallId: string;
  toolName: string;
  decision: "accept" | "refuse" | "always_accept";
  reason?: string | null;
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
