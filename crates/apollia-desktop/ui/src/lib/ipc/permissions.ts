/**
 * Typed Tauri IPC wrappers behind the authorizations screen and the sheet of
 * an agent: the fine rules, and the standing consents ("always" for one agent
 * and one action family).
 *
 * One thin function per `#[tauri::command]`. Components import these instead
 * of `invoke()`, so command names and payload shapes live in a single place.
 */
import { invoke } from "@tauri-apps/api/core";

/** Governance tool as returned by `governance_list_tools`. */
export interface GovernanceToolStatus {
  name: string;
}

/**
 * Tools that execute arbitrary code. The runtime never honors a blanket
 * "allow" rule for them: an allow needs a single simple command as its
 * prefix. Mirrors `CODE_EXECUTOR_TOOLS` in
 * `crates/apollia-permissions/src/executor_guard.rs`.
 */
const CODE_EXECUTOR_TOOLS = ["bash_executor", "python_executor"] as const;

/** Whether a tool name is a code executor subject to the no-blanket-allow rule. */
export function isCodeExecutor(toolName: string): boolean {
  return (CODE_EXECUTOR_TOOLS as readonly string[]).includes(toolName);
}

/** Fetch the list of governance-managed tools available for a new rule. */
export async function governanceListTools(): Promise<GovernanceToolStatus[]> {
  return invoke<GovernanceToolStatus[]>("governance_list_tools");
}

/** Whom a fine rule targets: one agent, or every agent. */
export type PermissionRuleScope = "agent" | "global";

/** A fine rule, as `governance_list_permission_rules` returns it. */
export interface PermissionRuleDto {
  id: number;
  tool_name: string;
  arg_prefix: string | null;
  action: string;
  scope: PermissionRuleScope | (string & {});
  project_path: string | null;
  agent_id: string | null;
  expires_at: string | null;
  created_at: string;
  created_by: string | null;
}

/** Every fine rule, the rules of one agent and the rules for every agent. */
export async function governanceListPermissionRules(): Promise<PermissionRuleDto[]> {
  return invoke<PermissionRuleDto[]>("governance_list_permission_rules", {
    filter: { scope: null, tool_name: null },
  });
}

/** Removes one fine rule. The removal is recorded in the journal. */
export async function governanceRevokePermissionRule(ruleId: number): Promise<void> {
  await invoke("governance_revoke_permission_rule", { ruleId });
}

/**
 * Removes every rule of one scope: `agent` for the rules that target a single
 * agent, `global` for the rules for every agent. Returns how many were removed.
 */
export async function governanceRevokeAllRules(scope: PermissionRuleScope): Promise<number> {
  return invoke<number>("governance_revoke_all_rules", { scope });
}

/** Payload of a fine rule for every agent. */
export interface AddPermissionPrefixRuleInput {
  toolName: string;
  argPrefix: string | null;
  action: "allow" | "deny";
}

/** Creates a fine rule for every agent. */
export async function addPermissionPrefixRule(
  input: AddPermissionPrefixRuleInput,
): Promise<void> {
  await invoke("add_permission_prefix_rule", {
    toolName: input.toolName,
    argPrefix: input.argPrefix,
    action: input.action,
    scope: "global",
    projectPath: null,
  });
}

/** Payload of a fine rule that targets one agent. */
export interface AddAgentPermissionRuleInput extends AddPermissionPrefixRuleInput {
  agentId: string;
}

/** Creates a fine rule that targets one agent. Returns its identifier. */
export async function addAgentPermissionRule(
  input: AddAgentPermissionRuleInput,
): Promise<number> {
  return invoke<number>("add_agent_permission_rule", {
    agentId: input.agentId,
    toolName: input.toolName,
    argPrefix: input.argPrefix,
    action: input.action,
  });
}

/** A family a standing consent can cover. */
export type ConsentFamily = "read" | "read_personal" | "write_or_delete" | "send_external";

/** One standing consent: "always" for one agent and one action family. */
export interface FamilyConsent {
  agent_id: string;
  family: ConsentFamily | (string & {});
  /** Unix epoch, seconds. */
  granted_at: number;
  /** Who gave it: `user`, or `upgrade` for one carried over from an earlier version. */
  granted_by: string;
}

/** Every standing consent, whatever the agent. */
export async function listFamilyConsents(): Promise<FamilyConsent[]> {
  return invoke<FamilyConsent[]>("list_family_consents");
}

/** Withdraws a standing consent. The withdrawal is recorded in the journal. */
export async function revokeFamilyConsent(agentId: string, family: string): Promise<void> {
  await invoke("revoke_family_consent", { agentId, family });
}

/** Gives back a standing consent just withdrawn. Recorded in the journal. */
export async function restoreFamilyConsent(agentId: string, family: string): Promise<void> {
  await invoke("restore_family_consent", { agentId, family });
}
