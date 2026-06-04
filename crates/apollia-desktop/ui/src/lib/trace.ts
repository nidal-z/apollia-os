/**
 * Types pour la trace d'exécution event-sourced.
 *
 * Discriminated union sur `kind` : chaque variant a un payload typé. Les
 * `RuntimeEventDto` proviennent soit de `invoke("get_task_trace", ...)`
 * (replay paginé), soit du channel Tauri `"trace-event"` (live SSE).
 *
 * Les payloads miroir 1:1 le mapping côté Rust dans
 * `crates/apollia-runtime/src/observability/persistor.rs::event_to_record`.
 */

// ─────────────────────────────────────────────────────────────────
// Payloads typés par kind
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
  /** ErrorAnalysis (catégorie, sévérité, hint…) - opaque côté UI pour l'instant. */
  analysis: Record<string, unknown>;
}

export interface ToolCallStartedPayload {
  tool_name: string;
  /** JSON sérialisé. Peut être null si capture_tool_args = false. */
  args_json: string | null;
}

export interface ToolCallCompletedPayload {
  tool_name: string;
  /** JSON sérialisé. Peut être null si capture_tool_outputs = false ou tool sans output. */
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

/** Champs communs à tous les événements de trace. */
interface RuntimeEventBase {
  /** UUID v7 - clé primaire ordonnée chronologiquement. */
  eventId: string;
  /** Tâche concernée. */
  taskId: string;
  /** Agent émetteur. */
  agentId: string;
  /** Lien parent (tool_call_completed → started, A2A child → invoke). */
  parentEventId: string | null;
  /** ID partagé sur une chaîne A2A. */
  correlationId: string | null;
  /** Tour ReAct (NULL hors loop). */
  stepNum: number | null;
  /** ISO 8601 RFC 3339 millisecondes. */
  ts: string;
}

/** Discriminated union - narrowing par `kind`. */
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
  // Fallback ouvert pour les variants ajoutés par les Lots futurs sans
  // forcer un compile-error côté front (forward compat).
  | (RuntimeEventBase & { kind: string; payload: Record<string, unknown> });

/** Réponse paginée de `invoke("get_task_trace", ...)`. */
export interface TraceResponse {
  taskId: string;
  events: RuntimeEventDto[];
  /** Curseur à passer en `since` au prochain appel pour récupérer la suite. */
  nextCursor: string | null;
}

/** Paramètres de `invoke("get_task_trace", ...)`. */
export interface GetTraceParams {
  taskId: string;
  since?: string | null;
  limit?: number | null;
}
