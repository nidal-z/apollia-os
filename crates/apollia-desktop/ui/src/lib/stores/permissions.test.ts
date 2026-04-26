import { vi, describe, test, expect, beforeEach } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

import { get } from "svelte/store";
import { invoke } from "@tauri-apps/api/core";
import {
  permissionRules,
  filterScope,
  filterTool,
  loadRules,
  revokeRule,
  revokeAll,
  countRulesForScope,
  setScopeFilter,
  setToolFilter,
  type PermissionRuleDto,
} from "./permissions";

const mockedInvoke = vi.mocked(invoke);

function rule(partial: Partial<PermissionRuleDto>): PermissionRuleDto {
  return {
    id: 1,
    tool_name: "bash_executor",
    arg_prefix: null,
    action: "allow",
    scope: "global",
    project_path: null,
    expires_at: null,
    created_at: "2026-04-20T10:00:00Z",
    created_by: null,
    ...partial,
  };
}

beforeEach(() => {
  permissionRules.set([]);
  filterScope.set(null);
  filterTool.set(null);
  vi.clearAllMocks();
});

describe("loadRules", () => {
  test("populates the store using current filters", async () => {
    // GIVEN
    filterScope.set("project");
    filterTool.set("bash_executor");
    mockedInvoke.mockResolvedValueOnce([rule({ id: 42, scope: "project" })]);

    // WHEN
    await loadRules();

    // THEN
    expect(mockedInvoke).toHaveBeenCalledWith("governance_list_permission_rules", {
      filter: { scope: "project", tool_name: "bash_executor" },
    });
    const list = get(permissionRules);
    expect(list).toHaveLength(1);
    expect(list[0].id).toBe(42);
  });
});

describe("revokeRule", () => {
  test("removes the rule from the local store after the IPC call", async () => {
    // GIVEN
    permissionRules.set([rule({ id: 7 }), rule({ id: 9 })]);
    mockedInvoke.mockResolvedValueOnce(undefined);

    // WHEN
    await revokeRule(7);

    // THEN
    expect(mockedInvoke).toHaveBeenCalledWith("governance_revoke_permission_rule", {
      ruleId: 7,
    });
    expect(get(permissionRules).map((r) => r.id)).toEqual([9]);
  });
});

describe("revokeAll", () => {
  test("forwards the scope and reloads the list", async () => {
    // GIVEN
    mockedInvoke
      .mockResolvedValueOnce(3)
      .mockResolvedValueOnce([rule({ id: 1, scope: "session" })]);

    // WHEN
    const removed = await revokeAll("global");

    // THEN
    expect(removed).toBe(3);
    expect(mockedInvoke).toHaveBeenNthCalledWith(1, "governance_revoke_all_rules", {
      scope: "global",
    });
    expect(mockedInvoke).toHaveBeenNthCalledWith(2, "governance_list_permission_rules", {
      filter: { scope: null, tool_name: null },
    });
  });
});

describe("countRulesForScope", () => {
  test("filters by scope and supports null = total", () => {
    // GIVEN
    permissionRules.set([
      rule({ id: 1, scope: "session" }),
      rule({ id: 2, scope: "project" }),
      rule({ id: 3, scope: "project" }),
      rule({ id: 4, scope: "global" }),
    ]);

    // THEN
    expect(countRulesForScope(null)).toBe(4);
    expect(countRulesForScope("session")).toBe(1);
    expect(countRulesForScope("project")).toBe(2);
    expect(countRulesForScope("global")).toBe(1);
  });
});

describe("setScopeFilter / setToolFilter", () => {
  test("updates the writable and triggers loadRules", async () => {
    // GIVEN
    mockedInvoke.mockResolvedValue([]);

    // WHEN
    await setScopeFilter("project");
    await setToolFilter("file_write");

    // THEN
    expect(get(filterScope)).toBe("project");
    expect(get(filterTool)).toBe("file_write");
    expect(mockedInvoke).toHaveBeenLastCalledWith("governance_list_permission_rules", {
      filter: { scope: "project", tool_name: "file_write" },
    });
  });
});
