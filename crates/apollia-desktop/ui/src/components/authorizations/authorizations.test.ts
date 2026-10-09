import { describe, expect, test } from "vitest";
import type { FamilyConsent, PermissionRuleDto } from "$lib/ipc/permissions";
import type { OrgPolicyBody, PolicyView } from "$lib/ipc/policy";
import { accessView, groupAccesses, type AccessDescriptor } from "../agents/sheet/agentSheet";
import {
  ALL_AGENTS,
  agentSummary,
  alwaysOf,
  canCreateRule,
  consentKey,
  draftFrom,
  draftProblem,
  familyLines,
  imposesSomething,
  isSimpleCommand,
  matchesFilter,
  planIsEmpty,
  policySummary,
  revokeAllPlan,
  ruleActions,
  ruleSentence,
  rulesOf,
  sameAsRule,
} from "./authorizations";

function rule(partial: Partial<PermissionRuleDto>): PermissionRuleDto {
  return {
    id: 1,
    tool_name: "bash_executor",
    arg_prefix: null,
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

function always(agent: string, family: string, by = "user"): FamilyConsent {
  return { agent_id: agent, family, granted_at: 1_790_000_000, granted_by: by };
}

function descriptor(family: AccessDescriptor["family"], description: string): AccessDescriptor {
  return { description, family, family_source: "builtin", kind: "native" };
}

const GROUPS = groupAccesses([
  accessView("file_read", descriptor("read", "Read the files of the Reports folder")),
  accessView("web_search", descriptor("read", "Search the web")),
  accessView("file_write", descriptor("write_or_delete", "Write in the Reports folder")),
  accessView("send_mail", descriptor("send_external", "Send a message")),
  accessView("agent:sorter", null),
]);

function body(partial: Partial<OrgPolicyBody>): OrgPolicyBody {
  return {
    schema_version: 1,
    signed_at: "2026-09-30T08:00:00Z",
    connections: { allow: [], deny: [], remote: null },
    families: { deny: [] },
    inference: { allowed_servers: [], remote_endpoints: null, require_model_hash: false },
    settings: {},
    packages: { require_signature: false, trusted_publishers: [] },
    journal: { required: false, retention_days: null, export: { detail: null, daily_summary_dir: null } },
    ...partial,
  };
}

function active(partial: Partial<OrgPolicyBody>): PolicyView {
  return {
    state: "active",
    dir: "/etc/apollia",
    key_fingerprint: "ab:cd",
    publisher: "IT department",
    signed_at: "2026-09-30T08:00:00Z",
    policy: body(partial),
  };
}

const NOTHING_WITHDRAWN = { consents: new Set<string>(), rules: new Set<number>() };

describe("authorizations - an agent at a glance", () => {
  test("the accesses are grouped by family, each with what it covers and what it asks", () => {
    // GIVEN an agent with accesses in three families and a standing consent
    // for sending outside
    const consents = [always("writer", "send_external")];

    // WHEN its families are listed with no policy
    const lines = familyLines(GROUPS, policySummary(null), consents);

    // THEN only the families it holds are listed, in the order of governance,
    // reading without asking, writing asking first, sending already agreed
    expect(lines.map((line) => [line.family, line.ask])).toEqual([
      ["read", "without_asking"],
      ["write_or_delete", "asks_first"],
      ["send_external", "always"],
    ]);
    expect(lines[0].covers).toEqual(["Read the files of the Reports folder", "Search the web"]);
  });

  test("the standing consents and the fine rules are those of the agent, the rules for all apart", () => {
    // GIVEN the consents of two agents, a rule for one agent, one for another
    // and one for every agent
    const consents = [always("writer", "send_external"), always("sorter", "write_or_delete")];
    const rules = [
      rule({ id: 14 }),
      rule({ id: 20, scope: "agent", agent_id: "writer" }),
      rule({ id: 21, scope: "agent", agent_id: "sorter" }),
    ];

    // WHEN what one agent holds is read, then what every agent is under
    const mine = alwaysOf(consents, "writer");
    const myRules = rulesOf(rules, "writer");
    const forAll = rulesOf(rules, ALL_AGENTS);

    // THEN each list holds only its own
    expect(mine.map(consentKey)).toEqual(["writer::send_external"]);
    expect(myRules.map((r) => r.id)).toEqual([20]);
    expect(forAll.map((r) => r.id)).toEqual([14]);
  });

  test("what was just withdrawn no longer counts under the agent's name", () => {
    // GIVEN an agent with two standing consents and one rule
    const consents = [always("writer", "send_external"), always("writer", "write_or_delete")];
    const rules = [rule({ id: 20, scope: "agent", agent_id: "writer" })];

    // WHEN one consent and the rule have been withdrawn
    const before = agentSummary("writer", 3, consents, rules, NOTHING_WITHDRAWN);
    const after = agentSummary("writer", 3, consents, rules, {
      consents: new Set(["writer::send_external"]),
      rules: new Set([20]),
    });

    // THEN the counts follow, and the families granted do not move
    expect(before).toEqual({ families: 3, always: 2, rules: 1 });
    expect(after).toEqual({ families: 3, always: 1, rules: 0 });
  });

  test("the filter finds an agent by its name, its description or one of its accesses", () => {
    // GIVEN an agent and the words its accesses are shown with
    const entry = { id: "writer", name: "Report writer", description: "Writes the weekly report" };
    const words = ["Send outside", "Gmail"];

    // WHEN it is filtered by a name, a word of its accesses, and a stranger
    // THEN the first two find it, the last does not, and no filter keeps it
    expect(matchesFilter("report", entry, words)).toBe(true);
    expect(matchesFilter("gmail", entry, words)).toBe(true);
    expect(matchesFilter("invoice", entry, words)).toBe(false);
    expect(matchesFilter("  ", entry, words)).toBe(true);
  });

  test("a rule reads in plain words, a command for a tool that runs code", () => {
    // GIVEN a rule on a command, one on a tool with a prefix, one on a whole tool
    const command = rule({ arg_prefix: "git push" });
    const prefixed = rule({ tool_name: "file_read", arg_prefix: "/models" });
    const whole = rule({ tool_name: "file_read" });

    // WHEN each is put in words
    // THEN the command is quoted and the other two name what the tool does
    expect(ruleSentence(command, "Run a shell command")).toEqual({
      key: "authorizations.rule_run_command",
      values: { command: "git push" },
    });
    expect(ruleSentence(prefixed, "Read a file").key).toBe("authorizations.rule_tool_prefixed");
    expect(ruleSentence(whole, "Read a file")).toEqual({
      key: "authorizations.rule_tool_all",
      values: { what: "Read a file" },
    });
  });
});

describe("authorizations - what the policy imposes", () => {
  test("a forbidden family is read only at the agent, with the policy's mention", () => {
    // GIVEN a policy that forbids sending outside, and an agent holding that
    // family with a standing consent
    const summary = policySummary(active({ families: { deny: ["send_external"] } }));

    // WHEN the agent's families are listed
    const lines = familyLines(GROUPS, summary, [always("writer", "send_external")]);

    // THEN the forbidden family says it is imposed, whatever was consented,
    // and the others are untouched
    expect(lines.find((line) => line.family === "send_external")?.ask).toBe("imposed");
    expect(lines.find((line) => line.family === "read")?.ask).toBe("without_asking");
  });

  test("forbidden families, forbidden connections and fixed settings are all counted", () => {
    // GIVEN a policy that forbids a family, two connections and the remote
    // ones, and fixes two settings
    const view = active({
      families: { deny: ["execute_code"] },
      connections: { allow: [], deny: ["selection:slack", "package:npm:x"], remote: "deny" },
      settings: { "desktop.developer_mode": false, "updates.channel": "stable" },
    });

    // WHEN it is summarised
    const summary = policySummary(view);

    // THEN each kind of constraint is there, and the screen has something to mention
    expect(summary.state).toBe("active");
    expect(summary.deniedFamilies).toEqual(["execute_code"]);
    expect(summary.deniedConnections).toEqual(["selection:slack", "package:npm:x", "remote"]);
    expect(summary.fixedSettings).toEqual(["desktop.developer_mode", "updates.channel"]);
    expect(imposesSomething(summary)).toBe(true);
  });

  test("no policy and a refused policy impose nothing, and say which they are", () => {
    // GIVEN no policy, a policy not read yet, and a refused one
    const none = policySummary({ state: "none", dir: "/etc/apollia" });
    const unread = policySummary(null);
    const invalid = policySummary({ state: "invalid", dir: "/etc/apollia", reason: "signature", detail: "bad" });

    // WHEN each is summarised
    // THEN nothing is imposed, and the refused one keeps its reason
    expect([none.state, unread.state, invalid.state]).toEqual(["none", "none", "invalid"]);
    expect(invalid.reason).toBe("signature");
    expect([none, unread, invalid].some(imposesSomething)).toBe(false);
  });
});

describe("authorizations - fine rules and the developer mode", () => {
  test("outside developer mode a rule can only be withdrawn, and none created", () => {
    // GIVEN the developer mode off
    // WHEN the actions of a rule and the right to create one are read
    // THEN withdrawing is the only action and creation is closed
    expect(ruleActions(false)).toEqual(["withdraw"]);
    expect(canCreateRule(false)).toBe(false);
  });

  test("in developer mode a rule is changed, copied and withdrawn, and one can be created", () => {
    // GIVEN the developer mode on
    // WHEN the actions of a rule and the right to create one are read
    // THEN the three actions are offered and creation is open
    expect(ruleActions(true)).toEqual(["edit", "duplicate", "withdraw"]);
    expect(canCreateRule(true)).toBe(true);
  });

  test("a new rule refuses by default and targets every agent", () => {
    // GIVEN no rule to start from
    // WHEN the dialog opens on a new rule
    const draft = draftFrom(null, "", "writer");

    // THEN it refuses, names no tool, and targets every agent
    expect(draft).toEqual({ action: "deny", family: "", tool: "", argPrefix: "", scope: "all", agentId: "writer" });
    expect(draftProblem(draft)).toBe("tool_missing");
  });

  test("duplicating a rule fills the form with it, for the same agent or for all", () => {
    // GIVEN a rule for every agent and a rule for one agent
    const forAll = rule({ id: 14, arg_prefix: "git push" });
    const forOne = rule({ id: 20, tool_name: "file_read", action: "allow", scope: "agent", agent_id: "sorter" });

    // WHEN each is copied into a draft
    const first = draftFrom(forAll, "execute_code", "writer");
    const second = draftFrom(forOne, "read", "writer");

    // THEN the drafts say what the rules say and can be saved as they are
    expect(first).toEqual({
      action: "deny",
      family: "execute_code",
      tool: "bash_executor",
      argPrefix: "git push",
      scope: "all",
      agentId: "writer",
    });
    expect(second.scope).toBe("agent");
    expect(second.agentId).toBe("sorter");
    expect(sameAsRule(first, forAll)).toBe(true);
    expect(sameAsRule(second, forOne)).toBe(true);
    expect(draftProblem(first)).toBeNull();
  });

  test("a rule that allows a tool of a family is refused, since it would decide nothing", () => {
    // GIVEN drafts that allow a writing tool, refuse it, allow a tool of no
    // family, and allow the shell on one simple command
    const base = { family: "" as const, argPrefix: "", scope: "all" as const, agentId: "writer" };
    const allowWrite = { ...base, action: "allow" as const, tool: "file_write" };
    const denyWrite = { ...base, action: "deny" as const, tool: "file_write" };
    const allowUnclassified = { ...base, action: "allow" as const, tool: "mcp:notes/lookup" };
    const allowCommand = { ...base, action: "allow" as const, tool: "bash_executor", argPrefix: "git status" };

    // WHEN each is checked with the family of its tool
    // THEN only the rule that allows a tool of a family is refused
    expect(draftProblem(allowWrite, "write_or_delete")).toBe("allow_without_effect");
    expect(draftProblem(denyWrite, "write_or_delete")).toBeNull();
    expect(draftProblem(allowUnclassified, "")).toBeNull();
    expect(draftProblem(allowCommand, "execute_code")).toBeNull();
  });

  test("allowing a tool that runs code needs one simple command", () => {
    // GIVEN drafts that allow the shell: as a whole, with a chained command,
    // with a simple command; and one that refuses it as a whole
    const base = draftFrom(null, "", "writer");
    const blanket = { ...base, tool: "bash_executor", action: "allow" as const };
    const chained = { ...blanket, argPrefix: "git status && rm -rf /" };
    const simple = { ...blanket, argPrefix: "git status" };
    const refused = { ...base, tool: "bash_executor" };

    // WHEN each is checked
    // THEN only the simple command and the refusal can be saved
    expect(draftProblem(blanket)).toBe("executor_needs_command");
    expect(draftProblem(chained)).toBe("command_not_simple");
    expect(draftProblem(simple)).toBeNull();
    expect(draftProblem(refused)).toBeNull();
    expect(isSimpleCommand("ls $(pwd)")).toBe(false);
  });

  test("a rule for one agent needs that agent", () => {
    // GIVEN a draft that targets one agent without naming it
    const draft = { ...draftFrom(null, "", ""), tool: "file_read", scope: "agent" as const };

    // WHEN it is checked, then given its agent
    // THEN it is refused first and accepted after
    expect(draftProblem(draft)).toBe("agent_missing");
    expect(draftProblem({ ...draft, agentId: "writer" })).toBeNull();
  });
});

describe("authorizations - revoke all", () => {
  const rules = [
    rule({ id: 14 }),
    rule({ id: 15, tool_name: "file_read", action: "allow" }),
    rule({ id: 20, scope: "agent", agent_id: "writer" }),
    rule({ id: 21, scope: "agent", agent_id: "sorter" }),
  ];
  const consents = [
    always("writer", "send_external"),
    always("writer", "write_or_delete"),
    always("sorter", "write_or_delete"),
  ];

  test("for every agent, both: every rule by scope and every standing consent", () => {
    // GIVEN rules of both scopes and consents of two agents
    // WHEN everything is to be revoked for every agent
    const plan = revokeAllPlan("both", null, rules, consents);

    // THEN both scopes are emptied and every consent withdrawn
    expect(plan.bulkScopes).toEqual(["agent", "global"]);
    expect(plan.ruleIds).toEqual([]);
    expect(plan.ruleCount).toBe(4);
    expect(plan.consents).toHaveLength(3);
  });

  test("for one agent, its rules and its consents only; the rules for all stay", () => {
    // GIVEN the same rules and consents
    // WHEN everything is to be revoked for one agent
    const plan = revokeAllPlan("both", "writer", rules, consents);

    // THEN only what targets that agent is in the plan
    expect(plan.bulkScopes).toEqual([]);
    expect(plan.ruleIds).toEqual([20]);
    expect(plan.ruleCount).toBe(1);
    expect(plan.consents.map(consentKey)).toEqual(["writer::send_external", "writer::write_or_delete"]);
  });

  test("the choice of what is withdrawn is kept: rules only, standing consents only", () => {
    // GIVEN the same rules and consents
    // WHEN only the rules, then only the consents, are to be revoked
    const onlyRules = revokeAllPlan("rules", null, rules, consents);
    const onlyAlways = revokeAllPlan("always", "sorter", rules, consents);

    // THEN each plan leaves the other kind alone
    expect(onlyRules.consents).toEqual([]);
    expect(onlyRules.ruleCount).toBe(4);
    expect(onlyAlways.ruleCount).toBe(0);
    expect(onlyAlways.ruleIds).toEqual([]);
    expect(onlyAlways.consents.map(consentKey)).toEqual(["sorter::write_or_delete"]);
  });

  test("a plan that withdraws nothing cannot be confirmed", () => {
    // GIVEN an agent with neither rule nor consent
    // WHEN everything is to be revoked for it
    const plan = revokeAllPlan("both", "analyst", rules, consents);

    // THEN the plan is empty
    expect(planIsEmpty(plan)).toBe(true);
    expect(planIsEmpty(revokeAllPlan("both", null, rules, consents))).toBe(false);
  });
});
