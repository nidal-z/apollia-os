/**
 * Always-accept permission rules store.
 *
 * Rules are owned by the backend; this store mirrors them locally and
 * exposes a small CRUD surface.
 */
import { writable } from "svelte/store";
import { invoke } from "@tauri-apps/api/core";
import type { AlwaysAcceptScope } from "$lib/permissions/telemetry";

export interface AlwaysAcceptRule {
  id: string;
  scope: AlwaysAcceptScope;
  tool: string;
  agent_id?: string;
  session_id?: string;
  created_at: string;
  last_used_at?: string;
  /**
   * Identifiant de l'auteur de la règle.
   *
   * Valeurs typiques : `"user-hitl"` (bouton « toujours autoriser »),
   * `"user-settings"` (édition manuelle UI), `"onboarding-agent"` (ou tout autre
   * agent système), `"config-import"` (migration `safe_list` au boot du moteur).
   * `undefined` lorsque l'écriture est antérieure au tracking.
   */
  created_by?: string;
}

export const alwaysAcceptRules = writable<AlwaysAcceptRule[]>([]);

export async function loadAlwaysAcceptRules(): Promise<void> {
  try {
    const rules = await invoke<AlwaysAcceptRule[]>("list_always_accept_rules");
    alwaysAcceptRules.set(Array.isArray(rules) ? rules : []);
  } catch {
    alwaysAcceptRules.set([]);
  }
}

export async function createAlwaysAcceptRule(
  input: Omit<AlwaysAcceptRule, "id" | "created_at" | "last_used_at">,
): Promise<void> {
  await invoke("create_always_accept_rule", { rule: input });
  await loadAlwaysAcceptRules();
}

export async function revokeAlwaysAcceptRule(id: string): Promise<void> {
  await invoke("revoke_always_accept_rule", { ruleId: id });
  await loadAlwaysAcceptRules();
}

export async function revokeAllAlwaysAcceptRules(): Promise<void> {
  await invoke("revoke_all_always_accept_rules");
  await loadAlwaysAcceptRules();
}
