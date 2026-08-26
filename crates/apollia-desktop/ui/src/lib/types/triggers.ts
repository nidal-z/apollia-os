// ─── Triggers ───

/** Status of one configured trigger. */
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

/** History entry of one trigger. */
export interface TriggerLogEntry {
  id: string;
  trigger_id: string;
  agent_name: string;
  fired_at: string;
  task_id: string | null;
  status: "fired" | "skipped" | "error";
  reason: string | null;
}

/** Result of firing a trigger by hand. */
export interface TriggerFireResult {
  task_id: string;
}

/** Result of reloading the triggers config. */
export interface TriggerReloadResult {
  reloaded: number;
}

/** Full view of one trigger returned by the CRUD operations. */
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

/** Source configuration in the trigger CRUD requests. */
export type TriggerSourceInput =
  | { type: "cron"; schedule: string }
  | { type: "interval"; every: string }
  | { type: "oneshot"; fire_at: string }
  | { type: "file_watch"; path: string; events: string[]; recursive?: boolean }
  | { type: "webhook"; secret: string };

/** Request body creating a trigger. */
export interface CreateTriggerRequest {
  id: string;
  agent?: string;
  enabled: boolean;
  on_busy: "queue" | "drop";
  source: TriggerSourceInput;
  input_template?: string;
}

/** Request body updating a trigger. */
export interface UpdateTriggerRequest {
  agent?: string;
  enabled?: boolean;
  on_busy?: "queue" | "drop";
  source: TriggerSourceInput;
  input_template?: string;
}
