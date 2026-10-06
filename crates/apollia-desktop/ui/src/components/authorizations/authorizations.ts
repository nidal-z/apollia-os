/**
 * What the authorizations screen says, computed apart from its components:
 * the families an agent holds and what each asks, its standing consents
 * ("always"), the fine rules, what the organisation policy imposes, the plan
 * of a "revoke all" and the draft of a fine rule.
 */
import {
  isCodeExecutor,
  type FamilyConsent,
  type PermissionRuleDto,
  type PermissionRuleScope,
} from "$lib/ipc/permissions";
import type { PolicyView } from "$lib/ipc/policy";
import type { AccessGroup, AccessView } from "../agents/sheet/agentSheet";

/** Key of the list row that holds the rules for every agent. */
export const ALL_AGENTS = "*";

/** The five action families of governance, in the order the screen lists them. */
export const FAMILY_ORDER = [
  "read",
  "read_personal",
  "write_or_delete",
  "send_external",
  "execute_code",
] as const;

/** One of the five action families. */
export type FamilyKey = (typeof FAMILY_ORDER)[number];

/** Whether `value` names one of the five families. */
export function isFamilyKey(value: string): value is FamilyKey {
  return (FAMILY_ORDER as readonly string[]).includes(value);
}

/** An agent as the list of the screen shows it. */
export interface AgentEntry {
  /** Identifier governance knows the agent by. */
  id: string;
  name: string;
  description: string;
}

// ── Policy ──────────────────────────────────────────────────────────────

/** What the organisation policy imposes, as the screen reads it. */
export interface PolicySummary {
  state: "none" | "active" | "invalid";
  /** Families no agent may use. */
  deniedFamilies: FamilyKey[];
  /** Connections that cannot be added. */
  deniedConnections: string[];
  /** Settings the organisation fixes. */
  fixedSettings: string[];
  /** Who signed the policy, when it is active. */
  publisher: string | null;
  /** When it was signed, when it is active. */
  signedAt: string | null;
  /** Why it is refused, when it is invalid. */
  reason: string | null;
}

const NO_POLICY: PolicySummary = {
  state: "none",
  deniedFamilies: [],
  deniedConnections: [],
  fixedSettings: [],
  publisher: null,
  signedAt: null,
  reason: null,
};

/** The summary of a policy; a policy not read yet imposes nothing. */
export function policySummary(view: PolicyView | null): PolicySummary {
  if (view === null || view.state === "none") return NO_POLICY;
  if (view.state === "invalid") return { ...NO_POLICY, state: "invalid", reason: view.reason };
  const { policy } = view;
  const deniedConnections = [...policy.connections.deny];
  if (policy.connections.remote === "deny") deniedConnections.push("remote");
  return {
    state: "active",
    deniedFamilies: policy.families.deny.filter(isFamilyKey),
    deniedConnections,
    fixedSettings: Object.keys(policy.settings).sort(),
    publisher: view.publisher,
    signedAt: view.signed_at,
    reason: null,
  };
}

/** Whether the policy imposes anything the screen has to mention. */
export function imposesSomething(summary: PolicySummary): boolean {
  return (
    summary.deniedFamilies.length > 0 ||
    summary.deniedConnections.length > 0 ||
    summary.fixedSettings.length > 0
  );
}

// ── Families ────────────────────────────────────────────────────────────

/** What a family asks before the agent uses it. */
export type FamilyAsk = "imposed" | "without_asking" | "always" | "asks_first" | "asks_each_time";

/** One family an agent holds accesses in. */
export interface FamilyLine {
  family: FamilyKey;
  /** What the accesses of the family let the agent do, in plain words. */
  covers: string[];
  ask: FamilyAsk;
}

function askOf(family: FamilyKey, imposed: boolean, always: boolean): FamilyAsk {
  if (imposed) return "imposed";
  if (family === "read") return "without_asking";
  if (family === "execute_code") return "asks_each_time";
  return always ? "always" : "asks_first";
}

/**
 * The families an agent holds at least one access in, each with what it
 * covers and what it asks. A family the policy forbids is marked imposed and
 * carries no action; one covered by a standing consent says so.
 */
export function familyLines(
  groups: readonly AccessGroup[],
  summary: PolicySummary,
  always: readonly FamilyConsent[],
): FamilyLine[] {
  const lines: FamilyLine[] = [];
  for (const family of FAMILY_ORDER) {
    const group = groups.find((g) => g.key === family);
    if (!group || group.accesses.length === 0) continue;
    lines.push({
      family,
      covers: group.accesses.map((access: AccessView) => access.permits ?? access.label),
      ask: askOf(
        family,
        summary.deniedFamilies.includes(family),
        always.some((consent) => consent.family === family),
      ),
    });
  }
  return lines;
}

// ── Standing consents and rules ─────────────────────────────────────────

/** Stable key of a standing consent. */
export function consentKey(consent: Pick<FamilyConsent, "agent_id" | "family">): string {
  return `${consent.agent_id}::${consent.family}`;
}

/** The standing consents of one agent, oldest first. */
export function alwaysOf(consents: readonly FamilyConsent[], agentId: string): FamilyConsent[] {
  return consents.filter((consent) => consent.agent_id === agentId);
}

/**
 * The fine rules a list row shows: those for every agent under
 * {@link ALL_AGENTS}, those that target the agent otherwise.
 */
export function rulesOf(rules: readonly PermissionRuleDto[], selection: string): PermissionRuleDto[] {
  if (selection === ALL_AGENTS) return rules.filter((rule) => rule.scope === "global");
  return rules.filter((rule) => rule.scope === "agent" && rule.agent_id === selection);
}

/** What the list says under an agent's name. */
export interface AgentSummary {
  families: number;
  always: number;
  rules: number;
}

/** The counts under an agent's name; what was just withdrawn no longer counts. */
export function agentSummary(
  agentId: string,
  families: number,
  consents: readonly FamilyConsent[],
  rules: readonly PermissionRuleDto[],
  withdrawn: Withdrawn,
): AgentSummary {
  return {
    families,
    always: alwaysOf(consents, agentId).filter((c) => !withdrawn.consents.has(consentKey(c))).length,
    rules: rulesOf(rules, agentId).filter((r) => !withdrawn.rules.has(r.id)).length,
  };
}

/** What was withdrawn since the screen loaded, kept in place with its undo. */
export interface Withdrawn {
  consents: ReadonlySet<string>;
  rules: ReadonlySet<number>;
}

/**
 * Whether an agent matches the filter: by its name, its description, or one
 * of the words its families, standing consents and rules are shown with.
 */
export function matchesFilter(query: string, entry: AgentEntry, words: readonly string[]): boolean {
  const needle = query.trim().toLowerCase();
  if (needle.length === 0) return true;
  return [entry.name, entry.description, ...words].some((word) =>
    word.toLowerCase().includes(needle),
  );
}

/**
 * A wire instant, an RFC 3339 string or Unix seconds, as the machine's local
 * calendar day in the locale's words. A value that is no date is shown as is.
 */
export function formatDay(value: string | number, locale: string): string {
  const date = new Date(typeof value === "number" ? value * 1000 : value);
  if (Number.isNaN(date.getTime())) return String(value);
  return date.toLocaleDateString(locale, { day: "numeric", month: "long", year: "numeric" });
}

// ── Fine rules ──────────────────────────────────────────────────────────

/** What a fine rule line offers. */
export type RuleAction = "edit" | "duplicate" | "withdraw";

/**
 * What can be done to a fine rule. Everyone can withdraw one; changing or
 * copying one is creating a rule, which the developer mode reserves.
 */
export function ruleActions(developerMode: boolean): RuleAction[] {
  return developerMode ? ["edit", "duplicate", "withdraw"] : ["withdraw"];
}

/** Whether a fine rule can be created: in developer mode only. */
export function canCreateRule(developerMode: boolean): boolean {
  return developerMode;
}

/** A fine rule in plain words: a catalogue key and its values. */
export interface RuleSentence {
  key: string;
  values: Record<string, string>;
}

/** What a rule allows or refuses, said without its technical detail. */
export function ruleSentence(rule: PermissionRuleDto, what: string): RuleSentence {
  const prefix = rule.arg_prefix?.trim() ?? "";
  if (isCodeExecutor(rule.tool_name)) {
    return prefix.length > 0
      ? { key: "authorizations.rule_run_command", values: { command: prefix } }
      : { key: "authorizations.rule_run_any_code", values: {} };
  }
  return prefix.length > 0
    ? { key: "authorizations.rule_tool_prefixed", values: { what, prefix } }
    : { key: "authorizations.rule_tool_all", values: { what } };
}

/** Why the dialog of a fine rule is open. */
export type RuleDialogMode = "add" | "duplicate" | "edit";

/** A fine rule being written. */
export interface RuleDraft {
  action: "allow" | "deny";
  /** Family the tool list is narrowed to; empty for every family. */
  family: FamilyKey | "";
  tool: string;
  argPrefix: string;
  scope: "all" | "agent";
  agentId: string;
}

/**
 * The draft a dialog opens on: empty for a new rule, refusing by default;
 * the rule's own values when it is copied or changed.
 */
export function draftFrom(
  rule: PermissionRuleDto | null,
  family: FamilyKey | "",
  fallbackAgent: string,
): RuleDraft {
  if (rule === null) {
    return { action: "deny", family: "", tool: "", argPrefix: "", scope: "all", agentId: fallbackAgent };
  }
  return {
    action: rule.action === "allow" ? "allow" : "deny",
    family,
    tool: rule.tool_name,
    argPrefix: rule.arg_prefix ?? "",
    scope: rule.scope === "agent" ? "agent" : "all",
    agentId: rule.scope === "agent" && rule.agent_id ? rule.agent_id : fallbackAgent,
  };
}

/** Mirrors `is_single_simple_command` of the permissions crate. */
export function isSimpleCommand(command: string): boolean {
  return !/[;|&\n\r`><(){}]/.test(command) && !command.includes("$(") && !command.includes("${");
}

/** Why a draft cannot be saved. */
export type DraftProblem = "tool_missing" | "agent_missing" | "executor_needs_command" | "command_not_simple";

/**
 * What keeps a draft from being saved, `null` when nothing does. Allowing a
 * tool that runs code needs one simple command: never the tool as a whole,
 * never a chained or redirected command.
 */
export function draftProblem(draft: RuleDraft): DraftProblem | null {
  if (draft.tool.trim().length === 0) return "tool_missing";
  if (draft.scope === "agent" && draft.agentId.trim().length === 0) return "agent_missing";
  if (isCodeExecutor(draft.tool)) {
    const command = draft.argPrefix.trim();
    if (draft.action === "allow" && command.length === 0) return "executor_needs_command";
    if (command.length > 0 && !isSimpleCommand(command)) return "command_not_simple";
  }
  return null;
}

/** Whether a draft says exactly what `rule` already says. */
export function sameAsRule(draft: RuleDraft, rule: PermissionRuleDto): boolean {
  const scope = rule.scope === "agent" ? "agent" : "all";
  return (
    draft.action === rule.action &&
    draft.tool === rule.tool_name &&
    draft.argPrefix.trim() === (rule.arg_prefix ?? "").trim() &&
    draft.scope === scope &&
    (scope === "all" || draft.agentId === rule.agent_id)
  );
}

// ── Revoke all ──────────────────────────────────────────────────────────

/** What a "revoke all" withdraws. */
export type RevokeWhat = "rules" | "always" | "both";

/** What a "revoke all" will do, before it is confirmed. */
export interface RevokeAllPlan {
  /** Scopes emptied in one call each, when every agent is targeted. */
  bulkScopes: PermissionRuleScope[];
  /** Rules removed one by one, when a single agent is targeted. */
  ruleIds: number[];
  /** Standing consents withdrawn. */
  consents: FamilyConsent[];
  /** How many fine rules the plan removes. */
  ruleCount: number;
}

/**
 * The plan of a "revoke all": the fine rules, the standing consents or both,
 * for every agent (`agentId` null) or for one. Targeting one agent leaves the
 * rules for every agent in place. The accesses granted by family never move.
 */
export function revokeAllPlan(
  what: RevokeWhat,
  agentId: string | null,
  rules: readonly PermissionRuleDto[],
  consents: readonly FamilyConsent[],
): RevokeAllPlan {
  const withRules = what !== "always";
  const withAlways = what !== "rules";
  const targetedRules = !withRules ? [] : agentId === null ? [...rules] : rulesOf(rules, agentId);
  const scopes = new Set<PermissionRuleScope>();
  if (agentId === null) {
    for (const rule of targetedRules) scopes.add(rule.scope === "agent" ? "agent" : "global");
  }
  return {
    bulkScopes: [...scopes].sort(),
    ruleIds: agentId === null ? [] : targetedRules.map((rule) => rule.id),
    consents: !withAlways ? [] : agentId === null ? [...consents] : alwaysOf(consents, agentId),
    ruleCount: targetedRules.length,
  };
}

/** Whether a plan withdraws nothing, so its confirmation stays disabled. */
export function planIsEmpty(plan: RevokeAllPlan): boolean {
  return plan.ruleCount === 0 && plan.consents.length === 0;
}

// ── Catalogue keys ──────────────────────────────────────────────────────

// Spelled out rather than interpolated, so the catalogue guards see every key
// the screen reads.

/** Name of each family. */
export const FAMILY_LABEL: Record<FamilyKey, string> = {
  read: "agent_sheet.family_read",
  read_personal: "agent_sheet.family_read_personal",
  write_or_delete: "agent_sheet.family_write_or_delete",
  send_external: "agent_sheet.family_send_external",
  execute_code: "agent_sheet.family_execute_code",
};

/** What a family asks, as its badge says it. */
export const FAMILY_ASK_LABEL: Record<FamilyAsk, string> = {
  imposed: "settings.policy_locked",
  without_asking: "authorizations.ask_without_asking",
  always: "authorizations.ask_always",
  asks_first: "authorizations.ask_first",
  asks_each_time: "authorizations.ask_each_time",
};

/** Why a draft cannot be saved, as the dialog says it. */
export const DRAFT_PROBLEM_LABEL: Record<DraftProblem, string> = {
  tool_missing: "dev.authorizations.problem_tool_missing",
  agent_missing: "dev.authorizations.problem_agent_missing",
  executor_needs_command: "dev.authorizations.problem_executor_needs_command",
  command_not_simple: "dev.authorizations.problem_command_not_simple",
};

/** Title of the dialog of a fine rule. */
export const RULE_DIALOG_TITLE: Record<RuleDialogMode, string> = {
  add: "dev.authorizations.dialog_add_title",
  duplicate: "dev.authorizations.dialog_duplicate_title",
  edit: "dev.authorizations.dialog_edit_title",
};
