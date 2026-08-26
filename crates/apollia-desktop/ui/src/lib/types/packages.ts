// ─── Agent packages ───

// ─── Agent Packages ─────────────────────────────────────────────

/** Résumé d'un agent dans un package. */
export interface PackageAgentSummary {
  name: string;
  role: "director" | "worker" | "assistant" | (string & {});
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
  source_type: "cron" | "interval" | "webhook" | "file_watch" | "oneshot" | (string & {});
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
