/** Statut d'un agent dans le runtime. */
export interface AgentStatus {
  id: string;
  name: string;
  state:
    | "initializing"
    | "active"
    | "degraded"
    | "stopping"
    | "stopped";
  uptime_secs: number;
  tasks_completed: number;
  tasks_failed: number;
  degraded_reason?: string;
}

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

/** État de la connexion SSE. */
export type ConnectionStatus =
  | "connecting"
  | "connected"
  | "reconnecting"
  | "error";

/** Snapshot renvoyé par GET /api/v1/dashboard/state. */
export interface DashboardState {
  agents: DashboardAgent[];
  recent_tasks: DashboardTask[];
  timestamp: string;
}

/** Agent tel que renvoyé par le endpoint dashboard/state. */
export interface DashboardAgent {
  id: string;
  status: string;
  task_count: number;
}

/** Tâche telle que renvoyée par le endpoint dashboard/state. */
export interface DashboardTask {
  id: string;
  agent: string;
  status: string;
  started_at: string;
}

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
