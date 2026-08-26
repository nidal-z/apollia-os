/**
 * Types of the event-sourced execution trace.
 *
 * Discriminated union on `kind`: every variant has a typed payload. The
 * `RuntimeEventDto` come either from `invoke("get_task_trace", ...)`
 * (paginated replay) or from the Tauri channel `"trace-event"` (live SSE).
 *
 * The payloads mirror 1:1 the Rust mapping in
 * `crates/apollia-runtime/src/observability/persistor.rs::event_to_record`.
 */

// ─────────────────────────────────────────────────────────────────
// Payloads typed per kind
// ─────────────────────────────────────────────────────────────────

export interface AgentLogPayload {
  level: "debug" | "info" | "warn" | "error";
  message: string;
  extra_fields_json: string | null;
}

export interface ThoughtPayload {
  text: string;
}

export interface LlmCallStartedPayload {
  step_id: string | null;
  backend: string;
  model: string;
  messages_count: number;
  prompt_chars: number;
}

export interface LlmCallFailedPayload {
  step_id: string | null;
  backend: string;
  model: string;
  error: string;
  /** ErrorAnalysis (category, severity, hint...), opaque to the UI for now. */
  analysis: Record<string, unknown>;
}

export interface ToolCallStartedPayload {
  tool_name: string;
  /** Serialised JSON. Can be null when capture_tool_args = false. */
  args_json: string | null;
}

export interface ToolCallCompletedPayload {
  tool_name: string;
  /** Serialised JSON. Null when capture_tool_outputs = false, or the tool has no output. */
  output_json: string | null;
  exit_code: number | null;
  duration_ms: number;
  success: boolean;
}

export interface ToolCallDeniedPayload {
  tool_name: string;
  reason:
    | "not_in_manifest"
    | "permission_denied"
    | "hitl_rejected"
    | "circuit_open"
    | "other";
  detail: string | null;
}

export interface A2AInvokeStartedPayload {
  skill_id: string;
  child_task_id: string | null;
}

export interface A2AInvokeCompletedPayload {
  skill_id: string;
  success: boolean;
  output_summary: string | null;
  duration_ms: number;
}

export interface RetryPayload {
  cause: "action_parse_error" | "tool_error" | "llm_error" | "other";
  attempt: number;
}

export interface ActionParseErrorPayload {
  raw_content: string;
  repair_attempted: boolean;
}

// ─────────────────────────────────────────────────────────────────
// Discriminated union
// ─────────────────────────────────────────────────────────────────

/** Fields common to every trace event. */
interface RuntimeEventBase {
  /** UUID v7 - chronologically ordered primary key. */
  eventId: string;
  /** The task concerned. */
  taskId: string;
  /** The emitting agent. */
  agentId: string;
  /** Parent link (tool_call_completed -> started, A2A child -> invoke). */
  parentEventId: string | null;
  /** Id shared across one A2A chain. */
  correlationId: string | null;
  /** ReAct turn (NULL outside the loop). */
  stepNum: number | null;
  /** ISO 8601 RFC 3339, milliseconds. */
  ts: string;
}

/** Discriminated union - narrowing on `kind`. */
export type RuntimeEventDto =
  | (RuntimeEventBase & { kind: "agent_log"; payload: AgentLogPayload })
  | (RuntimeEventBase & { kind: "thought"; payload: ThoughtPayload })
  | (RuntimeEventBase & { kind: "llm_call_started"; payload: LlmCallStartedPayload })
  | (RuntimeEventBase & { kind: "llm_call_failed"; payload: LlmCallFailedPayload })
  | (RuntimeEventBase & { kind: "tool_call_started"; payload: ToolCallStartedPayload })
  | (RuntimeEventBase & { kind: "tool_call_completed"; payload: ToolCallCompletedPayload })
  | (RuntimeEventBase & { kind: "tool_call_denied"; payload: ToolCallDeniedPayload })
  | (RuntimeEventBase & { kind: "a2a_invoke_started"; payload: A2AInvokeStartedPayload })
  | (RuntimeEventBase & { kind: "a2a_invoke_completed"; payload: A2AInvokeCompletedPayload })
  | (RuntimeEventBase & { kind: "retry"; payload: RetryPayload })
  | (RuntimeEventBase & { kind: "action_parse_error"; payload: ActionParseErrorPayload })
  // Open fallback for the variants added later, so a new one does not force
  // a compile error on the front side (forward compat).
  | (RuntimeEventBase & { kind: string; payload: Record<string, unknown> });

/** Paginated response of `invoke("get_task_trace", ...)`. */
export interface TraceResponse {
  taskId: string;
  events: RuntimeEventDto[];
  /** Cursor to pass as `since` on the next call to fetch the rest. */
  nextCursor: string | null;
}

/** Parameters of `invoke("get_task_trace", ...)`. */
export interface GetTraceParams {
  taskId: string;
  since?: string | null;
  limit?: number | null;
}
