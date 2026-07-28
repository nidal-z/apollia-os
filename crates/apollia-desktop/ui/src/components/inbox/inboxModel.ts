/**
 * Inbox view-model: pure adapters from the runtime stores to the unified
 * `InboxItem`, plus filtering / grouping / risk helpers.
 *
 * Kept free of Svelte and i18n stores so it stays unit-testable: the few
 * user-facing fallbacks take a translator argument (`tr`) supplied by the
 * caller. No `invoke`, no side effects.
 */
import type { PendingApproval, PendingChatApproval, PendingUserInputView } from "$lib/types";
import type { PendingPlanApproval } from "$lib/stores/chat-global";
import type { AskUserQuestion } from "$lib/types";
import type { RiskLevel, InboxType } from "$lib/components/operator";
import type { InboxItem, InboxRisk } from "./types";

/** Minimal translator shape: key in, string out. */
export type Translate = (key: string, opts?: unknown) => string;

/** Pending-tab filter chips. */
export type FilterKey = "all" | "approval" | "ask_user";

/** Date bucket for the grouped pending list. */
export type GroupKey = "today" | "yesterday" | "earlier";

/** Activity-tab filter chips. */
export type ActivityFilter = "all" | "failures" | "degradations" | "llm";

export const GROUP_ORDER: GroupKey[] = ["today", "yesterday", "earlier"];

/** Extracts the optional `risk` payload carried on a task's context. */
export function extractRisk(ctx: Record<string, unknown> | undefined): InboxRisk | undefined {
  if (!ctx || typeof ctx !== "object") return undefined;
  const r = (ctx as { risk?: unknown }).risk;
  if (!r || typeof r !== "object") return undefined;
  const rec = r as Record<string, unknown>;
  const level = rec.level;
  if (level !== "low" && level !== "medium" && level !== "high") return undefined;
  return {
    level,
    summary: typeof rec.summary === "string" ? rec.summary : "",
    impact: typeof rec.impact === "string" ? rec.impact : undefined,
    consequences: Array.isArray(rec.consequences) ? (rec.consequences as string[]) : undefined,
    rationale: typeof rec.rationale === "string" ? rec.rationale : undefined,
    thinking: typeof rec.thinking === "string" ? rec.thinking : undefined,
  };
}

export function taskToInbox(p: PendingApproval, tr: Translate): InboxItem {
  const risk = extractRisk(p.context);
  return {
    id: `task:${p.task_id}`,
    kind: "task",
    agentName: p.agent_name || tr("approvals.unknown_agent"),
    summary: risk?.summary || p.prompt || "-",
    suspendedAt: p.suspended_at,
    risk,
    source: p,
  };
}

type ChatKind = "tool" | "filesystem" | "bash" | "always_accept";
function chatKind(toolName: string): ChatKind {
  if (toolName.startsWith("fs:") || toolName.startsWith("filesystem")) return "filesystem";
  if (toolName.startsWith("bash") || toolName.startsWith("shell")) return "bash";
  return "tool";
}

export function chatToInbox(c: PendingChatApproval): InboxItem {
  return {
    id: `chat:${c.sessionId}:${c.messageId}:${c.toolCallId}`,
    kind: chatKind(c.toolName),
    agentName: c.sessionId.slice(0, 8),
    sessionId: c.sessionId,
    toolName: c.toolName,
    summary: c.inputPreview.slice(0, 140),
    suspendedAt: c.receivedAt,
    source: c,
  };
}

export function planToInbox(p: PendingPlanApproval, tr: Translate): InboxItem {
  const head = p.summary || `${p.stepCount} step(s)`;
  return {
    id: `plan:${p.sessionId}:${p.planId}`,
    kind: "plan",
    agentName: p.sessionId.slice(0, 8),
    sessionId: p.sessionId,
    summary: `${tr("inbox.plan.label")} (${p.stepCount}) - ${head}`,
    suspendedAt: p.submittedAt,
    source: p,
    stepCount: p.stepCount,
  };
}

export function askUserToInbox(u: PendingUserInputView, tr: Translate): InboxItem {
  let parsed: unknown[] = [];
  try {
    const raw = JSON.parse(u.questions_json);
    if (Array.isArray(raw)) parsed = raw;
  } catch {
    // A malformed payload yields an empty (harmless) question list; the raw
    // string is still shown in the builder detail so the failure is visible.
    parsed = [];
  }
  const firstQ = (parsed[0] as { question?: string } | undefined)?.question ?? tr("inbox.row.type_question");
  return {
    id: `ask_user:${u.request_id}`,
    kind: "ask_user",
    agentName: u.session_id ? u.session_id.slice(0, 8) : "agent",
    sessionId: u.session_id || undefined,
    summary: firstQ.slice(0, 140),
    suspendedAt: u.created_at,
    source: u,
    questions: parsed,
  };
}

/**
 * Normalizes the raw `questions_json` payload into the typed shape the
 * `<AskUserForm>` expects (runtime uses snake_case `question_type`).
 */
export function questionsForForm(raw: unknown[]): AskUserQuestion[] {
  const out: AskUserQuestion[] = [];
  for (const entry of raw) {
    if (!entry || typeof entry !== "object") continue;
    const rec = entry as Record<string, unknown>;
    const id = typeof rec.id === "string" ? rec.id : undefined;
    const question = typeof rec.question === "string" ? rec.question : undefined;
    if (!id || !question) continue;
    const rawType =
      (typeof rec.type === "string" ? rec.type : undefined) ??
      (typeof rec.question_type === "string" ? rec.question_type : undefined);
    const type: AskUserQuestion["type"] =
      rawType === "single_choice" || rawType === "multi_choice" ? rawType : "open";
    const options = Array.isArray(rec.options)
      ? (rec.options as unknown[]).filter((o): o is string => typeof o === "string")
      : [];
    const hint = typeof rec.hint === "string" ? rec.hint : undefined;
    out.push({ id, question, type, options, hint });
  }
  return out;
}

export function contextForForm(item: InboxItem): string | undefined {
  if (item.kind !== "ask_user") return undefined;
  const ctx = (item.source as PendingUserInputView).context;
  return typeof ctx === "string" && ctx.trim().length > 0 ? ctx : undefined;
}

export function rowType(item: InboxItem): InboxType {
  return item.kind === "ask_user" ? "question" : "approval";
}

export function rowFilterKey(item: InboxItem): FilterKey {
  return item.kind === "ask_user" ? "ask_user" : "approval";
}

export function riskLevel(item: InboxItem): RiskLevel {
  if (item.kind === "task" && item.risk) {
    if (item.risk.level === "medium") return "medium";
    if (item.risk.level === "high") return "high";
    return "low";
  }
  if (item.kind === "bash" || item.kind === "filesystem") return "medium";
  return "low";
}

export function isChatToolItem(item: InboxItem): boolean {
  return item.kind === "tool" || item.kind === "filesystem" || item.kind === "bash";
}

export function isApprovalItem(item: InboxItem): boolean {
  return item.kind !== "ask_user";
}

export function groupOf(iso: string): GroupKey {
  const d = new Date(iso);
  const now = new Date();
  const startToday = new Date(now.getFullYear(), now.getMonth(), now.getDate()).getTime();
  const startYesterday = startToday - 24 * 60 * 60 * 1000;
  const ts = d.getTime();
  if (ts >= startToday) return "today";
  if (ts >= startYesterday) return "yesterday";
  return "earlier";
}
