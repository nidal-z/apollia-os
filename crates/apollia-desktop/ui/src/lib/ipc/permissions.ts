/**
 * Typed Tauri IPC wrappers for the permission-governance domain.
 *
 * One thin function per `#[tauri::command]` that the Settings > Permissions
 * page calls directly (the list / revoke / audit commands already live in
 * `$lib/stores/permissions`). Components import these instead of `invoke()`,
 * so command names and payload shapes live in a single place.
 */
import { invoke } from "@tauri-apps/api/core";

/** Governance tool as returned by `governance_list_tools`. */
export interface GovernanceToolStatus {
  name: string;
}

/**
 * Tools that execute arbitrary code. The runtime never grants them a blanket
 * "allow" rule: an allow rule for one of these MUST carry an argument prefix,
 * otherwise only per-invocation approval (HITL) applies. Mirrors the backend
 * `CODE_EXECUTOR_TOOLS` invariant enforced by `add_permission_prefix_rule`.
 */
export const CODE_EXECUTOR_TOOLS = ["bash", "python"] as const;

/** Whether a tool name is a code executor subject to the no-blanket-allow rule. */
export function isCodeExecutor(toolName: string): boolean {
  return (CODE_EXECUTOR_TOOLS as readonly string[]).includes(toolName);
}

/** Fetch the list of governance-managed tools available for a new rule. */
export async function listGovernanceTools(): Promise<GovernanceToolStatus[]> {
  return invoke<GovernanceToolStatus[]>("governance_list_tools");
}

/** Payload for a new prefix rule. Only `global` and `project` scopes create rules. */
export interface AddPermissionPrefixRuleInput {
  toolName: string;
  argPrefix: string | null;
  action: "allow" | "deny";
  scope: "global" | "project";
  projectPath: string | null;
}

/** Create a persistent permission rule. Session / agent rules are born of HITL. */
export async function addPermissionPrefixRule(
  input: AddPermissionPrefixRuleInput,
): Promise<void> {
  await invoke("add_permission_prefix_rule", {
    toolName: input.toolName,
    argPrefix: input.argPrefix,
    action: input.action,
    scope: input.scope,
    projectPath: input.projectPath,
  });
}
