// ─── Agents, A2A skills, templates ───

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
    | (string & {})
    | null;
  /**
   * Namespace mémoire primaire déclaré dans le manifest Python. `null` si
   * l'agent n'a pas de mémoire persistante.
   *
   * Plusieurs agents d'un même package partagent souvent le même namespace
   * (ex. : tous les agents du package `veille-ia` déclarent
   * `memory_namespace = "veille-ia"`), donc cette valeur n'est jamais
   * dérivée de `name` - elle vient toujours du manifest.
   */
  memory_namespace: string | null;
  /** Namespaces mémoire partagés accessibles en lecture par l'agent. */
  shared_memory_namespaces: string[];
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
