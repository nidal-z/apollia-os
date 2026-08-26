// ─── Agent packages ───

// ─── Agent Packages ─────────────────────────────────────────────

/** Summary of one agent inside a package. */
export interface PackageAgentSummary {
  name: string;
  role: "director" | "worker" | "assistant" | (string & {});
  entry: string;
}

/** Item of the installed package list. */
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

/** Full detail of one package. */
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

/** Preview of one trigger (dry run, no strict validation). */
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

/** Configuration override for one trigger (a webhook secret, for instance). */
export interface TriggerConfigOverride {
  id: string;
  secret?: string;
}

/** Result of a preview (dry run). */
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

/** Result of a package install. */
export interface InstallPackageResponse {
  name: string;
  version: string;
  agent_count: number;
  trigger_count: number;
  trigger_errors: string[];
}
