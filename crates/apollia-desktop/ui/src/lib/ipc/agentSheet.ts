/**
 * Typed Tauri IPC wrappers behind the sheet of an agent.
 *
 * Thin wrappers over the commands of
 * `crates/apollia-desktop/src/commands/agent_sheet.rs`, each of which reads
 * one runtime route: the agent as resolved (package definition, then the
 * user's overrides), the last time its model fell back, and its governance
 * history in the journal.
 */
import { invoke } from "@tauri-apps/api/core";

/** Where the model assigned to an agent runs. */
export type ModelSource = "local" | "server";

/** The model the user assigned to an agent. */
export interface AgentModelAssignment {
  agent: string;
  model_source: ModelSource;
  /** A local registry identifier, or `<server>/<model>`. */
  model_id: string;
  /** Preset of the model; `null` keeps the model's default. */
  preset_id: number | null;
}

/** An agent in force, as `GET /api/v1/agent-definitions/{name}` returns it. */
export interface ResolvedAgent {
  id: string;
  origin: "package" | "interface";
  package: string | null;
  version: string;
  enabled: boolean;
  name: string;
  mission: string;
  examples: string[];
  /** Accesses granted: tool names, `mcp:<server>`, `agent:<id>`, `workflow:<id>`. */
  access: string[];
  /** Whether the agent holds the default access rather than a list chosen for it. */
  access_by_default?: boolean;
  /** The accesses given on top of the default access; empty for an agent that does not hold it. */
  access_added?: string[];
  /** Sources checked, `profile` among them. */
  sources: string[];
  model: AgentModelAssignment | null;
  autonomy: string;
  profile: string | null;
  /** Fields whose value is the user's rather than the package's. */
  overridden: string[];
  /** The definition as the package declares it, before any override. */
  definition?: Record<string, unknown>;
}

/** The last time an agent's model was not available. */
export interface AgentModelFallback {
  agent: string;
  /** Model the agent asked for. */
  requested: string;
  /** Model that served the call instead. */
  used: string;
  /** RFC 3339. */
  at: string;
}

/** One entry of the governance history of an agent, as the journal holds it. */
export interface AgentChangeEntry {
  seq: number;
  run_id: string;
  /** RFC 3339 UTC. */
  ts: string;
  /** `access_changed`, `agent_override_changed`, `agent_installed`, ... */
  kind: string;
  /** The governance record: `event`, `access_id`, `change`, `field`, `actor`, ... */
  payload: Record<string, unknown>;
}

/** The agent `name` resolved by the runtime, `null` when it holds none. */
export async function getAgentDefinition(name: string): Promise<ResolvedAgent | null> {
  return invoke<ResolvedAgent | null>("get_agent_definition", { name });
}

/** The last fallback of the model of `agent`, `null` when it never fell back. */
export async function getAgentModelFallback(
  agent: string,
): Promise<AgentModelFallback | null> {
  return invoke<AgentModelFallback | null>("get_agent_model_fallback", { agent });
}

/** The governance history of `agent`, oldest first. */
export async function listAgentChanges(agent: string): Promise<AgentChangeEntry[]> {
  return invoke<AgentChangeEntry[]>("list_agent_changes", { agent });
}
