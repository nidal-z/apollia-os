// ─── Thinking, decisions, session metrics, ask_user ───

// ── Thinking / Reasoning transparency ────────────────────────

/** Payload of the `ThinkingStarted` runtime event. */
export interface ThinkingStartedEvent {
  turn_id: string;
  ts_ms: number;
}

/** Payload of the `ThinkingEnded` runtime event. */
export interface ThinkingEndedEvent {
  turn_id: string;
  ts_ms: number;
  duration_ms: number;
  raw_content: string;
  tokens: number;
}

/** Coarse error category used by `<ErrorCard />` to pick icon + color. */
export type ErrorCategory =
  | "tool_failure"
  | "llm_error"
  | "timeout"
  | "null_output"
  | "malformed_output"
  | "permission_denied"
  | "network_error"
  | "hallucination_suspected"
  | "unknown";

/** Structured error analysis attached to failure-bearing runtime events. */
export interface ErrorAnalysis {
  category: ErrorCategory;
  human_message: string;
  suggested_action?: string | null;
  hallucination_suspected: boolean;
  technical_details: string;
}

/** Quality assessment produced by `MetaRoutine::GenerateThinkingSummary`. */
export type ThinkingQuality = "low" | "medium" | "high";

/** Reference to a prior turn whose reasoning contradicts the current one. */
export interface ThinkingContradiction {
  turn_id: string;
  excerpt: string;
}

/** Structured summary of a thinking trace (produced by the meta LLM). */
export interface ThinkingSummary {
  summary: string;
  quality: ThinkingQuality;
  contradiction_with_previous: ThinkingContradiction | null;
}

/** Per-turn thinking state managed client-side. */
export interface ThinkingState {
  turn_id: string;
  started_ms: number;
  ended_ms: number | null;
  raw_content: string;
  tokens: number;
  summary: ThinkingSummary | null;
}

// ── Decision branches ───────────────────────────

/** Coarse kind of significant decision the agent made this turn. */
export type DecisionKind =
  | "tool_choice"
  | "agent_delegate"
  | "memory_write"
  | "significant";

/** One alternative path the agent weighed but did not take. */
export interface ConsideredAlternative {
  label: string;
  rejected_reason: string;
  /** Signed gap in confidence vs the chosen path - expected ≤ 0. */
  confidence_delta: number;
}

/** Decision point captured from the thinking trace (≤ 3 alternatives). */
export interface DecisionPoint {
  turn_id: string;
  kind: DecisionKind;
  chosen: string;
  alternatives: ConsideredAlternative[];
}

/** Payload of the `DecisionPointRecorded` runtime event. */
export interface DecisionPointRecordedEvent {
  point: DecisionPoint;
}

// ── Memory injection visibility ─────────────────

/**
 * Memory entry the agent injected into a specific turn.
 *
 * Populated by the PyO3 `recall_entry()` / `recall_all()` wrappers and
 * surfaced via the Tauri command `get_injected_memory_entries(turn_id)`.
 */
export interface InjectedEntry {
  id: string;
  content_preview: string;
  namespace: string;
  injection_reason: string;
  /** Clamped to [0, 1]. */
  relevance_score: number;
}

// ── Session metrics ────────────────────────────────

/** Timing of one tool call, with the delta against the static hint. */
export interface ToolTiming {
  tool_name: string;
  expected_ms: number | null;
  actual_ms: number;
  delta_pct: number | null;
}

/** Context compaction event. */
export interface SummarizationEvent {
  messages_summarized_count: number;
  tokens_saved: number;
  summary_excerpt: string;
}

/** Aggregated snapshot of the metrics of one session. */
export interface SessionMetrics {
  tokens_in: number;
  tokens_out: number;
  tokens_cached: number;
  tokens_meta: number;
  context_window_used: number;
  context_window_max: number;
  token_budget: number;
  tool_timings: ToolTiming[];
  summarization_events: SummarizationEvent[];
}

/** Alert level on the token budget. */
export type BudgetAlertLevel = "ok" | "warning" | "block";

/** Payload of the `SessionMetricsUpdated` runtime event. */
export interface SessionMetricsUpdatedEvent {
  session_id: string;
  metrics: SessionMetrics;
  alert: BudgetAlertLevel;
}

// ─── ask_user tool - dynamic form payload ─────────────────────────────────

/** Type of question an agent asks through the `ask_user` tool. */
export type AskUserQuestionType = "open" | "single_choice" | "multi_choice";

/** One individual question inside an `ask_user` request. */
export interface AskUserQuestion {
  id: string;
  question: string;
  type: AskUserQuestionType;
  options: string[];
  hint?: string;
}

/** Operator answer to an `ask_user` question. */
export interface AskUserAnswer {
  id: string;
  /** Single value for `open` or `single_choice` questions (null = no answer). */
  value?: string | null;
  /** Multiple values for `multi_choice` questions. */
  values?: string[];
  /** `true` when the operator did not answer (soft validation). */
  skipped: boolean;
}
