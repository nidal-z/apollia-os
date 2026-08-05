/**
 * Typed Tauri command wrappers for the Agents surface.
 *
 * Every `invoke()` the Agents route needs lives here, so `.svelte` files never
 * call the IPC layer directly (see `crates/apollia-desktop/ui/AGENTS.md` §3).
 * Wrapper names are `camelCase`; the underlying Rust commands are `snake_case`.
 */
import { invoke } from "@tauri-apps/api/core";
import type {
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
export async function clearAgentMemory(namespace: string): Promise<number> {
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
