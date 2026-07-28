// Typed IPC wrappers for the delegation tree surface.
//
// `listDelegationTasks` enumerates recent A2A tasks; `getDelegationTree` reads
// the parent/child delegation graph for one task. Keeping the calls here removes
// direct `invoke` usage from the `.svelte` file.

import { invoke } from "@tauri-apps/api/core";

/** One node of the delegation graph, mirroring the Rust `DelegationNode`. */
export interface DelegationNode {
  agent_id: string;
  agent_name: string;
  status: string;
  started_at: string | null;
  children: DelegationNode[];
}

/** A selectable task in the delegation picker. */
export interface DelegationTaskOption {
  id: string;
  agent_name: string;
  status: string;
}

/** Lists recent A2A tasks eligible for delegation inspection. */
export async function listDelegationTasks(): Promise<DelegationTaskOption[]> {
  return invoke<DelegationTaskOption[]>("list_tasks", { filter: null });
}

/** Reads the delegation tree rooted at a given task. */
export async function getDelegationTree(taskId: string): Promise<DelegationNode> {
  return invoke<DelegationNode>("get_delegation_tree", { taskId });
}
