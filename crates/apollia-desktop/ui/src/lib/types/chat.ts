// ─── Chat sessions and messages ───

import type { ErrorAnalysis } from "./reasoning";

// ─── Chat ───────────────────────────────────────────────────────────────────

/** Résumé d'une session de chat pour la liste. */
export interface ChatSessionSummary {
  id: string;
  mode: "libre" | "agent";
  agent_name: string | null;
  status: "active" | "processing" | "closed";
  last_message_preview: string | null;
  message_count: number;
  created_at: string;
  closed_at: string | null;
  title: string | null;
  project_id: string | null;
}

/** Détail complet d'une session de chat avec messages. */
export interface ChatSessionDetail {
  id: string;
  mode: "libre" | "agent";
  agent_name: string | null;
  system_prompt: string;
  status: "active" | "processing" | "closed";
  available_tools: string[];
  authorized_tools: string[];
  messages: ChatMessageView[];
  created_at: string;
  closed_at: string | null;
  llm_backend: string | null;
  title: string | null;
  project_id: string | null;
  /** Whether the session runs in first-class conversational plan mode. */
  plan_mode: boolean;
  /** Plan-mode lifecycle phase as a stable lowercase string. */
  plan_phase: PlanPhase;
}

/** Plan-mode lifecycle phase of a chat session. */
export type PlanPhase =
  | "discovery"
  | "drafting"
  | "awaiting_approval"
  | "executing"
  | "done";

/** Payload pour mettre à jour la configuration d'une session. */
export interface UpdateSessionRequest {
  system_prompt?: string;
  tools?: string[];
  llm_backend?: string | null;
}

/** Message individuel dans une session de chat. */
export interface ChatMessageView {
  id: string;
  role: "user" | "assistant" | "system" | "tool";
  content: string;
  tool_calls: ToolCallView[] | null;
  tool_name: string | null;
  seq: number;
  created_at: string;
  metadata?: Record<string, unknown> | null;
}

/** Appel d'outil dans un message de chat. */
export interface ToolCallView {
  tool_name: string;
  input: Record<string, unknown>;
  output: string | null;
  /**
   * `failed` means the call ran and errored, `refused` that a human stopped it
   * before it ran. Mirrors `ToolCallStatus` in the runtime.
   */
  status: "pending" | "authorized" | "executed" | "refused" | "failed";
  /** Execution duration in milliseconds, available after the call completes. */
  duration_ms?: number | null;
  /** Process exit code, available when the tool produces one (e.g. bash_executor). */
  exit_code?: number | null;
  /** Meta-LLM narration generated before execution (opt-in). */
  rationale?: ToolCallRationale | null;
  /** Retry chain captured for this invocation. */
  retry_attempts?: RetryAttempt[];
}

/** Outcome of a single attempt in a retry / fallback chain. */
export type AttemptOutcome =
  | { kind: "success" }
  | { kind: "failed" }
  | { kind: "timed_out" }
  | { kind: "fallback"; to: string };

/** One attempt in a retry / fallback chain captured for a tool call. */
export interface RetryAttempt {
  attempt_number: number;
  started_at: number;
  ended_at: number;
  outcome: AttemptOutcome;
  reason?: ErrorAnalysis | null;
}

/** LLM fallback snapshot - emitted when the router switches provider. */
export interface LlmFallback {
  from_provider: string;
  to_provider: string;
  reason: string;
}

/** Structured meta-LLM narration attached to a tool call. */
export interface ToolCallRationale {
  /** One short sentence (<= 25 words) on WHY the agent calls this tool. */
  summary: string;
  /** 2-4 key `[key, short_value]` entries in agent-specified order. */
  inputs_recap: Array<[string, string]>;
  /** One short sentence on WHAT the agent expects from the call. */
  expected_outcome: string;
  /** Optional performance hint (expected duration or faster alternative). */
  performance_hint?: string | null;
}

/** Requête de création d'une session de chat. */
export interface CreateSessionRequest {
  mode: "libre" | "agent";
  agent_name?: string;
  system_prompt?: string;
  tools?: string[];
  project_id?: string;
}

/** Requête d'envoi de message dans une session de chat. */
export interface SendMessageRequest {
  content: string;
}

/** Requête d'autorisation d'un appel d'outil. */
export interface ToolAuthorizationRequest {
  message_id: string;
  tool_name: string;
  decision: "accept" | "refuse" | "always_accept";
}

/** Pending chat tool approval - tracked globally so it survives page navigation. */
export interface PendingChatApproval {
  sessionId: string;
  messageId: string;
  /** Unique id of the tool call, correlating the request with its resolution
   *  so the same tool invoked twice in one turn never collides. */
  toolCallId: string;
  toolName: string;
  inputPreview: string;
  receivedAt: string;
}

/** Pending ask_user request - tracked globally so it survives page navigation. */
export interface PendingUserInputView {
  request_id: string;
  session_id: string;
  questions_json: string;
  context: string | null;
  created_at: string;
}
