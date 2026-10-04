import { describe, expect, test } from "vitest";
import type { FamilyConsent, PermissionRuleDto } from "$lib/ipc/permissions";
import {
  runRevokeAll,
  saveRule,
  undoAlways,
  undoRule,
  withdrawAlways,
  withdrawRule,
  type AuthorizationsIpc,
} from "./authorizationActions";
import { draftFrom, revokeAllPlan } from "./authorizations";

/** A shell that records the commands it receives, in order. */
function fakeShell(failOn?: string): { ipc: AuthorizationsIpc; calls: unknown[][] } {
  const calls: unknown[][] = [];
  const record = <T>(name: string, answer: T) => (...args: unknown[]): Promise<T> => {
    calls.push([name, ...args]);
    return name === failOn ? Promise.reject(new Error("refused")) : Promise.resolve(answer);
  };
  return {
    calls,
    ipc: {
      revokeFamilyConsent: record("revoke_family_consent", undefined),
      restoreFamilyConsent: record("restore_family_consent", undefined),
      governanceRevokePermissionRule: record("governance_revoke_permission_rule", undefined),
      governanceRevokeAllRules: record("governance_revoke_all_rules", 2),
      addPermissionPrefixRule: record("add_permission_prefix_rule", undefined),
      addAgentPermissionRule: record("add_agent_permission_rule", 31),
    },
  };
}

function rule(partial: Partial<PermissionRuleDto>): PermissionRuleDto {
  return {
    id: 14,
    tool_name: "bash_executor",
    arg_prefix: "git push",
    action: "deny",
    scope: "global",
    project_path: null,
    agent_id: null,
    expires_at: null,
    created_at: "2026-09-02T10:00:00Z",
    created_by: null,
    ...partial,
  };
}

const CONSENT: FamilyConsent = {
  agent_id: "writer",
  family: "send_external",
  granted_at: 1_790_000_000,
  granted_by: "user",
};

describe("authorization actions - withdrawal in one click", () => {
  test("withdrawing a standing consent is one command, which the runtime journals", async () => {
    // GIVEN a standing consent
    const { ipc, calls } = fakeShell();

    // WHEN it is withdrawn
    await withdrawAlways(ipc, CONSENT);

    // THEN one command names the agent and the family, with no confirmation step
    expect(calls).toEqual([["revoke_family_consent", "writer", "send_external"]]);
  });

  test("the undo gives the same consent back", async () => {
    // GIVEN a consent just withdrawn
    const { ipc, calls } = fakeShell();

    // WHEN the withdrawal is undone
    await undoAlways(ipc, CONSENT);

    // THEN the same agent and family are restored
    expect(calls).toEqual([["restore_family_consent", "writer", "send_external"]]);
  });

  test("withdrawing a fine rule is one command on its identifier", async () => {
    // GIVEN a fine rule
    const { ipc, calls } = fakeShell();

    // WHEN it is withdrawn
    await withdrawRule(ipc, rule({ id: 14 }));

    // THEN one command removes it
    expect(calls).toEqual([["governance_revoke_permission_rule", 14]]);
  });

  test("the undo of a rule writes it again for the same target", async () => {
    // GIVEN a withdrawn rule for every agent and one for a single agent
    const { ipc, calls } = fakeShell();

    // WHEN each withdrawal is undone
    await undoRule(ipc, rule({}));
    await undoRule(ipc, rule({ scope: "agent", agent_id: "writer", action: "allow", arg_prefix: null, tool_name: "file_read" }));

    // THEN the first is written for all and the second for its agent
    expect(calls).toEqual([
      ["add_permission_prefix_rule", { toolName: "bash_executor", argPrefix: "git push", action: "deny" }],
      ["add_agent_permission_rule", { toolName: "file_read", argPrefix: null, action: "allow", agentId: "writer" }],
    ]);
  });
});

describe("authorization actions - saving a fine rule", () => {
  test("a new rule is written for every agent or for one, as its scope says", async () => {
    // GIVEN a draft for every agent and the same for one agent
    const { ipc, calls } = fakeShell();
    const draft = { ...draftFrom(null, "", "writer"), tool: "bash_executor", argPrefix: " curl " };

    // WHEN both are saved as new rules
    const first = await saveRule(ipc, "add", draft, null);
    const second = await saveRule(ipc, "add", { ...draft, scope: "agent" }, null);

    // THEN each reaches its command, the prefix trimmed
    expect([first, second]).toEqual(["created", "created"]);
    expect(calls).toEqual([
      ["add_permission_prefix_rule", { toolName: "bash_executor", argPrefix: "curl", action: "deny" }],
      ["add_agent_permission_rule", { toolName: "bash_executor", argPrefix: "curl", action: "deny", agentId: "writer" }],
    ]);
  });

  test("a duplicated rule is written beside the one it copies", async () => {
    // GIVEN a rule and the draft copied from it
    const { ipc, calls } = fakeShell();
    const original = rule({});

    // WHEN the copy is saved unchanged
    const outcome = await saveRule(ipc, "duplicate", draftFrom(original, "execute_code", "writer"), original);

    // THEN a rule is written and the original is not removed
    expect(outcome).toBe("created");
    expect(calls.map((call) => call[0])).toEqual(["add_permission_prefix_rule"]);
  });

  test("a changed rule is written first, then the one it replaces is removed", async () => {
    // GIVEN a rule and a draft that changes its prefix
    const { ipc, calls } = fakeShell();
    const original = rule({ id: 14 });
    const draft = { ...draftFrom(original, "execute_code", "writer"), argPrefix: "git push --force" };

    // WHEN the change is saved
    const outcome = await saveRule(ipc, "edit", draft, original);

    // THEN the new rule is written before the old one goes
    expect(outcome).toBe("replaced");
    expect(calls.map((call) => call[0])).toEqual([
      "add_permission_prefix_rule",
      "governance_revoke_permission_rule",
    ]);
    expect(calls[1]).toEqual(["governance_revoke_permission_rule", 14]);
  });

  test("a change that changes nothing writes nothing", async () => {
    // GIVEN a rule and its own values as a draft
    const { ipc, calls } = fakeShell();
    const original = rule({});

    // WHEN the draft is saved as a change
    const outcome = await saveRule(ipc, "edit", draftFrom(original, "execute_code", "writer"), original);

    // THEN no command is sent
    expect(outcome).toBe("unchanged");
    expect(calls).toEqual([]);
  });

  test("a failed write leaves the rule being changed in place", async () => {
    // GIVEN a shell that refuses to write a rule
    const { ipc, calls } = fakeShell("add_permission_prefix_rule");
    const original = rule({ id: 14 });
    const draft = { ...draftFrom(original, "execute_code", "writer"), argPrefix: "git fetch" };

    // WHEN the change is saved
    const attempt = saveRule(ipc, "edit", draft, original);

    // THEN it fails and the original was never removed
    await expect(attempt).rejects.toThrow("refused");
    expect(calls.map((call) => call[0])).toEqual(["add_permission_prefix_rule"]);
  });

  test("a draft that cannot be saved reaches no command", async () => {
    // GIVEN a draft allowing the shell as a whole
    const { ipc, calls } = fakeShell();
    const draft = { ...draftFrom(null, "", "writer"), tool: "bash_executor", action: "allow" as const };

    // WHEN it is saved
    const attempt = saveRule(ipc, "add", draft, null);

    // THEN it is refused before any command
    await expect(attempt).rejects.toThrow("executor_needs_command");
    expect(calls).toEqual([]);
  });
});

describe("authorization actions - revoke all", () => {
  const rules = [rule({ id: 14 }), rule({ id: 20, scope: "agent", agent_id: "writer" })];
  const consents = [CONSENT, { ...CONSENT, agent_id: "sorter", family: "write_or_delete" }];

  test("for every agent, each scope is emptied and each consent withdrawn", async () => {
    // GIVEN the plan of a "revoke all" for every agent, rules and consents
    const { ipc, calls } = fakeShell();
    const plan = revokeAllPlan("both", null, rules, consents);

    // WHEN it runs
    const done = await runRevokeAll(ipc, plan);

    // THEN both scopes and both consents are withdrawn, and counted
    expect(calls).toEqual([
      ["governance_revoke_all_rules", "agent"],
      ["governance_revoke_all_rules", "global"],
      ["revoke_family_consent", "writer", "send_external"],
      ["revoke_family_consent", "sorter", "write_or_delete"],
    ]);
    expect(done).toEqual({ rules: 4, always: 2 });
  });

  test("for one agent, only its rules and its consents are withdrawn", async () => {
    // GIVEN the plan of a "revoke all" for one agent
    const { ipc, calls } = fakeShell();
    const plan = revokeAllPlan("both", "writer", rules, consents);

    // WHEN it runs
    const done = await runRevokeAll(ipc, plan);

    // THEN the rule for every agent and the other agent's consent stay
    expect(calls).toEqual([
      ["governance_revoke_permission_rule", 20],
      ["revoke_family_consent", "writer", "send_external"],
    ]);
    expect(done).toEqual({ rules: 1, always: 1 });
  });
});
