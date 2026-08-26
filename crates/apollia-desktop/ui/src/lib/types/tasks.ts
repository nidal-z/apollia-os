// ─── Tasks, HITL approvals, task timeline ───

/** Résumé d'une tâche pour l'affichage. */
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

/** Approbation en attente HITL. */
export interface PendingApproval {
  task_id: string;
  agent_name: string;
  prompt: string;
  context?: Record<string, unknown>;
  suspended_at: string;
}

/** Approbation résolue (approuvée ou rejetée) pour l'historique. */
export interface ResolvedApproval {
  task_id: string;
  agent_name: string;
  approved: boolean;
  reason: string | null;
  wait_duration_ms: number | null;
  responded_at: string | null;
}

/** Entrée historique d'une approbation HITL de chat. */
export interface ResolvedChatApproval {
  session_id: string;
  message_id: string;
  tool_name: string;
  /** "accept" | "refuse" | "always_accept" */
  decision: string;
  /** ISO-8601 timestamp. */
  resolved_at: string;
  /** Raison fournie par l'opérateur (refus uniquement). */
  reason: string | null;
}

/** État de la connexion SSE. */
export type ConnectionStatus =
  | "connecting"
  | "connected"
  | "reconnecting"
  | "error";

/** Payload JSON d'un événement HITL TaskInputRequired via SSE. */
export interface HitlInputRequiredPayload {
  event_type: "TaskInputRequired";
  task_id: string;
  prompt: string;
  step_id?: string;
}

/** Payload JSON d'un événement HITL TaskResumed via SSE. */
export interface HitlResumedPayload {
  event_type: "TaskResumed";
  task_id: string;
  approved: boolean;
}

/** Type discriminant pour les événements de timeline (snake_case JSON tag). */
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

/** Événement de la timeline d'une tâche (union discriminée par `type`). */
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

/** Filtre pour la commande list_tasks Tauri IPC. */
export interface TaskFilter {
  status?: string;
  agent_id?: string;
}
