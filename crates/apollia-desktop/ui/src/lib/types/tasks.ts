// ─── Tasks, HITL approvals, task timeline ───

/** Summary of one task, for the display. */
export interface TaskSummary {
  id: string;
  agent_id: string;
  agent_name: string;
  status:
    | "submitted"
    | "working"
    | "completed"
    | "failed"
    | "input_required"
    | "canceled";
  input_preview: string;
  output_text?: string;
  duration_ms?: number;
  created_at: string;
}

/** Pending HITL approval. */
export interface PendingApproval {
  task_id: string;
  agent_name: string;
  prompt: string;
  context?: Record<string, unknown>;
  suspended_at: string;
}

/** State of the SSE connection. */
export type ConnectionStatus =
  | "connecting"
  | "connected"
  | "reconnecting"
  | "error";

/** JSON payload of a TaskInputRequired HITL event over SSE. */
export interface HitlInputRequiredPayload {
  event_type: "TaskInputRequired";
  task_id: string;
  prompt: string;
  step_id?: string;
}

/** JSON payload of a TaskResumed HITL event over SSE. */
export interface HitlResumedPayload {
  event_type: "TaskResumed";
  task_id: string;
  approved: boolean;
}

/** Discriminant type of the timeline events (snake_case JSON tag). */
export type TimelineEventType =
  | "task_transition"
  | "step_started"
  | "step_completed"
  | "llm_call"
  | "tool_call"
  | "hitl_suspended"
  | "hitl_resolved"
  | "task_completed"
  | "step_observation"
  | "plan_cache_hit";

/** Event of the timeline of one task (union discriminated by `type`). */
export type TimelineEvent =
  | {
      type: "task_transition";
      status: string;
      execution_mode?: string;
      complexity_score?: number;
      timestamp: string;
    }
  | {
      type: "step_started";
      step_id: string;
      tool?: string;
      input_preview?: string;
      timestamp: string;
    }
  | {
      type: "step_completed";
      step_id: string;
      duration_ms?: number;
      success: boolean;
      model_hint?: string;
      timestamp: string;
    }
  | {
      type: "llm_call";
      backend: string;
      model: string;
      prompt_tokens?: number;
      completion_tokens?: number;
      cost_usd?: number;
      latency_ms?: number;
      timestamp: string;
    }
  | {
      type: "tool_call";
      tool_name: string;
      duration_ms?: number;
      exit_code?: number;
      truncated: boolean;
      input_preview?: string;
      output_preview?: string;
      timestamp: string;
    }
  | { type: "hitl_suspended"; prompt: string; timestamp: string }
  | {
      type: "hitl_resolved";
      approved: boolean;
      reason?: string;
      wait_ms?: number;
      timestamp: string;
    }
  | {
      type: "task_completed";
      output_preview?: string;
      duration_ms?: number;
      timestamp: string;
    }
  | {
      type: "step_observation";
      step_name: string;
      memory_key: string;
      memory_value: string;
      timestamp: string;
    }
  | {
      type: "plan_cache_hit";
      cache_key: string;
      plan_id: string;
      timestamp: string;
    };

/** Filter of the list_tasks Tauri IPC command. */
export interface TaskFilter {
  status?: string;
  agent_id?: string;
}
