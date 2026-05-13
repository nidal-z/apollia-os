// ─── Agent Packages (ADR-081) ─────────────────────────────────────────────

/** Résumé d'un agent dans un package. */
export interface PackageAgentSummary {
  name: string;
  role: "director" | "worker" | "assistant" | string;
  entry: string;
}

/** Élément de la liste des packages installés. */
export interface AgentPackageListItem {
  name: string;
  version: string;
  description: string;
  agent_count: number;
  agents: PackageAgentSummary[];
  installed_at: string;
  root_path: string;
  root_missing: boolean;
}

/** Détail complet d'un package. */
export interface AgentPackageDetailView {
  name: string;
  version: string;
  description: string;
  author: string;
  agents: PackageAgentSummary[];
  installed_at: string;
  updated_at: string;
  root_path: string;
  root_missing: boolean;
  manifest: Record<string, unknown>;
}

/** Preview d'un trigger (dry-run, sans validation stricte). */
export interface TriggerPreview {
  id: string;
  source_type: "cron" | "interval" | "webhook" | "file_watch" | "oneshot" | string;
  agent: string;
  schedule?: string;
  every?: string;
  path?: string;
  needs_config: boolean;
  enabled: boolean;
}

/** Override de configuration pour un trigger (ex : secret webhook). */
export interface TriggerConfigOverride {
  id: string;
  secret?: string;
}

/** Résultat d'un preview (dry-run). */
export interface PackagePreview {
  name: string;
  version: string;
  description: string;
  author: string;
  agents: PackageAgentSummary[];
  triggers: TriggerPreview[];
  trigger_count: number;
  pip_packages: string[];
  valid: boolean;
  error?: string;
}

/** Résultat d'une installation de package. */
export interface InstallPackageResponse {
  name: string;
  version: string;
  agent_count: number;
  trigger_count: number;
  trigger_errors: string[];
}

// ─────────────────────────────────────────────────────────────────────────────

/** Skill exposé par un worker agent via A2A. */
export interface A2ASkillListing {
  skill_id: string;
  agent_name: string;
  skill_name: string;
  description: string;
}

/** Télémétrie agrégée d'un skill A2A sur la fenêtre glissante. */
export interface A2ASkillTelemetry {
  skill_name: string;
  version: string;
  invocations: number;
  avg_latency_ms: number;
  success_rate: number;
  tokens_consumed: number;
}

/** Provenance d'un step dans une chaîne A2A. */
export interface A2AStepProvenance {
  step_id: string;
  input_excerpt: string;
  output_excerpt?: string | null;
  agent_from: string;
  agent_to: string;
  parent_step?: string | null;
  skill_id: string;
  timestamp_ms: number;
}

/** Warning de compatibilité semver entre required et advertised. */
export interface A2ACompatibilityWarning {
  skill_id: string;
  agent_name: string;
  required_version: string;
  advertised_version: string;
  severity: "warning" | "incompatible";
  message: string;
  alternative_agent?: string | null;
}

/** Agent pré-installé découvert dans le répertoire agents/. */
export interface AvailableAgent {
  id: string;
  path: string;
}

/** Statut d'un agent dans le runtime. */
export interface AgentStatus {
  id: string;
  name: string;
  description: string;
  state: "initializing" | "active" | "degraded" | "stopping" | "stopped";
  uptime_secs: number;
  tasks_completed: number;
  tasks_failed: number;
  degraded_reason?: string;
}

/** Élément de la liste unifiée agents (installés + runtime). */
export interface AgentListItem {
  /** UUID runtime (`null` si installé mais pas chargé). */
  id: string | null;
  /** Nom unique de l'agent. */
  name: string;
  /** Version semver. */
  version: string;
  /** Activé pour l'auto-start au boot. */
  enabled: boolean;
  /** État runtime (`null` si non chargé). */
  runtime_status:
    | "active"
    | "degraded"
    | "stopped"
    | "initializing"
    | "stopping"
    | null;
  /** Horodatage d'installation RFC 3339 (`null` pour les agents session-only). */
  installed_at: string | null;
  /** Description humaine de l'agent (du manifest). */
  description: string | null;
  /** Tags libres pour le routing/découverte. */
  tags: string[];
  /** Outils requis par l'agent. */
  tools_required: string[];
  /** Outils optionnels de l'agent. */
  tools_optional: string[];
  /** Mode d'exécution ORIA. */
  execution_mode: "direct" | "orchestrated" | "auto" | null;
  /** Score pondéré pour le classement/dispatch (issu du weighted scoring Observer). */
  weighted_score?: number;
  /** Chemin d'installation sur disque (`null` pour les agents runtime-only). */
  install_path: string | null;
  /** Indique si l'agent supporte la communication inter-agents (A2A). */
  supports_a2a: boolean;
  /** Skills A2A déclarés (vide si supports_a2a est false). */
  skills: AgentSkillView[];
  /** Rôle sémantique de l'agent pour la catégorisation UI. */
  agent_type: "worker" | "assistant" | "system" | null;
  /** Exemples de prompts illustrant les usages typiques (vide = non renseigné). */
  examples: string[];
  /** Limitations explicites : ce que l'agent ne fait pas (vide = non renseigné). */
  limitations: string[];
  /** Note de configuration requise avant la première utilisation (`null` = aucun prérequis). */
  setup_notes: string | null;
  /** Nom de la classe Python source de l'agent (décision D2). */
  agent_class:
    | "ReActAgent"
    | "ConversationalAgent"
    | "OrchestratedAgent"
    | "WorkerAgent"
    | string
    | null;
}

/** Skill A2A déclaré par un agent worker. */
export interface AgentSkillView {
  id: string;
  name: string;
  description: string;
}

/** Réponse d'une installation ou mise à jour d'agent. */
export interface InstallAgentResponse {
  name: string;
  version: string;
  install_path: string;
}

/** Résultat de la création d'un agent depuis un template SDK. */
export interface CreateAgentResult {
  /** Nom de l'agent créé. */
  name: string;
  /** Type de template utilisé. */
  template_type: string;
  /** Chemin du dossier créé sur le disque. */
  path: string;
}

/** Type de template d'agent SDK. */
export type AgentTemplateType = "react" | "conversational" | "orchestrated";

/** Définition d'un template d'agent pour le dialog de création. */
export interface TemplateDefinition {
  /** Identifiant du type de template. */
  type: AgentTemplateType;
  /** Titre affiché. */
  title: string;
  /** Description courte. */
  description: string;
  /** Nom de l'icône Lucide. */
  icon: string;
  /** Couleur ou gradient CSS pour la bordure. */
  color: string;
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

/** Approbation chat résolue pour l'historique. */
export interface ResolvedChatApproval {
  session_id: string;
  tool_name: string;
  decision: string;
  resolved_at: string;
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

/** Backend LLM configuré — vue CRUD retournée par `list_llm_backends`. */
export interface LlmBackendConfig {
  name: string;
  provider: "llama-cpp" | "openai" | "mistral" | "anthropic" | "ollama";
  model: string;
  config_json: Record<string, unknown>;
  enabled: boolean;
  is_default: boolean;
  /** Message d'erreur du dernier ping en RAM (absent = jamais pingé ou dernier OK). */
  last_ping_error?: string | null;
  /** Horodatage RFC 3339 du dernier ping (absent = jamais pingé). */
  last_ping_at?: string | null;
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
  /** Nom affiché libre. `null` ou absent = retombe sur `channel_id`. */
  label?: string | null;
  type: "desktop" | "webhook";
  enabled: boolean;
  events: string[];
  /** Intervalle minimal de throttling, en secondes (`0` = aucun). */
  min_interval_seconds?: number;
}

/** Définition complète d'un canal retournée par les opérations CRUD. */
export interface NotificationChannelView {
  id: string;
  /** Nom affiché libre. `null` = aucun label. */
  label: string | null;
  channel_type: "desktop" | "webhook";
  enabled: boolean;
  config: Record<string, unknown>;
  events: string[] | null;
  /** Intervalle minimal de throttling, en secondes (`0` = aucun). */
  min_interval_seconds: number;
  created_at: string;
  updated_at: string;
}

/** Corps de requête pour la création d'un canal de notification. */
export interface CreateChannelRequest {
  id: string;
  /** Nom affiché (libre, max 80 chars). Omettre si pas de label. */
  label?: string;
  channel_type: "desktop" | "webhook";
  enabled: boolean;
  config: Record<string, unknown>;
  events?: string[];
  /** Intervalle minimal de throttling, en secondes. Omettre = `0` (aucun). */
  min_interval_seconds?: number;
}

/**
 * Corps de requête pour la mise à jour d'un canal de notification.
 *
 * Sémantique du champ `label` :
 * - clé absente → conserver le label existant ;
 * - `label: null` → effacer le label ;
 * - `label: "texte"` → remplacer.
 */
export interface UpdateChannelRequest {
  label?: string | null;
  channel_type?: "desktop" | "webhook";
  enabled?: boolean;
  config?: Record<string, unknown>;
  events?: string[];
  /** Nouvel intervalle de throttling. Omettre = conserver l'existant. */
  min_interval_seconds?: number;
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

/** Événement de la timeline globale. */
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

/** État de lecture du replay, piloté depuis `SessionReplayControls`. */
export interface ReplayState {
  cursor: number;
  total: number;
  playing: boolean;
  speed: number;
}

/** Score de risque d'hallucination agrégé au niveau session. */
export interface HallucinationRisk {
  score: number;
  factors: string[];
}

/** Inputs transmis à `compute_session_meta`. */
export interface SessionHallucinationInputs {
  heuristic_flag_count: number;
  total_tool_outputs: number;
  assertion_citation_gaps: number;
  total_assertions: number;
  thinking_contradictions: number;
}

/** Suggestion actionnable retournée par `GenerateNextSteps` (string courte). */
export type NextStep = string;

/** Réponse agrégée du meta-layer de session. */
export interface SessionMeta {
  hallucination_risk: HallucinationRisk;
  event_count: number;
  summary: string | null;
  title: string | null;
  next_steps: NextStep[] | null;
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
}

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
}

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
  status: "pending" | "authorized" | "executed" | "refused";
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

/** LLM fallback snapshot — emitted when the router switches provider. */
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

/** Pending chat tool approval — tracked globally so it survives page navigation. */
export interface PendingChatApproval {
  sessionId: string;
  messageId: string;
  toolName: string;
  inputPreview: string;
  receivedAt: string;
}

/** Pending ask_user request — tracked globally so it survives page navigation. */
export interface PendingUserInputView {
  request_id: string;
  session_id: string;
  questions_json: string;
  context: string | null;
  created_at: string;
}

// ─── Sprint 20 — Système Agentique Amélioré ─────────────────────────────────

/** Résumé d'un outil pour l'affichage en liste. */
export interface ToolSummary {
  /** Nom unique de l'outil (ex: "bash_executor"). */
  name: string;
  /** Version semver de l'outil. */
  version: string;
  /** Description humaine de l'outil. */
  description: string;
  /** Type d'outil : "native", "mcp", "python". */
  kind: string;
}

/** Vue détaillée d'un outil pour l'introspection (miroir Rust ToolDescriptor). */
export interface ToolDescriptorView {
  /** Nom unique de l'outil (ex: "bash_executor"). */
  name: string;
  /** Version semver de l'outil. */
  version: string;
  /** Description humaine de l'outil. */
  description: string;
  /** Type d'outil : "native", "mcp", "python". */
  kind: string;
  /** JSON Schema d'entrée (`null` si non défini). */
  input_schema: Record<string, unknown> | null;
  /** JSON Schema de sortie (`null` si non défini). */
  output_schema: Record<string, unknown> | null;
  /** Permissions requises par l'outil. */
  permissions: string[];
}

/** Statistiques du cache de plans ORIA. */
export interface PlanCacheStats {
  /** Nombre total d'entrées en cache. */
  total_entries: number;
  /** Nombre total de cache hits depuis le démarrage. */
  cache_hits: number;
  /** Nombre total de cache misses depuis le démarrage. */
  cache_misses: number;
  /** Taux de hit en pourcentage (0-100). */
  hit_rate_pct: number;
  /** Horodatage RFC 3339 de l'entrée la plus ancienne (`null` si cache vide). */
  oldest_entry_at: string | null;
  /** Horodatage RFC 3339 de l'entrée la plus récente (`null` si cache vide). */
  newest_entry_at: string | null;
}

/** Message échangé entre deux agents via la mailbox. */
export interface AgentMessage {
  /** Identifiant unique du message. */
  id: string;
  /** Nom de l'agent expéditeur. */
  from_agent: string;
  /** Nom de l'agent destinataire. */
  to_agent: string;
  /** Contenu du message (JSON arbitraire). */
  payload: Record<string, unknown>;
  /** Horodatage d'envoi RFC 3339. */
  sent_at: string;
}

// ─── User Profile (ADR-087) ───────────────────────────────────────────────────

/** Canonical schema field exposed by get_profile_schema. */
export interface ProfileFieldView {
  key: string;
  label_fr: string;
  label_en: string;
  help_fr: string;
  help_en: string;
  section: "identity" | "work" | "preferences" | "constraints";
  sensitive: boolean;
  field_type: "text" | "long_text" | "select";
  options: string[];
}

/** A single profile entry returned by get_profile. */
export interface ProfileEntryView {
  key: string;
  value: string;
  /** Provenance tag: "onboarding", "user", or "agent:<name>". */
  written_by: string;
  created_at: string;
  updated_at: string;
  in_schema: boolean;
}

/** Aggregated user profile returned by get_profile. */
export interface UserProfileView {
  schema_entries: ProfileEntryView[];
  extras: ProfileEntryView[];
  entries: ProfileEntryView[];
  last_updated_at: string | null;
}

/** Request payload for set_profile_entry. */
export interface SetProfileEntryRequest {
  key: string;
  value: string;
}

/** Statistics for a single chat conversation session. */
export interface ConversationStatsView {
  message_count: number;
  summarized_count: number;
  context_usage_pct: number;
  user_memory_injected: boolean;
  cross_sessions_referenced: number;
}

/** Token-level breakdown of context window usage. */
export interface ContextWindowStats {
  messagesTokens: number;
  userMemoryTokens: number;
  summaryTokens: number;
  totalTokens: number;
  maxTokens: number;
}

/** Insight extracted from a chat conversation by LLM analysis. */
export interface InsightEntry {
  id: string;
  text: string;
  category: "preference" | "habit" | "context";
  confidence: number;
  source: string;
  /** Verbatim quote from the conversation that the insight was extracted from. */
  source_quote?: string;
  /** Short rationale provided by the extraction agent explaining why this insight was kept. */
  extraction_reasoning?: string;
}

/** Outcome of a memory-write proposal. */
export type MemoryWriteOutcome =
  | { kind: "accepted" }
  | { kind: "rejected"; reason: string };

/** An insight that was rejected, kept for audit in the "Rejected" tab. */
export interface RejectedInsightEntry extends InsightEntry {
  rejected_reason: string;
  rejected_at: string;
}

/** Événement de cache hit pour un plan d'exécution. */
export interface PlanCacheHitEvent {
  /** ID de la tâche qui a déclenché le cache hit. */
  task_id: string;
  /** Clé de cache SHA-256 qui a matché. */
  cache_key: string;
  /** ID du plan réutilisé depuis le cache. */
  plan_id: string;
  /** Horodatage RFC 3339 du cache hit. */
  timestamp: string;
}

// ─── Onboarding ───────────────────────────────────────────────────────────────

/** Onboarding completion status returned by get_onboarding_status. */
export interface OnboardingStatus {
  completed: boolean;
  mandatory_complete: boolean;
  topics_covered: string[];
  completion_pct: number;
  last_session_at: string | null;
  skipped: boolean;
}

/** Result of triggering an onboarding session via trigger_onboarding. */
export interface TriggerResult {
  session_id: string;
  mode: "full" | "partial";
  topic?: string;
}

/** Seven ordered phases of the Sprint 33 onboarding flow. */
export type OnboardingPhase =
  | "welcome"
  | "profile_choice"
  | "ai_setup"
  | "acquaintance"
  | "guided_tour"
  | "graduation"
  | "done";

/** Cumulative statistics accumulated across the full onboarding flow. */
export interface OnboardingStats {
  total_time_sec: number;
  actions_completed: number;
  companion_questions: number;
  voice_commands_used: number;
}

/** Full onboarding state returned by get_onboarding_state (Sprint 33). */
export interface OnboardingState {
  phase: OnboardingPhase;
  profile: string | null;
  llm_configured: boolean;
  stt_configured: boolean;
  topics_covered: string[];
  mandatory_complete: boolean;
  tour_step_index: number;
  tour_total_steps: number;
  tour_completed: boolean;
  companion_session_id: string | null;
  voice_enabled: boolean;
  skipped: boolean;
  completed: boolean;
  started_at: string | null;
  completed_at: string | null;
  stats: OnboardingStats;
}

// ─── Guided Tour ─────────────────────────────────────────────────────────────

/** Interaction descriptor attached to an interactive tour step. */
export interface TourInteraction {
  interaction_type: string;
  prefilled_data: Record<string, unknown> | null;
  validation_event: string | null;
}

/**
 * Descriptor for a single guided-tour step returned by `get_tour_steps`.
 *
 * `completion_mode` is one of `"auto"` | `"click_next"` | `"wait_event"`.
 */
export interface TourStep {
  id: string;
  route: string;
  spotlight_selector: string | null;
  companion_message_key: string;
  interaction: TourInteraction | null;
  completion_mode: string;
  estimated_seconds: number;
  /** Visual group for progress rail grouping (e.g. "dashboard"). Null = own group. */
  group: string | null;
}

// ─── Voice commands ──────────────────────────────────────────────────────────

/**
 * Action returned by `process_tour_voice_command`.
 *
 * Mirrors the Rust `TourVoiceAction` enum serialised with `#[serde(tag = "action")]`.
 */
export type TourVoiceAction =
  | { action: "NextStep" }
  | { action: "PreviousStep" }
  | { action: "SkipTour" }
  | { action: "AskCompanion"; message: string }
  | { action: "Unrecognized"; transcript: string };

// ─── STT (Speech-to-Text) ────────────────────────────────────────────────────

/** Description of an available STT model file on disk. */
export interface SttModelInfo {
  name: string;
  path: string;
  size_mb: number;
  language: string | null;
}

/** STT configuration read from / written to the `[stt]` section of apollia.toml. */
export interface SttConfigView {
  enabled: boolean;
  model_path: string;
  hotkey: string;
  clipboard_mode: string;
  clipboard_restore: boolean;
  silence_threshold_db: number;
  max_recording_sec: number;
  language: string | null;
  trigger_mode: string;
}

/** Configuration STT — vue CRUD avec types stricts (miroir de `SttConfigRow`). */
export interface SttConfig {
  enabled: boolean;
  model_path: string;
  hotkey: string;
  clipboard_mode: "paste" | "clipboard";
  clipboard_restore: boolean;
  silence_threshold_db: number;
  max_recording_sec: number;
  language: string | null;
  trigger_mode: "toggle" | "push-to-talk";
}

/** Current status of the STT engine reported by `get_stt_status`. */
export interface SttStatus {
  enabled: boolean;
  model_loaded: boolean;
  model_path: string;
  model_name: string;
  backend_name: string;
  metal_enabled: boolean;
  cuda_enabled: boolean;
}

/** A single transcription row returned by `list_transcriptions`. */
export interface TranscriptRow {
  id: string;
  full_text: string;
  language: string | null;
  source: "hotkey" | "file" | "api";
  audio_duration_ms: number;
  processing_time_ms: number;
  model_name: string | null;
  created_at: string;
}

// ─── MCP Integrations ────────────────────────────────────────────────────────

// ─── HITL Permission Types ────────────────────────────────────────────────────

/** Champs communs à toutes les demandes de permission HITL. */
export interface BasePermission {
  /** Discriminant du type de permission. */
  permission_type: string;
  /** Identifiant unique de la demande de permission. */
  request_id: string;
  /** Identifiant de l'agent à l'origine de la demande. */
  agent_id: string;
}

/** Demande de permission pour l'exécution d'une commande bash. */
export interface BashPermission extends BasePermission {
  permission_type: 'bash';
  /** Commande complète à exécuter. */
  command: string;
  /** Répertoire de travail de la commande. */
  working_dir: string;
}

/** Demande de permission pour l'édition d'un fichier existant. */
export interface FileEditPermission extends BasePermission {
  permission_type: 'file_edit';
  /** Chemin du fichier modifié. */
  file_path: string;
  /** Contenu original (avant modification). */
  old_content: string;
  /** Contenu résultant (après modification). */
  new_content: string;
}

/** Demande de permission pour l'écriture d'un fichier (création ou écrasement). */
export interface FileWritePermission extends BasePermission {
  permission_type: 'file_write';
  /** Chemin du fichier ciblé. */
  file_path: string;
  /** Contenu à écrire. */
  content: string;
  /** Mode : création d'un nouveau fichier ou écrasement d'un existant. */
  mode: 'create' | 'overwrite';
}

/** Demande de permission pour une opération sur le système de fichiers. */
export interface FilesystemPermission extends BasePermission {
  permission_type: 'filesystem';
  /** Type d'opération. */
  operation: 'delete' | 'move' | 'mkdir';
  /** Chemin source de l'opération. */
  path: string;
  /** Chemin destination (uniquement pour `move`). */
  target_path?: string;
}

/** Demande de permission pour l'invocation d'un outil MCP. */
export interface McpPermission extends BasePermission {
  permission_type: 'mcp';
  /** Nom du serveur MCP exposant l'outil. */
  server_name: string;
  /** Nom de l'outil MCP invoqué. */
  tool_name: string;
  /** Arguments transmis à l'outil. */
  arguments: Record<string, unknown>;
}

/** Demande de permission générique (outil non typé). */
export interface GenericPermission extends BasePermission {
  permission_type: 'generic';
  /** Nom de l'outil. */
  tool_name: string;
  /** Entrée brute de l'outil. */
  input: Record<string, unknown>;
}

/** Union discriminée de toutes les demandes de permission HITL. */
export type ApollaPermission =
  | BashPermission
  | FileEditPermission
  | FileWritePermission
  | FilesystemPermission
  | McpPermission
  | GenericPermission;

/** MCP server summary returned by `list_mcp_servers` and `restart_mcp_server`. */
export interface McpServerStatusView {
  name: string;
  server_info: string;
  tools_count: number;
  requires_approval: boolean;
  connected: boolean;
  pid: number | null;
  uptime_secs: number | null;
  last_call_at: string | null;
  error: string | null;
  package: string | null;
  transport: string;
}

/** MCP server detail including its tool list, returned by `get_mcp_server_detail`. */
export interface McpServerDetailView {
  status: McpServerStatusView;
  tools: McpToolSummaryView[];
  config: McpServerConfigView;
}

/** Summary of a single tool exposed by an MCP server. */
export interface McpToolSummaryView {
  full_name: string;
  local_name: string;
  description: string | null;
  input_schema: Record<string, unknown>;
}

/** Read-only configuration of an MCP server with secret values redacted. */
export interface McpServerConfigView {
  name: string;
  command: string;
  args: string[];
  env_keys: string[];
  transport: string;
  requires_approval: boolean;
  tags: string[];
}

/** Trust level badge shown next to a connector in the catalogue. */
export type TrustLevel =
  | "verified_official"
  | "community_verified"
  | "community"
  | "custom";

/** A remote connection endpoint for an MCP registry server. */
export interface RegistryRemoteView {
  /** Transport type: `"streamable-http"` or `"sse"`. */
  type: string;
  /** Connection URL (e.g. `"https://mcp.notion.com/mcp"`). */
  url: string;
  /** HTTP headers required to authenticate or configure the connection. */
  headers: RegistryRemoteHeaderView[];
}

/** A required HTTP header for a remote MCP connection. */
export interface RegistryRemoteHeaderView {
  name: string;
  description: string | null;
  isRequired: boolean;
  isSecret: boolean;
}

/** MCP server entry from the registry catalogue, enriched with trust and install state. */
export interface RegistryServerView {
  name: string;
  title: string | null;
  description: string | null;
  version: string;
  repository_url: string | null;
  website_url: string | null;
  packages: RegistryPackageView[] | null;
  trust_level: TrustLevel;
  category: string | null;
  enrichment: ConnectorEnrichmentView | null;
  is_installed: boolean;
  /** Remote connection endpoints (streamable-http, SSE). Empty for package-only servers. */
  remotes: RegistryRemoteView[];
}

/** An installable package for an MCP registry server. */
export interface RegistryPackageView {
  registry_type: string;
  identifier: string;
  version: string;
  runtime_hint: string | null;
  transport_type: string;
  environment_variables: RegistryEnvVarView[];
  package_arguments: RegistryPackageArgView[];
}

/** An environment variable required by an MCP server package. */
export interface RegistryEnvVarView {
  name: string;
  description: string | null;
  is_required: boolean;
  is_secret: boolean;
}

/** A positional or named argument forwarded to an MCP package at launch. */
export interface RegistryPackageArgView {
  arg_type: string;
  value_hint: string | null;
  description: string | null;
  is_required: boolean;
}

/** UI-friendly metadata enriching a registry server entry for the catalogue. */
export interface ConnectorEnrichmentView {
  operator_label: string;
  category: string;
  icon_name: string;
  trust_level: TrustLevel;
  auth_help_url: string | null;
  auth_help_text: string | null;
  default_requires_approval: boolean;
}

/** Result of testing an MCP server connection without persisting a session. */
export interface McpConnectionTestResultView {
  server_info: string;
  protocol_version: string;
  tools: McpToolSummaryView[];
  test_duration_ms: number;
}

/** Input payload for adding a new MCP server via `add_mcp_server`. */
export interface McpServerConfigInput {
  name: string;
  /** Executable to spawn. Omitted for network-based transports (`streamable-http`, `sse`). */
  command?: string;
  /** Arguments forwarded to the command. Omitted for network-based transports. */
  args?: string[];
  /** Remote server URL. Required for `streamable-http` and `sse` transports. */
  url?: string;
  env: Record<string, string>;
  transport: string;
  requires_approval: boolean;
  tags: string[];
}


// ─── Projects ────────────────────────────────────────────────────────────────

/** Résumé d'un projet dans la liste. */
export interface ProjectSummary {
  id: string;
  name: string;
  description: string | null;
  created_at: string;
  updated_at: string;
  workspace_path: string | null;
}

/** Document attaché à un projet. */
export interface ProjectDocument {
  id: string;
  project_id: string;
  name: string;
  file_path: string;
  size_bytes: number;
  uploaded_at: string;
}

/** Provider de contexte configuré pour un projet. */
export interface ProjectProviderRow {
  id: string;
  project_id: string;
  provider_type: string;
  name: string;
  config_json: string;
  path: string | null;
  enabled: boolean;
  priority: number;
}

/** Détail complet d'un projet. */
export interface ProjectDetail {
  id: string;
  name: string;
  description: string | null;
  instructions: string | null;
  created_at: string;
  updated_at: string;
  workspace_path: string | null;
  documents: ProjectDocument[];
  providers: ProjectProviderRow[];
  agents: string[];
}

/** Template de projet prédéfini. */
export interface ProjectTemplate {
  id: string;
  name: string;
  description: string | null;
  providers_config_json: string;
  is_builtin: boolean;
  created_at: string;
}

/** Payload pour la création d'un projet. */
export interface CreateProjectRequest {
  name: string;
  description?: string;
  instructions?: string;
  workspace_path?: string;
}

/** Payload pour la mise à jour partielle d'un projet. */
export interface UpdateProjectRequest {
  name?: string;
  description?: string | null;
  instructions?: string | null;
  workspace_path?: string | null;
}

/** Section d'un snapshot workspace. */
export interface WorkspaceSectionView {
  source: string;
  title: string;
  content: string;
}

/** Snapshot workspace live d'un projet. */
export interface WorkspaceSnapshotView {
  sections: WorkspaceSectionView[];
  error_count: number;
}

/** Status de l'installation CLI (modèle Docker Desktop). */
export interface CliStatus {
  bundled: boolean;
  bundled_path: string | null;
  installed: boolean;
  symlink_path: string;
  version: string;
  needs_privilege: boolean;
}

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
  /** Signed gap in confidence vs the chosen path — expected ≤ 0. */
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

/** Timing d'un appel outil avec delta par rapport au hint statique. */
export interface ToolTiming {
  tool_name: string;
  expected_ms: number | null;
  actual_ms: number;
  delta_pct: number | null;
}

/** Événement de compaction du contexte. */
export interface SummarizationEvent {
  messages_summarized_count: number;
  tokens_saved: number;
  summary_excerpt: string;
}

/** Snapshot agrégé des métriques d'une session. */
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

/** Niveau d'alerte sur le budget tokens. */
export type BudgetAlertLevel = "ok" | "warning" | "block";

/** Payload du runtime event `SessionMetricsUpdated`. */
export interface SessionMetricsUpdatedEvent {
  session_id: string;
  metrics: SessionMetrics;
  alert: BudgetAlertLevel;
}

// ─── ask_user tool — dynamic form payload ─────────────────────────────────

/** Type de question posée par un agent via le tool `ask_user`. */
export type AskUserQuestionType = "open" | "single_choice" | "multi_choice";

/** Une question individuelle dans une requête `ask_user`. */
export interface AskUserQuestion {
  id: string;
  question: string;
  type: AskUserQuestionType;
  options: string[];
  hint?: string;
}

/** Réponse de l'opérateur à une question `ask_user`. */
export interface AskUserAnswer {
  id: string;
  /** Valeur unique pour les questions `open` ou `single_choice`. */
  value?: string;
  /** Valeurs multiples pour les questions `multi_choice`. */
  values?: string[];
  /** `true` si l'opérateur n'a pas répondu (validation soft). */
  skipped: boolean;
}
