// ─── Agent packages ───

// ─── Agent Packages ─────────────────────────────────────────────

/** Summary of one agent inside a package. */
export interface PackageAgentSummary {
  name: string;
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
  /** Its publisher and the state of its signature. */
  signature: PackageSignatureView;
  /** Apollia ships it inside the application: it is never uninstalled. */
  embedded: boolean;
  /** How many agents, workflows, tools and connections it holds. */
  counts: PackageCounts;
}

/** Who published an installed package, as recorded at its installation. */
export interface PackageSignatureView {
  /** The keyring label when the key is trusted, else the declared name. */
  publisher: string;
  signed: boolean;
  trusted: boolean;
  key_fingerprint: string | null;
}

/** How many items of each kind a package installed. */
export interface PackageCounts {
  agents: number;
  workflows: number;
  tools: number;
  /** Connections, brought and required together. */
  connections: number;
}

/** An agent or a workflow of an installed package. */
export interface PackageContentItem {
  id: string;
  name: string;
  description: string;
}

/** A tool of an installed package, with its action family. */
export interface PackageContentTool {
  name: string;
  description: string;
  /** `read`, `read_personal`, `write_or_delete`, `send_external`, `execute_code`, or empty. */
  family: string;
}

/** A connection a package requires: it stays when the package goes. */
export interface PackageRequiredConnection {
  type: string;
  reason: string;
}

/** What an installed package holds, by kind. */
export interface PackageContentView {
  agents: PackageContentItem[];
  workflows: PackageContentItem[];
  tools: PackageContentTool[];
  /** The connections it brought: they go with it. */
  connections_brought: string[];
  connections_required: PackageRequiredConnection[];
}

/** An automation that starts an agent or a workflow of a package. */
export interface PackageAutomation {
  id: string;
  target: string;
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
  signature: PackageSignatureView;
  embedded: boolean;
  content: PackageContentView;
  /** The automations that go with the package when it is uninstalled. */
  automations: PackageAutomation[];
}
