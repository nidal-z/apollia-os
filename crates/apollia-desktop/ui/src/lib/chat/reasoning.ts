/**
 * Unified reasoning item model.
 *
 * `ReasoningItem` is a discriminated union describing a single step of the
 * assistant's reasoning trace: a tool call, a web search/read, a thinking or
 * rationale paragraph, a retry chain, or a citation. A whole message's trace
 * is rendered as an ordered `ReasoningItem[]`, each displayed by
 * `ReasoningCard.svelte`.
 */

import type {
  ChatMessageView,
  RetryAttempt as StructuredRetryAttempt,
  ToolCallRationale,
  ToolCallView,
} from "$lib/types";
import { parseStream } from "./streamParser";

export type ReasoningStatus =
  | "pending"
  | "running"
  | "success"
  | "error"
  | "approved"
  | "rejected";

export interface WebSearchResult {
  title: string;
  url: string;
  snippet: string;
  rank: number;
  age?: string | null;
}

export interface RetryAttempt {
  index: number;
  status: ReasoningStatus;
  duration_ms?: number | null;
  error?: string | null;
}

export interface ReasoningItemBase {
  /** Stable id used for expand-state persistence (sessionStorage). */
  id: string;
  status: ReasoningStatus;
}

export type ReasoningItem =
  | (ReasoningItemBase & {
      kind: "tool_call";
      tool: string;
      args: Record<string, unknown>;
      output: string | null;
      duration_ms?: number | null;
      exit_code?: number | null;
      rationale?: ToolCallRationale | null;
      /** Structured retry chain. Empty on first-try success. */
      retry_attempts?: StructuredRetryAttempt[];
    })
  | (ReasoningItemBase & {
      kind: "web_search";
      query: string;
      backend?: string;
      results: WebSearchResult[];
      total_results?: number;
      duration_ms?: number | null;
    })
  | (ReasoningItemBase & {
      kind: "web_read";
      url: string;
      title?: string | null;
      byline?: string | null;
      extracted: string;
      chars_total?: number;
      truncated?: boolean;
      duration_ms?: number | null;
    })
  | (ReasoningItemBase & {
      kind: "thinking";
      content: string;
    })
  | (ReasoningItemBase & {
      kind: "rationale";
      content: string;
    })
  | (ReasoningItemBase & {
      kind: "retry";
      attempts: RetryAttempt[];
      final_error?: string | null;
    })
  | (ReasoningItemBase & {
      kind: "citation";
      source: string;
      excerpt: string;
      url?: string | null;
    });

export type ReasoningKind = ReasoningItem["kind"];

const TOOL_STATUS_MAP: Record<ToolCallView["status"], ReasoningStatus> = {
  pending: "pending",
  authorized: "running",
  executed: "success",
  refused: "rejected",
};

function safeParseObject<T>(raw: string | null): T | null {
  if (!raw) return null;
  try {
    const v = JSON.parse(raw) as T;
    return v && typeof v === "object" ? v : null;
  } catch {
    return null;
  }
}

/**
 * Convert one runtime tool call into a reasoning item.
 *
 * Promotes `web_search` / `web_read` to their specialized kinds when the
 * output parses cleanly; otherwise falls back to a generic `tool_call` so the
 * raw JSON remains visible.
 */
export function toReasoningItem(
  toolCall: ToolCallView,
  idPrefix: string,
  index: number,
): ReasoningItem {
  const id = `${idPrefix}-${index}`;
  const status = TOOL_STATUS_MAP[toolCall.status];

  if (toolCall.tool_name === "web_search") {
    const parsed = safeParseObject<{
      query?: string;
      backend?: string;
      results?: WebSearchResult[];
      total_results?: number;
      duration_ms?: number;
    }>(toolCall.output);
    if (parsed && Array.isArray(parsed.results)) {
      return {
        id,
        kind: "web_search",
        status,
        query:
          (typeof toolCall.input.query === "string"
            ? toolCall.input.query
            : parsed.query) ?? "",
        backend: parsed.backend,
        results: parsed.results,
        total_results: parsed.total_results,
        duration_ms: parsed.duration_ms ?? toolCall.duration_ms,
      };
    }
  }

  if (toolCall.tool_name === "web_read") {
    const parsed = safeParseObject<{
      url?: string;
      title?: string | null;
      byline?: string | null;
      content?: string;
      chars_total?: number;
      truncated?: boolean;
      duration_ms?: number;
    }>(toolCall.output);
    if (parsed && typeof parsed.content === "string") {
      return {
        id,
        kind: "web_read",
        status,
        url:
          parsed.url ??
          (typeof toolCall.input.url === "string" ? toolCall.input.url : ""),
        title: parsed.title ?? null,
        byline: parsed.byline ?? null,
        extracted: parsed.content,
        chars_total: parsed.chars_total,
        truncated: parsed.truncated,
        duration_ms: parsed.duration_ms ?? toolCall.duration_ms,
      };
    }
  }

  return {
    id,
    kind: "tool_call",
    status,
    tool: toolCall.tool_name,
    args: toolCall.input,
    output: toolCall.output,
    duration_ms: toolCall.duration_ms,
    exit_code: toolCall.exit_code,
    rationale: toolCall.rationale ?? null,
    retry_attempts: toolCall.retry_attempts ?? [],
  };
}

/**
 * Build the full reasoning sequence for an assistant message.
 *
 * Order: thinking (if any) → tool calls (mapped in-order).
 * Pending tool calls are returned as-is so the caller can render them via
 * the approval cards instead of a `ReasoningCard` if needed.
 */
export function buildReasoningSequence(
  message: Pick<ChatMessageView, "id" | "tool_calls" | "metadata">,
  content?: string,
): ReasoningItem[] {
  const items: ReasoningItem[] = [];
  const thinking =
    (message.metadata?.thinking_trace as string | null | undefined) ?? null;

  if (thinking) {
    items.push({
      id: `${message.id}-think`,
      kind: "thinking",
      status: "success",
      content: thinking,
    });
  } else if (content) {
    // Models like Qwen3 235B embed thinking in <think>...</think> tags inside content.
    const thinkingBlocks = parseStream(content).filter(
      (b) => b.type === "thinking" && b.closed,
    );
    if (thinkingBlocks.length > 0) {
      items.push({
        id: `${message.id}-think`,
        kind: "thinking",
        status: "success",
        content: thinkingBlocks.map((b) => b.content).join("\n\n"),
      });
    }
  }

  const calls = message.tool_calls ?? [];
  for (let i = 0; i < calls.length; i += 1) {
    items.push(toReasoningItem(calls[i], `${message.id}-tc`, i));
  }
  return items;
}

/** Hard threshold above which the full item list collapses by default. */
export const COLLAPSE_ITEM_THRESHOLD = 10;

/** Line threshold above which JSON previews are truncated with a "show all". */
export const JSON_LINE_THRESHOLD = 600;
