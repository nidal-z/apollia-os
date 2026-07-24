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

const TOOL_STATUS_MAP: Record<ToolCallView["status"], ReasoningStatus> = {
  pending: "pending",
  authorized: "running",
  executed: "success",
  refused: "rejected",
};

/**
 * Convert one runtime tool call into a reasoning item.
 *
 * Every tool, including `web_search` / `web_read`, maps to the flat `tool_call`
 * row so the whole reasoning stream is visually uniform. The web tools' rich
 * result JSON stays visible in the expandable output, exactly like any other
 * tool.
 */
export function toReasoningItem(
  toolCall: ToolCallView,
  idPrefix: string,
  index: number,
): ReasoningItem {
  const id = `${idPrefix}-${index}`;
  const status = TOOL_STATUS_MAP[toolCall.status];

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
 * Separator the runtime uses to join per-step reasoning fragments into the
 * single `thinking_trace` blob (`react_loop.rs`). Splitting on it recovers each
 * step's fragment so they render as separate collapsible captions instead of
 * one dumped block.
 */
const THINKING_FRAGMENT_SEPARATOR = "\n\n---\n\n";

/** Split a joined `thinking_trace` blob back into its per-step fragments. */
function splitThinkingFragments(trace: string): string[] {
  return trace
    .split(THINKING_FRAGMENT_SEPARATOR)
    .map((s) => s.trim())
    .filter((s) => s.length > 0);
}

/** Read `metadata.reasoning_boundaries` as a numeric array, or null when absent. */
function readBoundaries(
  metadata: ChatMessageView["metadata"],
): number[] | null {
  const raw = metadata?.reasoning_boundaries;
  if (!Array.isArray(raw)) return null;
  const nums = raw.filter((n): n is number => typeof n === "number");
  return nums.length === raw.length ? nums : null;
}

/**
 * Build the full reasoning sequence for an assistant message.
 *
 * Each thinking fragment (split from the joined trace) renders as its own
 * caption. When the runtime supplied per-fragment tool-call boundaries, each
 * fragment is interleaved with the tool calls of its ReAct step (fragment k
 * before the tool call at index `boundaries[k]`); otherwise fragments are
 * emitted first, then the tool calls in order. Pending tool calls are returned
 * as-is so the caller can render them via the approval cards.
 */
export function buildReasoningSequence(
  message: Pick<ChatMessageView, "id" | "tool_calls" | "metadata">,
  content?: string,
): ReasoningItem[] {
  const items: ReasoningItem[] = [];
  const thinking =
    (message.metadata?.thinking_trace as string | null | undefined) ?? null;

  let fragments: string[] = [];
  let boundaries: number[] | null = null;
  if (thinking) {
    fragments = splitThinkingFragments(thinking);
    boundaries = readBoundaries(message.metadata);
  } else if (content) {
    // Models like Qwen3 235B embed thinking in <think>...</think> tags inside content.
    fragments = parseStream(content)
      .filter((b) => b.type === "thinking" && b.closed)
      .map((b) => b.content.trim())
      .filter((s) => s.length > 0);
  }

  const calls = message.tool_calls ?? [];
  const thinkingItem = (fragment: string, i: number): ReasoningItem => ({
    id: `${message.id}-think-${i}`,
    kind: "thinking",
    status: "success",
    content: fragment,
  });
  const toolItem = (i: number): ReasoningItem =>
    toReasoningItem(calls[i], `${message.id}-tc`, i);

  // Interleave only when the boundaries line up with the fragments; any
  // mismatch (older messages, a fragment that itself contained the separator)
  // falls back to the safe fragments-then-tools order.
  if (boundaries && boundaries.length === fragments.length) {
    let ti = 0;
    fragments.forEach((fragment, k) => {
      const upto = Math.min(boundaries[k], calls.length);
      while (ti < upto) {
        items.push(toolItem(ti));
        ti += 1;
      }
      items.push(thinkingItem(fragment, k));
    });
    while (ti < calls.length) {
      items.push(toolItem(ti));
      ti += 1;
    }
    return items;
  }

  fragments.forEach((fragment, i) => items.push(thinkingItem(fragment, i)));
  for (let i = 0; i < calls.length; i += 1) {
    items.push(toolItem(i));
  }
  return items;
}

/** Hard threshold above which the full item list collapses by default. */
export const COLLAPSE_ITEM_THRESHOLD = 10;

/** Line threshold above which JSON previews are truncated with a "show all". */
export const JSON_LINE_THRESHOLD = 600;
