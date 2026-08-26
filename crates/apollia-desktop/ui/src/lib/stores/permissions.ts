import { writable, get } from "svelte/store";
import { invoke } from "@tauri-apps/api/core";

/** Scope of a permission rule, persisted or in memory. */
export type PermissionRuleScope = "session" | "project" | "agent" | "global";

/** Frontend representation of a rule `governance_list_permission_rules` exposes. */
export interface PermissionRuleDto {
  id: number;
  tool_name: string;
  arg_prefix: string | null;
  action: string;
  scope: PermissionRuleScope;
  project_path: string | null;
  agent_id: string | null;
  expires_at: string | null;
  created_at: string;
  created_by: string | null;
}

/** Entry of the immutable audit log of the permission decisions. */
export interface AuditEntryDto {
  id: number;
  tool_name: string;
  first_arg: string | null;
  decision: string;
  scope: string | null;
  rule_id: number | null;
  agent: string | null;
  decided_at: string;
}

export const permissionRules = writable<PermissionRuleDto[]>([]);
export const auditEntries = writable<AuditEntryDto[]>([]);
export const loadingRules = writable<boolean>(false);
export const loadingAudit = writable<boolean>(false);
export const rulesError = writable<string | null>(null);
export const auditError = writable<string | null>(null);
export const filterScope = writable<PermissionRuleScope | null>(null);
export const filterTool = writable<string | null>(null);

function toErrorMessage(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

/** Reloads the rule list from the backend, honouring the filters. */
export async function loadRules(): Promise<void> {
  loadingRules.set(true);
  rulesError.set(null);
  try {
    const filter = {
      scope: get(filterScope),
      tool_name: get(filterTool),
    };
    const list = await invoke<PermissionRuleDto[]>("governance_list_permission_rules", {
      filter,
    });
    permissionRules.set(list);
  } catch (err) {
    rulesError.set(toErrorMessage(err));
  } finally {
    loadingRules.set(false);
  }
}

/** Deletes one rule, then updates the local list. */
export async function revokeRule(id: number): Promise<void> {
  await invoke("governance_revoke_permission_rule", { ruleId: id });
  permissionRules.update((list) => list.filter((r) => r.id !== id));
}

/**
 * Deletes every rule of one scope (`null` means every scope at once).
 * Returns the number of rules the backend revoked.
 */
export async function revokeAll(
  scope: PermissionRuleScope | null,
): Promise<number> {
  const count = await invoke<number>("governance_revoke_all_rules", {
    scope,
  });
  await loadRules();
  return count;
}

/** Counts the rules matching a given scope, inside the local store. */
export function countRulesForScope(scope: PermissionRuleScope | null): number {
  const rules = get(permissionRules);
  if (scope === null) return rules.length;
  return rules.filter((r) => r.scope === scope).length;
}

/** Reloads the audit log with the optional backend parameters. */
export async function loadAudit(
  tool: string | null = null,
  limit = 50,
): Promise<void> {
  loadingAudit.set(true);
  auditError.set(null);
  try {
    const list = await invoke<AuditEntryDto[]>("governance_list_audit", {
      toolName: tool,
      limit,
      offset: 0,
    });
    auditEntries.set(list);
  } catch (err) {
    auditError.set(toErrorMessage(err));
  } finally {
    loadingAudit.set(false);
  }
}

/** Updates the scope filter and reloads the list. */
export async function setScopeFilter(
  scope: PermissionRuleScope | null,
): Promise<void> {
  filterScope.set(scope);
  await loadRules();
}

/** Updates the per-tool filter and reloads the list. */
export async function setToolFilter(tool: string | null): Promise<void> {
  filterTool.set(tool);
  await loadRules();
}

// ─────────────────────────────────────────────────────────────────────────────
// Free-chat agent rules - `scope = 'agent'` rules for `apollia:chat`.
// ─────────────────────────────────────────────────────────────────────────────

/** Rules scoped to the agent, assigned to the Apollia Chat system agent. */
export const chatPermissionRules = writable<PermissionRuleDto[]>([]);
export const loadingChatRules = writable<boolean>(false);
export const chatRulesError = writable<string | null>(null);

/** Reloads the `agent_id = "apollia:chat"` rules from the backend. */
export async function loadChatRules(): Promise<void> {
  loadingChatRules.set(true);
  chatRulesError.set(null);
  try {
    const list = await invoke<PermissionRuleDto[]>("list_chat_permission_rules");
    chatPermissionRules.set(list);
  } catch (err) {
    chatRulesError.set(toErrorMessage(err));
  } finally {
    loadingChatRules.set(false);
  }
}

/** Deletes one agent-scoped rule, then updates the local store. */
export async function deleteChatRule(id: number): Promise<void> {
  await invoke("delete_chat_permission_rule", { ruleId: id });
  chatPermissionRules.update((list) => list.filter((r) => r.id !== id));
}

// ─────────────────────────────────────────────────────────────────────────────
// Active session authorizations - `scope = 'session'` in-memory only.
// ─────────────────────────────────────────────────────────────────────────────

/** In-memory authorisation granted during one chat session. */
export interface SessionAuthorizationDto {
  session_id: string;
  session_title: string | null;
  mode: "libre" | "agent" | "companion" | (string & {});
  tool_name: string;
}

export const sessionAuthorizations = writable<SessionAuthorizationDto[]>([]);
export const loadingSessionAuths = writable<boolean>(false);
export const sessionAuthsError = writable<string | null>(null);

/** Reloads the in-memory authorisations of every active session. */
export async function loadSessionAuthorizations(): Promise<void> {
  loadingSessionAuths.set(true);
  sessionAuthsError.set(null);
  try {
    const list = await invoke<SessionAuthorizationDto[]>(
      "list_active_chat_session_authorizations",
    );
    sessionAuthorizations.set(list);
  } catch (err) {
    sessionAuthsError.set(toErrorMessage(err));
  } finally {
    loadingSessionAuths.set(false);
  }
}

/** Removes an in-memory authorisation and refreshes the store. */
export async function revokeSessionAuthorization(
  sessionId: string,
  toolName: string,
): Promise<void> {
  await invoke("revoke_chat_session_authorization", {
    sessionId,
    toolName,
  });
  sessionAuthorizations.update((list) =>
    list.filter(
      (e) => !(e.session_id === sessionId && e.tool_name === toolName),
    ),
  );
}
