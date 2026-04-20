/**
 * Unified inbox item model (US-SP42-053, A.6.6).
 *
 * Discriminated union covering every flavor of pending human action the
 * runtime can surface — HITL task approvals, chat tool approvals, always-
 * accept requests, and (forward-compat) bash/filesystem approvals.
 */
import type { PendingApproval, PendingChatApproval } from "$lib/types";

export type InboxItemKind = "task" | "tool" | "filesystem" | "bash" | "always_accept";

/** Optional risk payload (US-SP42-054 / Chantier 4 P9). */
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

export type InboxItem = TaskInboxItem | ToolInboxItem;

/** Urgency threshold default (30 min) in milliseconds. */
export const DEFAULT_URGENCY_THRESHOLD_MS = 30 * 60 * 1000;

/** Reject reason length bounds (E.33). */
export const MIN_REJECT_REASON = 5;
export const MAX_REJECT_REASON = 500;
