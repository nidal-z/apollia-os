/**
 * dashboardData - pure shaping helpers for the operator Dashboard.
 *
 * Keeps the reactive `$derived` wiring in Dashboard.svelte thin: this module
 * owns the store-snapshot -> view-row transforms (inbox adapters, deliverable
 * / activity / project rows) with no Svelte reactivity of its own.
 */
import { formatRelativeTime } from "$lib/utils";
import type { InboxItem, InboxRisk } from "../inbox/types";
import type { PendingApproval, PendingChatApproval } from "$lib/types";
import type { InboxType, ProjectStatus } from "$lib/components/operator";

export interface PendingRow {
  id: string;
  type: InboxType;
  title: string;
  agent: string;
  timestamp: string;
}

export interface DeliverableRow {
  id: string;
  name: string;
  time: string;
}

export interface ActivityRow {
  id: string;
  name: string;
  status: string;
  time: string;
}

export interface ProjectRow {
  id: string;
  name: string;
  description?: string;
  status: ProjectStatus;
  lastActivity: string;
}

interface TaskLike {
  id: string;
  status: string;
  agent_name: string;
  created_at: string;
}

interface ProjectLike {
  id: string;
  name: string;
  description: string | null;
  updated_at: string;
}

export function extractRisk(
  ctx: Record<string, unknown> | undefined,
): InboxRisk | undefined {
  if (!ctx || typeof ctx !== "object") return undefined;
  const r = (ctx as { risk?: unknown }).risk;
  if (!r || typeof r !== "object") return undefined;
  const rec = r as Record<string, unknown>;
  const level = rec.level;
  if (level !== "low" && level !== "medium" && level !== "high") return undefined;
  return {
    level,
    summary: typeof rec.summary === "string" ? rec.summary : "",
  };
}

export function taskToInbox(p: PendingApproval): InboxItem {
  const risk = extractRisk(p.context);
  return {
    id: `task:${p.task_id}`,
    kind: "task",
    agentName: p.agent_name || "-",
    summary: risk?.summary || p.prompt || "-",
    suspendedAt: p.suspended_at,
    risk,
    source: p,
  };
}

export function chatToInbox(c: PendingChatApproval): InboxItem {
  return {
    id: `chat:${c.sessionId}:${c.messageId}:${c.toolCallId}`,
    kind: "tool",
    agentName: c.sessionId.slice(0, 8),
    sessionId: c.sessionId,
    toolName: c.toolName,
    summary: c.inputPreview.slice(0, 140),
    suspendedAt: c.receivedAt,
    source: c,
  };
}

export function buildInboxItems(
  tasks: PendingApproval[],
  chats: PendingChatApproval[],
): InboxItem[] {
  return [...tasks.map(taskToInbox), ...chats.map(chatToInbox)].sort(
    (a, b) => new Date(a.suspendedAt).getTime() - new Date(b.suspendedAt).getTime(),
  );
}

export function toPendingRows(
  items: InboxItem[],
  untitled: string,
  locale: string,
): PendingRow[] {
  return items.map((item) => ({
    id: item.id,
    type: "approval" as InboxType,
    title: item.summary || untitled,
    agent: item.agentName,
    timestamp: formatRelativeTime(item.suspendedAt, locale),
  }));
}

export function toDeliverableRows(
  tasks: TaskLike[],
  todayStartIso: string,
  locale: string,
  limit = 4,
): DeliverableRow[] {
  return tasks
    .filter((tk) => tk.status === "completed" && tk.created_at >= todayStartIso)
    .slice(0, limit)
    .map((tk) => ({
      id: tk.id,
      name: tk.agent_name,
      time: formatRelativeTime(tk.created_at, locale),
    }));
}

export function toActivityRows(tasks: TaskLike[], locale: string, limit = 4): ActivityRow[] {
  return [...tasks]
    .sort((a, b) => b.created_at.localeCompare(a.created_at))
    .slice(0, limit)
    .map((tk) => ({
      id: tk.id,
      name: tk.agent_name,
      status: tk.status,
      time: formatRelativeTime(tk.created_at, locale),
    }));
}

export function toProjectRows(
  projects: ProjectLike[],
  limit: number,
  last24hIso: string,
  locale: string,
): ProjectRow[] {
  return [...projects]
    .sort((a, b) => b.updated_at.localeCompare(a.updated_at))
    .slice(0, limit)
    .map((p) => ({
      id: p.id,
      name: p.name,
      description: p.description ?? undefined,
      status: (p.updated_at >= last24hIso ? "active" : "pause") as ProjectStatus,
      lastActivity: formatRelativeTime(p.updated_at, locale),
    }));
}
