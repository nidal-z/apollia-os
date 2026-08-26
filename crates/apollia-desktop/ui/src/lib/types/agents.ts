// ─── Agents, A2A skills, templates ───

// ─────────────────────────────────────────────────────────────────────────────

/** Skill a worker agent exposes through A2A. */
export interface A2ASkillListing {
  skill_id: string;
  agent_name: string;
  skill_name: string;
  description: string;
}

/** Aggregated telemetry of one A2A skill over the sliding window. */
export interface A2ASkillTelemetry {
  skill_name: string;
  version: string;
  invocations: number;
  avg_latency_ms: number;
  success_rate: number;
  tokens_consumed: number;
}

/** Provenance of one step in an A2A chain. */
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

/** Semver compatibility warning between required and advertised. */
export interface A2ACompatibilityWarning {
  skill_id: string;
  agent_name: string;
  required_version: string;
  advertised_version: string;
  severity: "warning" | "incompatible";
  message: string;
  alternative_agent?: string | null;
}

/** Pre-installed agent discovered in the agents/ directory. */
export interface AvailableAgent {
  id: string;
  path: string;
}

/** Status of one agent in the runtime. */
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

/** Item of the unified agent list (installed plus runtime). */
export interface AgentListItem {
  /** Runtime UUID (`null` when installed but not loaded). */
  id: string | null;
  /** Unique agent name. */
  name: string;
  /** Version semver. */
  version: string;
  /** Enabled for auto-start at boot. */
  enabled: boolean;
  /** Runtime state (`null` when not loaded). */
  runtime_status:
    | "active"
    | "degraded"
    | "stopped"
    | "initializing"
    | "stopping"
    | null;
  /** RFC 3339 install timestamp (`null` for session-only agents). */
  installed_at: string | null;
  /** Human description of the agent (from the manifest). */
  description: string | null;
  /** Free tags for routing and discovery. */
  tags: string[];
  /** Tools the agent requires. */
  tools_required: string[];
  /** Optional tools of the agent. */
  tools_optional: string[];
  /** ORIA execution mode. */
  execution_mode: "direct" | "orchestrated" | "auto" | null;
  /** Weighted score for ranking and dispatch (from the Observer weighted scoring). */
  weighted_score?: number;
  /** Install path on disk (`null` for runtime-only agents). */
  install_path: string | null;
  /** Whether the agent supports inter-agent communication (A2A). */
  supports_a2a: boolean;
  /** Declared A2A skills (empty when supports_a2a is false). */
  skills: AgentSkillView[];
  /** Semantic role of the agent, for the UI categorisation. */
  agent_type: "worker" | "assistant" | "system" | null;
  /** Prompt examples showing the typical uses (empty = not filled in). */
  examples: string[];
  /** Explicit limitations: what the agent does not do (empty = not filled in). */
  limitations: string[];
  /** Configuration note required before first use (`null` = no prerequisite). */
  setup_notes: string | null;
  /** Name of the Python class the agent comes from. */
  agent_class:
    | "ReActAgent"
    | "ConversationalAgent"
    | "OrchestratedAgent"
    | "WorkerAgent"
    | (string & {})
    | null;
  /**
   * Primary memory namespace declared in the Python manifest. `null` when the
   * agent has no persistent memory.
   *
   * Several agents of one package often share the same namespace (every agent
   * of the `veille-ia` package declares `memory_namespace = "veille-ia"`, for
   * instance), so this value is never derived from `name`: it always comes
   * from the manifest.
   */
  memory_namespace: string | null;
  /** Shared memory namespaces the agent can read. */
  shared_memory_namespaces: string[];
}

/** A2A skill declared by a worker agent. */
export interface AgentSkillView {
  id: string;
  name: string;
  description: string;
}

/** Response of an agent install or update. */
export interface InstallAgentResponse {
  name: string;
  version: string;
  install_path: string;
}

/** Result of creating an agent from an SDK template. */
export interface CreateAgentResult {
  /** Name of the created agent. */
  name: string;
  /** Template type used. */
  template_type: string;
  /** Path of the directory created on disk. */
  path: string;
}

/** Type of SDK agent template. */
export type AgentTemplateType = "react" | "conversational" | "orchestrated";

/** Definition of an agent template, for the creation dialog. */
export interface TemplateDefinition {
  /** Identifier of the template type. */
  type: AgentTemplateType;
  /** Displayed title. */
  title: string;
  /** Description courte. */
  description: string;
  /** Lucide icon name. */
  icon: string;
  /** CSS colour or gradient for the border. */
  color: string;
}
