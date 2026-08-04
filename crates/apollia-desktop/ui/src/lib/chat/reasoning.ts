/**
 * Unified reasoning item model.
 *
 * `ReasoningItem` is a discriminated union describing a single step of the
 * assistant's reasoning trace: a tool call, a thinking or rationale paragraph,
 * a retry chain, or a citation. A whole message's trace is rendered as an
 * ordered `ReasoningItem[]`, each displayed by `ReasoningCard.svelte`.
 *
 * Web tools (`web_search` / `web_read`) fold into the flat `tool_call` row so
 * every reasoning step shares one uniform header; their rich, clickable results
 * are restored in the expanded body via `parseWebSearchOutput` /
 * `parseWebReadOutput` below.
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
  failed: "error",
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

/** Minimal slice of a tool call needed to recover a rich web-tool payload. */
export interface WebToolCall {
  input?: Record<string, unknown> | null;
  output: string | null;
  duration_ms?: number | null;
}

/** Parsed `web_search` result payload for the rich, clickable results body. */
export interface WebSearchParsed {
  query: string;
  backend?: string;
  results: WebSearchResult[];
  total_results?: number;
  duration_ms?: number | null;
}

/** Parsed `web_read` result payload for the extracted-article body. */
export interface WebReadParsed {
  url: string;
  title?: string | null;
  byline?: string | null;
  extracted: string;
  chars_total?: number;
  truncated?: boolean;
  duration_ms?: number | null;
}

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
 * Recover the structured `web_search` payload from a tool call's raw output.
 *
 * Returns `null` when the output is absent or does not parse into a results
 * array, so the caller can fall back to the generic raw-output rendering.
 */
export function parseWebSearchOutput(call: WebToolCall): WebSearchParsed | null {
  const parsed = safeParseObject<{
    query?: string;
    backend?: string;
    results?: WebSearchResult[];
    total_results?: number;
    duration_ms?: number;
  }>(call.output);
  if (!parsed || !Array.isArray(parsed.results)) return null;
  const input = call.input ?? {};
  return {
    query:
      (typeof input.query === "string" ? input.query : parsed.query) ?? "",
    backend: parsed.backend,
    results: parsed.results,
    total_results: parsed.total_results,
    duration_ms: parsed.duration_ms ?? call.duration_ms,
  };
}

/**
 * Recover the extracted-article `web_read` payload from a tool call's raw
 * output. Returns `null` when the output is absent or carries no `content`.
 */
export function parseWebReadOutput(call: WebToolCall): WebReadParsed | null {
  const parsed = safeParseObject<{
    url?: string;
    title?: string | null;
    byline?: string | null;
    content?: string;
    chars_total?: number;
    truncated?: boolean;
    duration_ms?: number;
  }>(call.output);
  if (!parsed || typeof parsed.content !== "string") return null;
  const input = call.input ?? {};
  return {
    url: parsed.url ?? (typeof input.url === "string" ? input.url : ""),
    title: parsed.title ?? null,
    byline: parsed.byline ?? null,
    extracted: parsed.content,
    chars_total: parsed.chars_total,
    truncated: parsed.truncated,
    duration_ms: parsed.duration_ms ?? call.duration_ms,
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

/** One live tool invocation, as the streaming turn accumulates it. */
export interface LiveToolStep {
  name: string;
  status: "running" | "done" | "refused";
  startedAt: number;
  durationMs?: number;
  /** Thoughts already closed when this call started. Its place in the order. */
  reasoningCursor: number;
}

/** One row of the live timeline: a closed thought, or a tool invocation. */
export type LiveRow =
  | { kind: "thought"; id: string; content: string }
  | {
      kind: "tool";
      id: string;
      name: string;
      status: LiveToolStep["status"];
      durationMs?: number;
    };

/**
 * Interleave the streaming turn's closed thoughts and tool calls into the order
 * they happened, the live counterpart of [`buildReasoningSequence`].
 *
 * A tool whose `reasoningCursor` is `k` started after `k` thoughts had closed,
 * so it belongs before thought `k`. Rendering the two lists one after the other
 * instead, as an earlier revision did, is what turned a live ReAct loop into
 * all of the thinking followed by all of the actions.
 */
export function buildLiveSequence(
  closedThinking: string[],
  toolChain: LiveToolStep[],
): LiveRow[] {
  const rows: LiveRow[] = [];
  let next = 0;
  const pushToolsUpTo = (cursor: number): void => {
    while (next < toolChain.length && toolChain[next].reasoningCursor <= cursor) {
      const tool = toolChain[next];
      rows.push({
        kind: "tool",
        id: `live-tool-${next}-${tool.startedAt}`,
        name: tool.name,
        status: tool.status,
        durationMs: tool.durationMs,
      });
      next += 1;
    }
  };
  closedThinking.forEach((content, i) => {
    pushToolsUpTo(i);
    rows.push({ kind: "thought", id: `live-think-${i}`, content });
  });
  pushToolsUpTo(Number.POSITIVE_INFINITY);
  return rows;
}

/**
 * Number of reasoning captions (thinking / rationale fragments) an assistant
 * turn produced. Feeds the activity strip summary so the collapsed strip can
 * announce how many thoughts it hides.
 */
export function reasoningFragmentCount(
  message: Pick<ChatMessageView, "id" | "tool_calls" | "metadata">,
  content?: string,
): number {
  return buildReasoningSequence(message, content).filter(
    (i) => i.kind === "thinking" || i.kind === "rationale",
  ).length;
}

/**
 * A single web source shown as a compact card at the end of an assistant turn.
 *
 * Sources are recovered from the finalized `web_search` / `web_read` tool calls
 * and deduplicated by URL, so the answer is followed by a clean list of the
 * pages the agent actually consulted instead of a raw inline dump.
 */
export interface SourceCard {
  /** 1-based display index, matching the answer's citation markers. */
  index: number;
  url: string;
  /** Hostname without a leading `www.`, used for the label and favicon tile. */
  domain: string;
  title: string;
}

function domainOf(url: string): string {
  try {
    return new URL(url).hostname.replace(/^www\./, "");
  } catch {
    return url;
  }
}

/** Normalize a URL for dedup: drop a trailing slash and the hash fragment. */
function normalizeUrl(url: string): string {
  try {
    const u = new URL(url);
    u.hash = "";
    return u.toString().replace(/\/$/, "");
  } catch {
    return url.replace(/\/$/, "");
  }
}

/**
 * Extract the deduplicated web sources for an assistant turn.
 *
 * Walks the message's finalized tool calls: every `web_search` result and every
 * `web_read` page contributes one source, keyed by its URL. The first
 * occurrence of a URL wins (a page read after a search keeps the richer search
 * title only if the read has none). Results keep arrival order and are numbered
 * from 1, so the count matches both the activity summary and the answer's
 * citation markers.
 */
export function extractSources(
  toolCalls: ToolCallView[] | null | undefined,
): SourceCard[] {
  const calls = toolCalls ?? [];
  const seen = new Set<string>();
  const out: SourceCard[] = [];

  const push = (url: string, title: string | null | undefined): void => {
    if (!url) return;
    const key = normalizeUrl(url);
    if (seen.has(key)) return;
    seen.add(key);
    const domain = domainOf(url);
    out.push({
      index: out.length + 1,
      url,
      domain,
      title: (title ?? "").trim() || domain,
    });
  };

  for (const call of calls) {
    if (call.tool_name === "web_search") {
      const parsed = parseWebSearchOutput({
        input: call.input,
        output: call.output,
        duration_ms: call.duration_ms,
      });
      if (parsed) {
        for (const r of parsed.results) push(r.url, r.title);
      }
    } else if (call.tool_name === "web_read") {
      const parsed = parseWebReadOutput({
        input: call.input,
        output: call.output,
        duration_ms: call.duration_ms,
      });
      if (parsed) push(parsed.url, parsed.title);
    }
  }

  return out;
}

/**
 * Total wall-clock time attributable to a turn's tool calls, in milliseconds.
 *
 * Sums `duration_ms` across every call that reported one. Returns 0 when no
 * call carried a duration, letting the caller omit the figure gracefully.
 */
export function sumToolDurationMs(
  toolCalls: ToolCallView[] | null | undefined,
): number {
  let total = 0;
  for (const call of toolCalls ?? []) {
    if (typeof call.duration_ms === "number" && call.duration_ms > 0) {
      total += call.duration_ms;
    }
  }
  return total;
}

/** Hard threshold above which the full item list collapses by default. */
export const COLLAPSE_ITEM_THRESHOLD = 10;

/** Line threshold above which JSON previews are truncated with a "show all". */
export const JSON_LINE_THRESHOLD = 600;
