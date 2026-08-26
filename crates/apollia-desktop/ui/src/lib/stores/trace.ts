/**
 * Event-sourced execution trace store.
 *
 * Two sources of data, merged by task_id:
 * 1. `loadTrace(taskId)` - initial paginated fetch through the Tauri command
 *    `get_task_trace` (replay from SQLite).
 * 2. `subscribeTraceLive(taskId)` - subscription to the Tauri channel
 *    `"trace-event"` (the store already merges live and replay; the
 *    end-to-end SSE path is not wired yet).
 *
 * The events are indexed by `eventId` (lex-ordered UUIDv7), so insertion is
 * idempotent: when a live event arrives before the replay, the merge detects
 * the duplicate by eventId and keeps the order.
 */

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { writable, derived, type Readable } from "svelte/store";

import type { GetTraceParams, RuntimeEventDto, TraceResponse } from "$lib/trace";

/** Page size of the paginated fetch. Matches `MAX_LIMIT` on the Rust side. */
const FETCH_PAGE_SIZE = 500;

/** Loading state per task_id. */
export interface TraceState {
  events: RuntimeEventDto[];
  /** Cursor for the next page (null = the end was reached). */
  nextCursor: string | null;
  /** `true` during an initial fetch or a pagination step. */
  loading: boolean;
  /** Last fetch error, displayed in the component footer. */
  error: string | null;
  /** `true` when the live SSE subscription is active. */
  live: boolean;
}

const EMPTY_STATE: TraceState = {
  events: [],
  nextCursor: null,
  loading: false,
  error: null,
  live: false,
};

/** Map task_id -> full state. The source of truth of the store. */
const _traceMap = writable<Map<string, TraceState>>(new Map());

/** Derived: helper to pick the state of one specific task. */
export function traceFor(taskId: string): Readable<TraceState> {
  return derived(_traceMap, ($m) => $m.get(taskId) ?? EMPTY_STATE);
}

/** Unsubscribe handles indexed by taskId, for cleanup. */
const _liveUnsubs: Map<string, UnlistenFn> = new Map();

/** Minimal mutating updater. */
function _patch(taskId: string, patch: Partial<TraceState>): void {
  _traceMap.update((m) => {
    const prev = m.get(taskId) ?? EMPTY_STATE;
    m.set(taskId, { ...prev, ...patch });
    return m;
  });
}

/**
 * Inserts or merges a list of events into the state of one task.
 *
 * Idempotent: an eventId already present is not duplicated.
 *
 * Final order: sorted by `ts` (primary key), `eventId` as tiebreaker.
 *
 * Why not `eventId` alone? Some variants such as `tool_call_started`
 * pre-generate their UUIDv7 *on the producer side* (`ToolProxy::call`) to
 * serve as the `parent_event_id` of the companion `tool_call_completed`.
 * Others such as `thought` only receive their UUIDv7 when the persistor
 * consumes them from the bus. Consequence: a `tool_call_started` emitted
 * AFTER a `thought` can inherit a lex-earlier eventId, and the causal order
 * is inverted. `ts`, on the other hand, is set by the persistor for EVERY
 * variant, and the broadcast bus is FIFO single-consumer, so `ts` respects
 * the causal order. EventId stays the tiebreaker for ties at the
 * millisecond.
 */
function _mergeEvents(taskId: string, incoming: RuntimeEventDto[]): void {
  if (incoming.length === 0) return;
  _traceMap.update((m) => {
    const prev = m.get(taskId) ?? EMPTY_STATE;
    const seen = new Set(prev.events.map((e) => e.eventId));
    const fresh = incoming.filter((e) => !seen.has(e.eventId));
    if (fresh.length === 0) return m;
    const merged = [...prev.events, ...fresh].sort((a, b) => {
      if (a.ts !== b.ts) return a.ts < b.ts ? -1 : 1;
      if (a.eventId < b.eventId) return -1;
      if (a.eventId > b.eventId) return 1;
      return 0;
    });
    m.set(taskId, { ...prev, events: merged });
    return m;
  });
}

/**
 * Initial fetch or pagination of a trace from the runtime.
 *
 * Without `since`, it starts over from the beginning (the state is cleared
 * first). With `since`, it appends the next page without touching the events
 * already loaded.
 */
export async function loadTrace(
  taskId: string,
  opts: { since?: string | null; reset?: boolean } = {},
): Promise<void> {
  if (opts.reset) {
    _traceMap.update((m) => {
      m.set(taskId, EMPTY_STATE);
      return m;
    });
  }

  _patch(taskId, { loading: true, error: null });

  try {
    const params: GetTraceParams = {
      taskId,
      since: opts.since ?? null,
      limit: FETCH_PAGE_SIZE,
    };
    const resp = await invoke<TraceResponse>("get_task_trace", { params });
    _mergeEvents(taskId, resp.events);
    _patch(taskId, { nextCursor: resp.nextCursor, loading: false });
  } catch (e: unknown) {
    _patch(taskId, {
      loading: false,
      error: e instanceof Error ? e.message : String(e),
    });
  }
}

/**
 * Loads the whole trace, following the cursors to the end.
 *
 * Implicitly bounded by `MAX_LIMIT` on the Rust side (5000 events per page);
 * for very long traces the UI has to virtualise the rendering (see
 * ExecutionTrace.svelte).
 */
export async function loadFullTrace(taskId: string): Promise<void> {
  await loadTrace(taskId, { reset: true });
  let cursor: string | null = null;
  // Conservative bound: prevents an infinite loop should the server keep
  // returning a nextCursor (it should not, but a loop is never trusted
  // blindly).
  for (let i = 0; i < 50; i++) {
    const _state = await new Promise<TraceState>((resolve) => {
      const unsub = traceFor(taskId).subscribe((s) => {
        resolve(s);
        // Unsubscribe immediately: only the snapshot is wanted here.
        Promise.resolve().then(() => unsub());
      });
    });
    cursor = _state.nextCursor;
    if (cursor === null) return;
    await loadTrace(taskId, { since: cursor });
  }
}

/**
 * Tauri envelope of the `EventBus -> "runtime-event"` bridge.
 *
 * See `apollia-desktop/src/events.rs::TauriRuntimeEvent`. The bridge routes
 * EVERY `RuntimeEvent` of the bus to that single Tauri channel; `category`
 * lets the stores dispatch without parsing each variant.
 */
interface TauriRuntimeEvent {
  category: string;
  event_type: string;
  payload: Record<string, unknown>;
}

/**
 * Converts a `runtime-event` envelope (a Rust `RuntimeEvent` variant,
 * serialised externally-tagged as `{"VariantName": {...fields}}`) into a
 * `RuntimeEventDto` the UI components can consume.
 *
 * Generates a synthetic `eventId` on the front side (random UUIDv4),
 * distinct from the UUIDv7 the Rust `EventPersistor` produces. When the
 * panel reloads, `loadTrace` reads back from the DB with the real event_ids
 * and replaces the state (`reset: true`), so no duplicate survives.
 *
 * Returns `null` when the envelope does not carry a kind the trace exposes
 * (a legacy variant of another store, for instance).
 */
function envelopeToDto(env: TauriRuntimeEvent): RuntimeEventDto | null {
  if (env.category !== "trace-event") return null;

  // The payload is externally tagged: `{"AgentLog": {task_id: "T", ...}}`.
  const variantName = env.event_type;
  const variantPayload =
    (env.payload[variantName] as Record<string, unknown> | undefined) ?? null;
  if (variantPayload === null) return null;

  // Mapping from the Rust VariantName to the canonical trace kind (snake_case).
  const kindMap: Record<string, string> = {
    AgentLog: "agent_log",
    Thought: "thought",
    LlmCallStarted: "llm_call_started",
    LlmCallFailed: "llm_call_failed",
    ToolCallStarted: "tool_call_started",
    ToolCallCompleted: "tool_call_completed",
    ToolCallDenied: "tool_call_denied",
    A2AInvokeStarted: "a2a_invoke_started",
    A2AInvokeCompleted: "a2a_invoke_completed",
    Retry: "retry",
    ActionParseError: "action_parse_error",
  };
  const kind = kindMap[variantName] ?? variantName.toLowerCase();

  // Helper: only stringify primitive-ish values; objects/arrays fall back to "".
  const asString = (v: unknown): string =>
    typeof v === "string" ? v : "";

  // Extraction of the common fields from the variant. All are optional
  // depending on the variant, so whatever is present is taken.
  const taskId = asString(variantPayload.task_id);
  const agentId =
    asString(variantPayload.agent_id) ||
    asString(variantPayload.caller_agent_id);
  const parentEventId =
    typeof variantPayload.parent_event_id === "string"
      ? variantPayload.parent_event_id
      : null;
  const correlationId =
    typeof variantPayload.correlation_id === "string"
      ? variantPayload.correlation_id
      : null;
  const stepNum =
    typeof variantPayload.step_num === "number"
      ? variantPayload.step_num
      : null;
  // For the started/completed events that carry their explicit event_id,
  // reuse it so the client-side pairing works before the DB is even
  // queried.
  const eventId =
    typeof variantPayload.event_id === "string"
      ? variantPayload.event_id
      : `live-${crypto.randomUUID()}`;

  return {
    eventId,
    taskId,
    agentId,
    parentEventId,
    correlationId,
    stepNum,
    kind,
    payload: variantPayload,
    ts: new Date().toISOString(),
  } as RuntimeEventDto;
}

/**
 * Subscribes to the live events of one task through the Tauri `"runtime-event"` bus.
 *
 * Filters on the front side on `category === "trace-event"` AND on the target
 * `taskId`: the bus carries every event of every task and every category.
 *
 * Idempotent: calling it twice for the same task opens a single
 * subscription. Always call `unsubscribeTraceLive(taskId)` when the component
 * is torn down.
 */
export async function subscribeTraceLive(taskId: string): Promise<void> {
  if (_liveUnsubs.has(taskId)) return;
  const unlisten = await listen<TauriRuntimeEvent>("runtime-event", (event) => {
    const dto = envelopeToDto(event.payload);
    if (dto === null) return;
    if (dto.taskId !== taskId) return;
    _mergeEvents(taskId, [dto]);
  });
  _liveUnsubs.set(taskId, unlisten);
  _patch(taskId, { live: true });
}

/** Cuts the live subscription of one task. Call it in `onDestroy`. */
export function unsubscribeTraceLive(taskId: string): void {
  const fn = _liveUnsubs.get(taskId);
  if (fn) {
    fn();
    _liveUnsubs.delete(taskId);
    _patch(taskId, { live: false });
  }
}

/** Empties the whole trace of one task (navigation, memory cleanup). */
export function clearTrace(taskId: string): void {
  unsubscribeTraceLive(taskId);
  _traceMap.update((m) => {
    m.delete(taskId);
    return m;
  });
}
