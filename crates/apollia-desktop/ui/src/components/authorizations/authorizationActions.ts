/**
 * What the authorizations screen does, as sequences of shell commands: a
 * withdrawal and its undo, saving a fine rule, and "revoke all". Each takes
 * the commands as a parameter so a test can run it without a window. Every
 * withdrawal and every rule written is recorded in the journal by the
 * runtime; nothing here writes it.
 */
import {
  addAgentPermissionRule,
  addPermissionPrefixRule,
  governanceRevokeAllRules,
  governanceRevokePermissionRule,
  restoreFamilyConsent,
  revokeFamilyConsent,
  type AddAgentPermissionRuleInput,
  type AddPermissionPrefixRuleInput,
  type FamilyConsent,
  type PermissionRuleDto,
  type PermissionRuleScope,
} from "$lib/ipc/permissions";
import {
  draftProblem,
  sameAsRule,
  type RevokeAllPlan,
  type RuleDialogMode,
  type RuleDraft,
} from "./authorizations";

/** The shell commands the screen writes with. */
export interface AuthorizationsIpc {
  revokeFamilyConsent: (agentId: string, family: string) => Promise<void>;
  restoreFamilyConsent: (agentId: string, family: string) => Promise<void>;
  governanceRevokePermissionRule: (ruleId: number) => Promise<void>;
  governanceRevokeAllRules: (scope: PermissionRuleScope) => Promise<number>;
  addPermissionPrefixRule: (input: AddPermissionPrefixRuleInput) => Promise<void>;
  addAgentPermissionRule: (input: AddAgentPermissionRuleInput) => Promise<number>;
}

/** The commands of the real shell. */
export const shellIpc: AuthorizationsIpc = {
  revokeFamilyConsent,
  restoreFamilyConsent,
  governanceRevokePermissionRule,
  governanceRevokeAllRules,
  addPermissionPrefixRule,
  addAgentPermissionRule,
};

/** Withdraws a standing consent, in one call and without confirmation. */
export async function withdrawAlways(ipc: AuthorizationsIpc, consent: FamilyConsent): Promise<void> {
  await ipc.revokeFamilyConsent(consent.agent_id, consent.family);
}

/** Gives back a standing consent just withdrawn. */
export async function undoAlways(ipc: AuthorizationsIpc, consent: FamilyConsent): Promise<void> {
  await ipc.restoreFamilyConsent(consent.agent_id, consent.family);
}

/** Withdraws a fine rule, in one call and without confirmation. */
export async function withdrawRule(ipc: AuthorizationsIpc, rule: PermissionRuleDto): Promise<void> {
  await ipc.governanceRevokePermissionRule(rule.id);
}

async function writeRule(
  ipc: AuthorizationsIpc,
  rule: { toolName: string; argPrefix: string | null; action: "allow" | "deny" },
  agentId: string | null,
): Promise<void> {
  if (agentId === null) {
    await ipc.addPermissionPrefixRule(rule);
  } else {
    await ipc.addAgentPermissionRule({ ...rule, agentId });
  }
}

/** Writes again a fine rule just withdrawn, for the same agent or for all. */
export async function undoRule(ipc: AuthorizationsIpc, rule: PermissionRuleDto): Promise<void> {
  await writeRule(
    ipc,
    {
      toolName: rule.tool_name,
      argPrefix: rule.arg_prefix,
      action: rule.action === "allow" ? "allow" : "deny",
    },
    rule.scope === "agent" ? rule.agent_id : null,
  );
}

/** What saving a draft did. */
export type SaveOutcome = "created" | "replaced" | "unchanged";

/**
 * Saves the draft of a fine rule. A new or copied rule is written; a changed
 * one is written first and the rule it replaces removed after, so a failure
 * never leaves the agent with neither. A change that changes nothing writes
 * nothing.
 *
 * Throws when the draft cannot be saved; the dialog checks it first.
 */
export async function saveRule(
  ipc: AuthorizationsIpc,
  mode: RuleDialogMode,
  draft: RuleDraft,
  original: PermissionRuleDto | null,
): Promise<SaveOutcome> {
  const problem = draftProblem(draft);
  if (problem !== null) throw new Error(problem);
  if (mode === "edit" && original !== null && sameAsRule(draft, original)) return "unchanged";
  const prefix = draft.argPrefix.trim();
  await writeRule(
    ipc,
    { toolName: draft.tool, argPrefix: prefix.length > 0 ? prefix : null, action: draft.action },
    draft.scope === "agent" ? draft.agentId : null,
  );
  if (mode === "edit" && original !== null) {
    await ipc.governanceRevokePermissionRule(original.id);
    return "replaced";
  }
  return "created";
}

/** How much a "revoke all" withdrew. */
export interface RevokeAllResult {
  rules: number;
  always: number;
}

/** Runs a confirmed "revoke all" and counts what it withdrew. */
export async function runRevokeAll(ipc: AuthorizationsIpc, plan: RevokeAllPlan): Promise<RevokeAllResult> {
  let rules = 0;
  for (const scope of plan.bulkScopes) {
    rules += await ipc.governanceRevokeAllRules(scope);
  }
  for (const id of plan.ruleIds) {
    await ipc.governanceRevokePermissionRule(id);
    rules += 1;
  }
  let always = 0;
  for (const consent of plan.consents) {
    await ipc.revokeFamilyConsent(consent.agent_id, consent.family);
    always += 1;
  }
  return { rules, always };
}
