import { writable, get } from "svelte/store";
import { invoke } from "@tauri-apps/api/core";

/** Portée d'une règle de permission persistée ou en mémoire. */
export type PermissionRuleScope = "session" | "project" | "agent" | "global";

/** Représentation frontend d'une règle exposée par `governance_list_permission_rules`. */
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

/** Entrée de l'audit log immuable des décisions de permission. */
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

/** Filtres appliqués à la liste des règles. `null` ⇒ pas de filtre. */
export interface PermissionRuleFilter {
  scope: PermissionRuleScope | null;
  tool_name: string | null;
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

/** Recharge la liste des règles depuis le backend, en respectant les filtres. */
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

/** Supprime une règle individuelle puis met à jour la liste locale. */
export async function revokeRule(id: number): Promise<void> {
  await invoke("governance_revoke_permission_rule", { ruleId: id });
  permissionRules.update((list) => list.filter((r) => r.id !== id));
}

/**
 * Supprime toutes les règles d'un scope (`null` ⇒ toutes portées confondues).
 * Retourne le nombre de règles révoquées côté backend.
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

/** Compte les règles correspondant à un scope donné, dans le store local. */
export function countRulesForScope(scope: PermissionRuleScope | null): number {
  const rules = get(permissionRules);
  if (scope === null) return rules.length;
  return rules.filter((r) => r.scope === scope).length;
}

/** Recharge l'audit log avec les paramètres optionnels du backend. */
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

/** Met à jour le filtre de scope et recharge la liste. */
export async function setScopeFilter(
  scope: PermissionRuleScope | null,
): Promise<void> {
  filterScope.set(scope);
  await loadRules();
}

/** Met à jour le filtre par outil et recharge la liste. */
export async function setToolFilter(tool: string | null): Promise<void> {
  filterTool.set(tool);
  await loadRules();
}

// ─────────────────────────────────────────────────────────────────────────────
// Chat-libre agent rules — règles `scope = 'agent'` pour `apollia:chat`.
// ─────────────────────────────────────────────────────────────────────────────

/** Liste des règles agent-scoped attribuées à l'agent système Apollia Chat. */
export const chatPermissionRules = writable<PermissionRuleDto[]>([]);
export const loadingChatRules = writable<boolean>(false);
export const chatRulesError = writable<string | null>(null);

/** Recharge les règles `agent_id = "apollia:chat"` depuis le backend. */
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

/** Supprime une règle agent-scoped puis met à jour le store local. */
export async function deleteChatRule(id: number): Promise<void> {
  await invoke("delete_chat_permission_rule", { ruleId: id });
  chatPermissionRules.update((list) => list.filter((r) => r.id !== id));
}

// ─────────────────────────────────────────────────────────────────────────────
// Active session authorizations — `scope = 'session'` in-memory only.
// ─────────────────────────────────────────────────────────────────────────────

/** Autorisation in-memory accordée pendant une session de chat. */
export interface SessionAuthorizationDto {
  session_id: string;
  session_title: string | null;
  mode: "libre" | "agent" | "companion" | (string & {});
  tool_name: string;
}

export const sessionAuthorizations = writable<SessionAuthorizationDto[]>([]);
export const loadingSessionAuths = writable<boolean>(false);
export const sessionAuthsError = writable<string | null>(null);

/** Recharge les autorisations in-memory de toutes les sessions actives. */
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

/** Retire une autorisation in-memory et rafraîchit le store. */
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
