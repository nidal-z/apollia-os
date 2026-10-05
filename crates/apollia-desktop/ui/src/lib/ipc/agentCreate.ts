/**
 * Typed Tauri IPC wrapper behind the creation of an agent without code.
 *
 * Thin wrapper over the command of
 * `crates/apollia-desktop/src/commands/agent_create.rs`, which sends the
 * sheet the person validated to the runtime. The runtime stores the agent,
 * of origin `interface`, and records its creation.
 */
import { invoke } from "@tauri-apps/api/core";
import type { ResolvedAgent } from "$lib/ipc/agentSheet";

/** The sheet of an agent, as the person validated it in the form. */
export interface AgentSheetDraft {
  name: string;
  mission: string;
  examples: string[];
  /** The accesses checked in the form. */
  access: string[];
  /** The sources checked in the form. */
  sources: string[];
}

/** Creates the agent `sheet` describes, and answers the agent in force. */
export async function createAgentDefinition(sheet: AgentSheetDraft): Promise<ResolvedAgent> {
  return invoke<ResolvedAgent>("create_agent_definition", { sheet });
}
