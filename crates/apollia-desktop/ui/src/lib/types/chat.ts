// ─── Chat sessions and messages ───

import type { ErrorAnalysis } from "./reasoning";

// ─── Chat ───────────────────────────────────────────────────────────────────

/** Summary of one chat session, for the list. */
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
  /** Tokens the model generated across the answers of the session. */
  output_tokens?: number;
}

/** Full detail of one chat session, with its messages. */
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
  /** Whether the session runs in first-class conversational plan mode. */
  plan_mode: boolean;
  /** Plan-mode lifecycle phase as a stable lowercase string. */
  plan_phase: PlanPhase;
  /**
   * Profile the conversation designates; absent or `null` when it follows
   * the current default profile.
   */
  profile_id?: string | null;
  /**
   * Profile switch of the conversation: whether the built-in assistant
   * receives the profile in its prompt. Absent reads as on.
   */
  use_profile?: boolean;
}

/** Plan-mode lifecycle phase of a chat session. */
export type PlanPhase =
  | "discovery"
  | "drafting"
  | "awaiting_approval"
  | "executing"
  | "done";

/** Payload updating the configuration of one session. */
export interface UpdateSessionRequest {
  system_prompt?: string;
  tools?: string[];
  llm_backend?: string | null;
  /** Profile of the conversation; `null` returns it to the default profile. */
  profile_id?: string | null;
  /** Profile switch of the conversation. */
  use_profile?: boolean;
}

/** What the model of a conversation can do, as the composer reads it. */
export interface ChatModelCapabilities {
  /** The backend or model the conversation runs on, `null` when none. */
  model: string | null;
  local: boolean;
  reasoning_profile: "none" | "toggle" | "levels" | null;
  /** The model reads images. */
  vision: boolean;
  /** Image formats its engine decodes, empty without vision. */
  image_formats: string[];
}

/** A level of reasoning, for a model that reads one. */
export type ReasoningEffort = "low" | "medium" | "high";

/**
 * What a conversation changes from its agent. A field left out is inherited;
 * the runtime refuses a field it does not declare, so a write sends back
 * every field that was read.
 */
export interface SessionSettings {
  preset?: number;
  /** Reasoning on or off, for a model that reads it. */
  thinking?: boolean;
  reasoning_effort?: ReasoningEffort;
  /** The reasoning of the earlier answers goes back to the model; left out, it does not. */
  preserve_thinking?: boolean;
  temperature?: number;
  max_tokens?: number;
  context_overflow?: "summarize_middle" | "sliding_window" | "stop_at_limit";
  instructions?: string;
  disabled_accesses?: string[];
  proposed_accesses?: string[];
  profile_access?: boolean;
}

/** One stored image of a conversation, its media type and base64 bytes. */
export interface ChatImageData {
  mime_type: string;
  data: string;
}

/** One document sent with a message: its file name and base64 bytes. */
export interface ChatDocumentData {
  name: string;
  data: string;
}

/** Single message inside a chat session. */
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

/** Tool call inside a chat message. */
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

/** Request creating a chat session. */
export interface CreateSessionRequest {
  mode: "libre" | "agent";
  agent_name?: string;
  system_prompt?: string;
  tools?: string[];
  /** Limits the conversation to `tools`: nothing else of its agent is
   * offered, and a tool the agent does not hold asks at each call. */
  limited_to_tools?: boolean;
  /** The model of the session: the index identifier of a local language
   * model, or `<server>/<model>`. Absent keeps the agent's model. */
  model?: string;
}

/** Request sending a message inside a chat session. */
export interface SendMessageRequest {
  content: string;
}

/** Request authorising a tool call. */
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
  /** The answers the request offers, when it names them. */
  answers?: string[];
  /** The full arguments of the call, behind "see more". */
  detail?: string;
  /** The whole code of a tool that runs code. */
  code?: string | null;
  /** The folder the code runs in. */
  workingDir?: string | null;
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
