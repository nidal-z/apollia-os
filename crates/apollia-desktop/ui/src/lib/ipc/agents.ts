/**
 * Typed Tauri command wrappers for the Agents surface.
 *
 * Every `invoke()` the Agents route needs lives here, so `.svelte` files never
 * call the IPC layer directly (see `crates/apollia-desktop/ui/AGENTS.md` §3).
 * Wrapper names are `camelCase`; the underlying Rust commands are `snake_case`.
 */
import { invoke } from "@tauri-apps/api/core";
import type {
  A2ASkillListing,
  AgentMessage,
  ChatSessionSummary,
  CreateSessionRequest,
  MemoryEntry,
} from "$lib/types";

/** Start an installed agent from its on-disk entry path. */
export async function startAgent(path: string): Promise<void> {
  return invoke<void>("start_agent", { path });
}

/** Stop a running agent by its runtime id. */
export async function stopAgent(agentId: string): Promise<void> {
  return invoke<void>("stop_agent", { agentId });
}

/** Enable an agent for auto-start at boot. */
export async function enableAgent(name: string): Promise<void> {
  return invoke<void>("enable_agent", { name });
}

/** Disable an agent's auto-start at boot. */
export async function disableAgent(name: string): Promise<void> {
  return invoke<void>("disable_agent", { name });
}

/** Install a standalone agent from a `.py` entry path. */
export async function installAgent(path: string): Promise<void> {
  return invoke<void>("install_agent", { path });
}

/**
 * What became of the running instance while the module was replaced.
 *
 * `not_running`  nothing was live, the new module loads at the next start.
 * `restarted`    stopped and started again, the new module is serving now.
 * `stop_failed`  the stop was refused, the previous module is still serving.
 * `start_failed` stopped, but it did not come back up, nothing is serving.
 */
export type AgentRestartOutcome =
  | "not_running"
  | "restarted"
  | "stop_failed"
  | "start_failed";

/** Answer of `update_agent`, richer than an install because of the restart. */
export interface UpdateAgentResult {
  /** Unique name of the updated agent. */
  name: string;
  /** Semver version read from the new module's manifest. */
  version: string;
  /** Install path on disk. */
  install_path: string;
  /** What became of the running instance. */
  restart_outcome: AgentRestartOutcome;
  /** Raw cause of a failed stop or start, `null` otherwise. */
  restart_error: string | null;
}

/**
 * Replace the Python module of an already installed agent.
 *
 * The new file is validated by the loader before anything is written, so a
 * module the runtime refuses leaves the installed agent untouched. The install
 * directory, the auto-start flag and `installed_at` are preserved; the version
 * comes from the new module's manifest, which is why the answer carries it.
 *
 * A running agent is stopped and started again, because the interpreter keeps
 * the module it imported at start time: without the cycle the file on disk
 * changes and the previous code keeps answering. `restart_outcome` says which
 * version is serving now, so the caller never announces a deployment the
 * runtime did not perform.
 */
export async function updateAgent(
  name: string,
  path: string,
): Promise<UpdateAgentResult> {
  return invoke<UpdateAgentResult>("update_agent", { name, path });
}

/** Remove an installed agent: database row, install directory, runtime entry. */
export async function uninstallAgent(name: string): Promise<void> {
  return invoke<void>("uninstall_agent", { name });
}

/**
 * Delete every memory entry in a namespace, returning how many were removed.
 *
 * Used by the uninstall flow when the operator asks for the agent's data to go
 * with it. A namespace that was never created answers `0` rather than failing.
 */
export async function clearMemory(namespace: string): Promise<number> {
  return invoke<number>("clear_memory", { namespace, memoryType: null });
}

/** List memory entries for a declared namespace. */
export async function listMemoryEntries(
  namespace: string,
): Promise<MemoryEntry[]> {
  return invoke<MemoryEntry[]>("list_memory_entries", { namespace });
}

/** Create a chat session (used to start a conversation with an agent). */
export async function createChatSession(
  request: CreateSessionRequest,
): Promise<ChatSessionSummary> {
  return invoke<ChatSessionSummary>("create_chat_session", { request });
}

/** Recent A2A mailbox messages of one agent, newest first. */
export async function listAgentMessages(
  agentName: string,
  limit: number,
): Promise<AgentMessage[]> {
  return invoke<AgentMessage[]>("list_agent_messages", { agentName, limit });
}

/** Every A2A skill the installed agents advertise. */
export async function listA2aSkills(): Promise<A2ASkillListing[]> {
  return invoke<A2ASkillListing[]>("list_a2a_skills");
}
