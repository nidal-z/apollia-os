/**
 * The live state of the authorizations screen: the fine rules, the standing
 * consents, the organisation policy and the accesses of each agent, with the
 * withdrawals made since the screen loaded.
 *
 * A withdrawal takes effect at once. What was withdrawn stays in place, marked,
 * with its undo, until the screen is loaded again or left.
 */
import { getAgentDefinition } from "$lib/ipc/agentSheet";
import { listConnections } from "$lib/ipc/connections";
import {
  governanceListPermissionRules,
  governanceListTools,
  listFamilyConsents,
  type FamilyConsent,
  type PermissionRuleDto,
} from "$lib/ipc/permissions";
import { getOrgPolicy, type PolicyView } from "$lib/ipc/policy";
import { describeTool, toolKindTag } from "$lib/ipc/tools";
import type { AgentListItem, Connection } from "$lib/types";
import { connectionToolFamily } from "../agents/sheet/connectionGrant";
import {
  accessView,
  groupAccesses,
  type AccessDescriptor,
  type AccessGroup,
  type AccessView,
} from "../agents/sheet/agentSheet";
import {
  runRevokeAll,
  saveRule,
  shellIpc,
  undoAlways,
  undoRule,
  withdrawAlways,
  withdrawRule,
  type RevokeAllResult,
  type SaveOutcome,
} from "./authorizationActions";
import {
  consentKey,
  isFamilyKey,
  policySummary,
  type FamilyKey,
  type PolicySummary,
  type RevokeAllPlan,
  type RuleDialogMode,
  type RuleDraft,
  type Withdrawn,
} from "./authorizations";

/** A tool a fine rule can target, with its family once known. */
export interface RuleTool {
  name: string;
  family: FamilyKey | "";
  description: string;
}

export interface Authorizations {
  readonly loading: boolean;
  /** Why the screen could not be read, `null` when it was. */
  readonly error: string | null;
  readonly rules: PermissionRuleDto[];
  readonly consents: FamilyConsent[];
  readonly policy: PolicySummary;
  readonly withdrawn: Withdrawn;
  /** Key of the line a command is running for, `null` when none is. */
  readonly busy: string | null;
  /** The tools a fine rule can target. */
  readonly tools: RuleTool[];
  /** The accesses of an agent grouped by family, empty until read. */
  groupsOf(agentId: string): AccessGroup[];
  /** What a tool does in plain words, its name when nothing says. */
  describe(toolName: string): string;
  /** The family of a tool, empty when nobody classified it. */
  familyOf(toolName: string): FamilyKey | "";
  load(agents: readonly AgentListItem[], extraIds: readonly string[]): Promise<void>;
  loadTools(): Promise<void>;
  withdrawAlways(consent: FamilyConsent): Promise<void>;
  undoAlways(consent: FamilyConsent): Promise<void>;
  withdrawRule(rule: PermissionRuleDto): Promise<void>;
  undoRule(rule: PermissionRuleDto): Promise<void>;
  saveRule(mode: RuleDialogMode, draft: RuleDraft, original: PermissionRuleDto | null): Promise<SaveOutcome>;
  revokeAll(plan: RevokeAllPlan): Promise<RevokeAllResult>;
}

function message(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

/** Creates the state of one authorizations screen. */
export function createAuthorizations(): Authorizations {
  let loading = $state(true);
  let error = $state<string | null>(null);
  let rules = $state<PermissionRuleDto[]>([]);
  let consents = $state<FamilyConsent[]>([]);
  let policyView = $state<PolicyView | null>(null);
  let withdrawnConsents = $state<string[]>([]);
  let withdrawnRules = $state<number[]>([]);
  let busy = $state<string | null>(null);
  let groups = $state<Record<string, AccessGroup[]>>({});
  let descriptors = $state<Record<string, AccessDescriptor | null>>({});
  let tools = $state<RuleTool[]>([]);
  let connections: Connection[] = [];

  async function descriptorOf(id: string): Promise<AccessDescriptor | null> {
    if (id in descriptors) return descriptors[id] ?? null;
    const read = await describeTool(id).catch(() => null);
    const descriptor: AccessDescriptor | null = read
      ? {
          description: read.description,
          family: read.family,
          family_source: read.family_source,
          kind: toolKindTag(read.kind),
        }
      : null;
    descriptors = { ...descriptors, [id]: descriptor };
    return descriptor;
  }

  async function viewOf(id: string): Promise<AccessView> {
    if (id.startsWith("agent:") || id.startsWith("workflow:")) return accessView(id, null);
    if (id.startsWith("mcp:")) {
      const family = connectionToolFamily(id, connections);
      return accessView(
        id,
        family ? { description: "", family, family_source: "server", kind: "mcp" } : null,
      );
    }
    return accessView(id, await descriptorOf(id));
  }

  async function groupsFor(id: string): Promise<AccessGroup[]> {
    const definition = await getAgentDefinition(id).catch(() => null);
    const ids = definition ? definition.access : [];
    return groupAccesses(await Promise.all(ids.map((accessId) => viewOf(accessId))));
  }

  async function readStores(): Promise<void> {
    const [listedRules, listedConsents] = await Promise.all([
      governanceListPermissionRules(),
      listFamilyConsents(),
    ]);
    rules = listedRules;
    consents = listedConsents;
    withdrawnConsents = [];
    withdrawnRules = [];
  }

  async function load(agents: readonly AgentListItem[], extraIds: readonly string[]): Promise<void> {
    loading = true;
    error = null;
    try {
      const [, view, connected] = await Promise.all([
        readStores(),
        getOrgPolicy().catch(() => null),
        listConnections().catch(() => [] as Connection[]),
      ]);
      policyView = view;
      connections = connected;
      const targets: string[] = [...extraIds, ...agents.map((agent) => agent.name)];
      const read = await Promise.all(
        targets.map(async (id) => [id, await groupsFor(id)] as const),
      );
      groups = Object.fromEntries(read);
    } catch (err) {
      error = message(err);
    } finally {
      loading = false;
    }
  }

  async function loadTools(): Promise<void> {
    const listed = await governanceListTools().catch(() => []);
    const names = listed.map((tool) => tool.name).sort();
    const read = await Promise.all(
      names.map(async (name): Promise<RuleTool> => {
        const descriptor = await descriptorOf(name);
        const family = descriptor?.family ?? "";
        return {
          name,
          family: isFamilyKey(family) ? family : "",
          description: descriptor?.description.trim() ?? "",
        };
      }),
    );
    tools = read;
  }

  async function during<T>(key: string, run: () => Promise<T>): Promise<T> {
    busy = key;
    try {
      return await run();
    } finally {
      busy = null;
    }
  }

  return {
    get loading() {
      return loading;
    },
    get error() {
      return error;
    },
    get rules() {
      return rules;
    },
    get consents() {
      return consents;
    },
    get policy() {
      return policySummary(policyView);
    },
    get withdrawn() {
      return { consents: new Set(withdrawnConsents), rules: new Set(withdrawnRules) };
    },
    get busy() {
      return busy;
    },
    get tools() {
      return tools;
    },
    groupsOf(agentId) {
      return groups[agentId] ?? [];
    },
    describe(toolName) {
      const known = descriptors[toolName]?.description.trim() ?? "";
      return known.length > 0 ? known : toolName;
    },
    familyOf(toolName) {
      const family = descriptors[toolName]?.family ?? "";
      return isFamilyKey(family) ? family : "";
    },
    load,
    loadTools,
    async withdrawAlways(consent) {
      const key = consentKey(consent);
      await during(`always:${key}`, async () => {
        await withdrawAlways(shellIpc, consent);
        withdrawnConsents = [...withdrawnConsents, key];
      });
    },
    async undoAlways(consent) {
      const key = consentKey(consent);
      await during(`always:${key}`, async () => {
        await undoAlways(shellIpc, consent);
        withdrawnConsents = withdrawnConsents.filter((k) => k !== key);
      });
    },
    async withdrawRule(rule) {
      await during(`rule:${rule.id}`, async () => {
        await withdrawRule(shellIpc, rule);
        withdrawnRules = [...withdrawnRules, rule.id];
      });
    },
    async undoRule(rule) {
      await during(`rule:${rule.id}`, async () => {
        await undoRule(shellIpc, rule);
        const kept = withdrawnRules.filter((id) => id !== rule.id);
        const fresh = await governanceListPermissionRules();
        const known = new Set(rules.map((r) => r.id));
        const written = fresh.find((r) => !known.has(r.id));
        rules = written ? rules.map((r) => (r.id === rule.id ? written : r)) : fresh;
        withdrawnRules = kept;
      });
    },
    async saveRule(mode, draft, original) {
      return during("rule:save", async () => {
        const outcome = await saveRule(shellIpc, mode, draft, original);
        if (outcome !== "unchanged") {
          const kept = withdrawnRules;
          const marked = rules.filter((r) => kept.includes(r.id));
          const fresh = await governanceListPermissionRules();
          rules = [...fresh, ...marked.filter((m) => !fresh.some((r) => r.id === m.id))];
        }
        return outcome;
      });
    },
    async revokeAll(plan) {
      return during("revoke-all", async () => {
        try {
          return await runRevokeAll(shellIpc, plan);
        } finally {
          await readStores().catch((err: unknown) => {
            error = message(err);
          });
        }
      });
    },
  };
}
