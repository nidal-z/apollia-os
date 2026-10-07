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
