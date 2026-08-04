/**
 * Per-session metrics store for.
 *
 * Fetches aggregated metrics from the backend via the `chat_session_metrics`
 * Tauri command and refreshes when relevant chat runtime events fire. Updates
 * are throttled to max 2 per second per session to avoid flooding the IPC
 * bridge on long tool bursts.
 */

import { writable, derived, get, type Readable } from "svelte/store";
import { invoke } from "@tauri-apps/api/core";

export interface ToolStatEntry {
  tool_name: string;
  calls: number;
  executed: number;
  refused: number;
}

export interface SessionMetrics {
  session_id: string;
  prompt_tokens: number;
  completion_tokens: number;
  cache_read_input_tokens: number;
  cache_write_input_tokens: number;
  cost_usd: number | null;
  budget_max_steps: number;
  steps_used: number;
  /** Real context window of the active model, in tokens (0 when unknown). */
  context_window_tokens: number;
  /** Current context occupancy in tokens (prompt size of the last LLM call). */
  context_tokens_used: number;
  tool_stats: ToolStatEntry[];
  exchanges_count: number;
  started_at: string | null;
  updated_at: string | null;
  active_duration_ms: number;
  llm_backend: string | null;
}

const EMPTY: SessionMetrics = {
  session_id: "",
  prompt_tokens: 0,
  completion_tokens: 0,
  cache_read_input_tokens: 0,
  cache_write_input_tokens: 0,
  cost_usd: null,
  budget_max_steps: 0,
  steps_used: 0,
  context_window_tokens: 0,
  context_tokens_used: 0,
  tool_stats: [],
  exchanges_count: 0,
  started_at: null,
  updated_at: null,
  active_duration_ms: 0,
  llm_backend: null,
};

const THROTTLE_MS = 500; // max 2 refreshes/sec per session.

const metricsBySession = writable<Record<string, SessionMetrics>>({});
const lastFetchAt: Map<string, number> = new Map();

/**
 * Fetch the latest metrics for `sessionId` from the backend.
 *
 * Throttled: calls within 500 ms of the previous fetch for the same session
 * are coalesced (dropped). Pass `force = true` to bypass the throttle (e.g.
 * when the drawer first opens).
 */
export async function refreshSessionMetrics(
  sessionId: string,
  force = false,
): Promise<void> {
  if (!sessionId) return;
  const now = Date.now();
  const last = lastFetchAt.get(sessionId) ?? 0;
  if (!force && now - last < THROTTLE_MS) return;
  lastFetchAt.set(sessionId, now);

  try {
    const m = await invoke<SessionMetrics>("chat_session_metrics", {
      sessionId,
    });
    metricsBySession.update((all) => ({ ...all, [sessionId]: m }));
  } catch (err) {
    // Runtime may not be ready yet - keep previous value silently.
    console.debug("chat_session_metrics failed", err);
  }
}

/**
 * Refresh the metrics of the session currently open in the chat view.
 *
 * Used by events that carry no session id (e.g. `ContextCompacted`): the only
 * gauge on screen belongs to the viewed session, so refreshing it is both
 * sufficient and cheap. Forced: a compaction is rare and the gauge must move
 * immediately, not after the throttle window.
 */
export async function refreshActiveSessionMetrics(): Promise<void> {
  const { currentSession } = await import("$lib/stores/chat");
  const sessionId = get(currentSession)?.id;
  if (sessionId) {
    await refreshSessionMetrics(sessionId, true);
  }
}

/**
 * Clear the cached metrics for a session (e.g. on close/delete).
 */
export function clearSessionMetrics(sessionId: string): void {
  lastFetchAt.delete(sessionId);
  metricsBySession.update((all) => {
    const next = { ...all };
    delete next[sessionId];
    return next;
  });
}

/**
 * Derived store returning metrics for a given session, or a zero-filled
 * default when none are cached yet.
 */
export function sessionMetricsStore(sessionId: string): Readable<SessionMetrics> {
  return derived(metricsBySession, ($all) => $all[sessionId] ?? { ...EMPTY, session_id: sessionId });
}

/**
 * Read the current value synchronously (for one-shot consumers like tests).
 */
export function getCachedMetrics(sessionId: string): SessionMetrics {
  return get(metricsBySession)[sessionId] ?? { ...EMPTY, session_id: sessionId };
}
