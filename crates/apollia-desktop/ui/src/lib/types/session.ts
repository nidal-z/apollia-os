// ─── Global timeline, session meta-layer, audit trail ───

/** Événement de la timeline globale. */
export interface GlobalTimelineEvent {
  event_type: "task" | "tool" | "llm" | "hitl" | (string & {});
  timestamp: string;
  summary: string;
  detail: Record<string, unknown>;
}

/** Paramètres pour la commande get_global_timeline. */
export interface TimelineParams {
  window_minutes: number;
}

// ─────────────────────────────────────────────
// Session meta-layer
// ─────────────────────────────────────────────

/** Catégorie d'événement session affichée dans le scrubber. */
export type SessionEventKind = "tool" | "memory" | "hitl" | "a2a" | "error";

/** Événement persisté pour le scrubber + replay. */
export interface SessionEvent {
  ts: string;
  kind: SessionEventKind;
  label: string;
  correlation_id: string | null;
  payload_json: unknown;
}

/** Entrée de l'audit trail. */
export interface AuditTrailEntry {
  id: string;
  tool_name: string;
  agent_id: string;
  /** Nom lisible de l'agent résolu depuis le registre (ex: "standup-scribe"). */
  agent_name: string;
  timestamp: string;
  duration_ms: number | null;
  exit_code: number | null;
  args_json: string | null;
  stdout: string | null;
  stderr: string | null;
}

/** Entrée coût journalier par backend. */
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

/** Entrée clé/valeur d'une section de configuration. */
export interface ConfigEntry {
  key: string;
  value: string;
}

/** Section de configuration regroupée par thème. */
export interface ConfigSection {
  name: string;
  description: string;
  entries: ConfigEntry[];
  redirect_route: string | null;
}

/** Vue plate de la configuration Apollia OS. */
export interface ApollaConfigView {
  config_path: string;
  config_exists: boolean;
  sections: ConfigSection[];
}

/** Informations système pour la section Avancé de Settings. */
export interface SystemInfo {
  version: string;
  os: string;
  python_path: string | null;
  /** Répertoire de données résolu (`<home>/.apollia`), null si le home est introuvable. */
  data_dir: string | null;
}

/** Confinement OS appliqué aux processus enfants des outils natifs. */
export type ToolSandbox = "linux_namespaces" | "dev_no_sandbox";

/** Modele d'execution du code agent. */
export type AgentExecution = "in_process_trusted";

/** Posture de securite active, refletee depuis apollia-core. */
export interface SecurityPosture {
  platform: string;
  tool_sandbox: ToolSandbox;
  agent_execution: AgentExecution;
  rlimits_active: boolean;
  unshare_available: boolean;
}
