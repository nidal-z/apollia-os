// ─── Triggers ───

/** Statut d'un trigger configuré. */
export interface TriggerStatus {
  id: string;
  agent: string;
  source_kind: "cron" | "interval" | "file_watch" | "webhook" | "oneshot";
  source_config: string;
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

/** Vue complète d'un trigger retournée par les opérations CRUD. */
export interface TriggerDefinitionView {
  id: string;
  agent: string | null;
  enabled: boolean;
  on_busy: "queue" | "drop";
  source_type: "cron" | "interval" | "oneshot" | "file_watch" | "webhook";
  source_config: Record<string, unknown>;
  input_template: string | null;
  created_at: string;
  updated_at: string;
}

/** Configuration source dans les requêtes CRUD trigger. */
export type TriggerSourceInput =
  | { type: "cron"; schedule: string }
  | { type: "interval"; every: string }
  | { type: "oneshot"; fire_at: string }
  | { type: "file_watch"; path: string; events: string[]; recursive?: boolean }
  | { type: "webhook"; secret: string };

/** Corps de requête pour la création d'un trigger. */
export interface CreateTriggerRequest {
  id: string;
  agent?: string;
  enabled: boolean;
  on_busy: "queue" | "drop";
  source: TriggerSourceInput;
  input_template?: string;
}

/** Corps de requête pour la mise à jour d'un trigger. */
export interface UpdateTriggerRequest {
  agent?: string;
  enabled?: boolean;
  on_busy?: "queue" | "drop";
  source: TriggerSourceInput;
  input_template?: string;
}
