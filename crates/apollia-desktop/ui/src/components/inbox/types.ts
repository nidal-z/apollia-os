/**
 * Unified inbox item model.
 *
 * Discriminated union covering every flavor of pending human action the
 * runtime can surface - HITL task approvals, chat tool approvals, always-
 * accept requests, and (forward-compat) bash/filesystem approvals.
 */
import type { PendingApproval, PendingChatApproval, PendingUserInputView } from "$lib/types";
import type { PendingPlanApproval } from "$lib/stores/chat-global";

export type InboxItemKind =
  | "task"
  | "tool"
  | "filesystem"
  | "bash"
  | "always_accept"
  | "ask_user"
  | "plan";

/** Optional risk payload. */
export interface InboxRisk {
  level: "low" | "medium" | "high";
  summary: string;
  impact?: string;
  consequences?: string[];
  rationale?: string;
  thinking?: string;
}

interface BaseInboxItem {
  id: string;
  kind: InboxItemKind;
  agentName: string;
  sessionId?: string;
  summary: string;
  toolName?: string;
  suspendedAt: string;
  risk?: InboxRisk;
}

export interface TaskInboxItem extends BaseInboxItem {
  kind: "task";
  source: PendingApproval;
}

export interface ToolInboxItem extends BaseInboxItem {
  kind: "tool" | "filesystem" | "bash" | "always_accept";
  source: PendingChatApproval;
}

export interface AskUserInboxItem extends BaseInboxItem {
  kind: "ask_user";
  source: PendingUserInputView;
  /** Questions parsed from questions_json (cached). */
  questions: unknown[];
}

export interface PlanInboxItem extends BaseInboxItem {
  kind: "plan";
  source: PendingPlanApproval;
  /** Number of steps in the submitted plan. */
  stepCount: number;
}

export type InboxItem =
  | TaskInboxItem
  | ToolInboxItem
  | AskUserInboxItem
  | PlanInboxItem;

/** Urgency threshold default (30 min) in milliseconds. */
export const DEFAULT_URGENCY_THRESHOLD_MS = 30 * 60 * 1000;

/** Reject reason length bounds. */
export const MIN_REJECT_REASON = 5;
export const MAX_REJECT_REASON = 500;
