/**
 * Typed Tauri IPC wrappers behind the creation of an agent without code.
 *
 * Thin wrappers over the commands of
 * `crates/apollia-desktop/src/commands/agent_create.rs`: what a sheet may
 * hold on this machine, a proposed sheet checked against it, and the sheet
 * the person validated sent to the runtime, which stores the agent, of origin
 * `interface`, and records its creation.
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

/** One access that exists on the machine. */
export interface OfferedAccess {
  id: string;
  /** What it does, in one line. */
  description: string;
  /** Its action family, as the journal names it. */
  family: string;
  sensitive: boolean;
  /** Granted to every agent created without code. */
  by_default: boolean;
}

/** One source that exists on the machine. */
export interface OfferedSource {
  id: string;
  label: string;
}

/** What the draft of an agent may hold on this machine. */
export interface DraftOffer {
  accesses: OfferedAccess[];
  sources: OfferedSource[];
  models: string[];
}

/** A sheet as it is proposed, before the runtime checks it. */
export interface ProposedSheet {
  name: string;
  mission: string;
  examples: string[];
  access: string[];
  sources: string[];
}

/** What the draft of an agent may hold: accesses, sources and models. */
export async function agentDraftOffer(): Promise<DraftOffer> {
  return invoke<DraftOffer>("agent_draft_offer");
}

/**
 * Has `sheet` checked against the machine. The answer is the draft the form
 * opens, read with `draftOfChecked`; nothing is written.
 */
export async function checkAgentDraft(sheet: ProposedSheet): Promise<unknown> {
  return invoke<unknown>("check_agent_draft", { sheet });
}
