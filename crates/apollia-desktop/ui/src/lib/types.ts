/** Statut d'un agent dans le runtime. */
export interface AgentStatus {
  id: string;
  name: string;
  state: "initializing" | "active" | "degraded" | "stopping" | "stopped";
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

/** Approbation résolue (approuvée ou rejetée) pour l'historique. */
export interface ResolvedApproval {
  task_id: string;
  agent_name: string;
  approved: boolean;
  reason: string | null;
  wait_duration_ms: number | null;
  responded_at: string | null;
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
  | "task_completed";

/** Événement de la timeline d'une tâche (union discriminée par `type`). */
export type TimelineEvent =
  | { type: "task_transition"; status: string; timestamp: string }
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
    };

/** Filtre pour la commande list_tasks Tauri IPC. */
export interface TaskFilter {
  status?: string;
  agent_id?: string;
}

/** Statut d'un backend LLM. */
export interface LlmBackendStatus {
  name: string;
  backend_type: "embedded" | "api";
  model: string;
  status: "ready" | "loading" | "error";
}

/** Résultat d'un ping LLM. */
export interface LlmPingResult {
  backend: string;
  available: boolean;
  latency_ms: number | null;
  error: string | null;
}

/** Ligne de statistiques coût/tokens. */
export interface LlmCostStatsRow {
  backend: string;
  model: string;
  call_count: number;
  total_tokens: number;
  total_cost_usd: number;
}

/** Réponse agrégée des statistiques coût/tokens. */
export interface LlmCostStatsResponse {
  rows: LlmCostStatsRow[];
  days: number;
}

/** Statut d'un trigger configuré. */
export interface TriggerStatus {
  id: string;
  agent: string;
  source_kind: "cron" | "interval" | "file_watch" | "webhook" | "oneshot";
  enabled: boolean;
  fire_count: number;
  skip_count: number;
  last_fired: string | null;
}

/** Entrée d'historique d'un trigger. */
export interface TriggerLogEntry {
  id: string;
  trigger_id: string;
  agent_name: string;
  fired_at: string;
  task_id: string | null;
  status: "fired" | "skipped" | "error";
  reason: string | null;
}

/** Résultat d'un fire manuel de trigger. */
export interface TriggerFireResult {
  task_id: string;
}

/** Résultat du rechargement de la config triggers. */
export interface TriggerReloadResult {
  reloaded: number;
}

/** Résumé d'un pipeline run pour l'affichage. */
export interface PipelineRunSummary {
  run_id: string;
  pipeline_id: string;
  status: "running" | "waiting_approval" | "completed" | "failed";
  started_at: string | null;
  ended_at: string | null;
}

/** Step d'un pipeline run. */
export interface PipelineStepSummary {
  step_id: string;
  status: "pending" | "running" | "completed" | "failed";
  output: string | null;
  error: string | null;
  started_at: string | null;
  ended_at: string | null;
}

/** Détail complet d'un pipeline run avec ses steps. */
export interface PipelineRunDetail {
  run_id: string;
  pipeline_id: string;
  status: string;
  step_runs: PipelineStepSummary[];
  started_at: string;
  ended_at: string | null;
}

/** Pipeline disponible pour lancement. */
export interface PipelineInfo {
  id: string;
  description: string;
}

/** Résultat du lancement d'un pipeline run. */
export interface RunPipelineResult {
  run_id: string;
  pipeline_id: string;
  status: string;
}

/** Entrée mémoire unifiée (episodic | semantic | procedural). */
export interface MemoryEntry {
  id: string;
  entry_type: "episodic" | "semantic" | "procedural";
  key: string;
  value: string;
  created_at: string;
  expires_at: string | null;
  score: number | null;
}

/** Résultat de recherche FTS5 mémoire. */
export interface MemorySearchResult {
  id: string;
  entry_type: "episodic" | "semantic";
  content: string;
  score: number;
  relevance: number | null;
  created_at: string;
}

/** Canal de notification configuré. */
export interface NotificationChannel {
  channel_id: string;
  type: "desktop" | "webhook" | "sse";
  enabled: boolean;
  events: string[];
}

/** Résultat du test d'un canal de notification. */
export interface ChannelTestResult {
  channel_id: string;
  status: "ok" | "error" | "disabled";
  error: string | null;
  latency_ms: number | null;
}

/** Entrée de l'historique des notifications. */
export interface NotificationLogEntry {
  id: string;
  event_name: string;
  task_id: string | null;
  sent_at: string;
  channels: Record<string, string>;
  error: string | null;
}

/** Événement de la timeline globale (STORY-148, AC-1/AC-2). */
export interface GlobalTimelineEvent {
  event_type: "task" | "tool" | "llm" | "hitl" | string;
  timestamp: string;
  summary: string;
  detail: Record<string, unknown>;
}

/** Paramètres pour la commande get_global_timeline. */
export interface TimelineParams {
  window_minutes: number;
}

/** Entrée de l'audit trail (STORY-148, AC-4). */
export interface AuditTrailEntry {
  id: string;
  tool_name: string;
  agent_id: string;
  timestamp: string;
  duration_ms: number | null;
  exit_code: number | null;
  args_json: string | null;
  stdout: string | null;
  stderr: string | null;
}

/** Entrée coût journalier par backend (STORY-148, AC-3). */
export interface LlmDailyCostEntry {
  date: string;
  backend: string;
  cost_usd: number;
}

/** Réponse des coûts journaliers LLM. */
export interface LlmDailyCostsResponse {
  entries: LlmDailyCostEntry[];
  days: number;
}

/** Entrée clé/valeur d'une section de configuration (STORY-149). */
export interface ConfigEntry {
  key: string;
  value: string;
}

/** Section de configuration regroupée par thème (STORY-149). */
export interface ConfigSection {
  name: string;
  description: string;
  entries: ConfigEntry[];
  redirect_route: string | null;
}

/** Vue plate de la configuration Apollia OS (STORY-149). */
export interface ApollaConfigView {
  config_path: string;
  config_exists: boolean;
  sections: ConfigSection[];
}
