import { writable, get } from "svelte/store";
import { invoke } from "@tauri-apps/api/core";

/** Snapshot d'un outil natif renvoyé par `governance_list_tools`. */
export interface ToolStatusDto {
  name: string;
  enabled: boolean;
  config: Record<string, unknown> | null;
  credential_keys: string[];
  active_backend: string | null;
}

/** Métadonnée d'une credential — la valeur claire ne traverse jamais la frontière IPC. */
export interface CredentialEntryDto {
  tool_name: string;
  key_name: string;
  created_at: string;
  last_used_at: string | null;
}

/** Résultat d'une validation live de credential. */
export interface CredentialTestResultDto {
  ok: boolean;
  latency_ms: number | null;
  error: string | null;
}

/** Store global de la page `/settings/tools`. */
export const tools = writable<ToolStatusDto[]>([]);
export const loadingTools = writable<boolean>(false);
export const toolsError = writable<string | null>(null);

function toErrorMessage(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

/** Recharge la liste complète des outils depuis le backend. */
export async function loadTools(): Promise<void> {
  loadingTools.set(true);
  toolsError.set(null);
  try {
    const list = await invoke<ToolStatusDto[]>("governance_list_tools");
    tools.set(list);
  } catch (err) {
    toolsError.set(toErrorMessage(err));
  } finally {
    loadingTools.set(false);
  }
}

/**
 * Active ou désactive un outil. Le store applique l'état optimiste avant
 * l'appel et rollback automatiquement si l'IPC échoue.
 */
export async function toggleTool(name: string, enabled: boolean): Promise<void> {
  const previous = get(tools);
  const optimistic = previous.map((t) =>
    t.name === name ? { ...t, enabled } : t,
  );
  tools.set(optimistic);
  try {
    await invoke("governance_set_tool_enabled", { toolName: name, enabled });
  } catch (err) {
    tools.set(previous);
    throw new Error(toErrorMessage(err));
  }
}

/** Lit la configuration JSON d'un outil. */
export async function getToolConfig(
  name: string,
): Promise<Record<string, unknown> | null> {
  const value = await invoke<Record<string, unknown> | null>(
    "governance_get_tool_config",
    { toolName: name },
  );
  return value;
}

/** Persiste la configuration JSON d'un outil et synchronise le store local. */
export async function updateToolConfig(
  name: string,
  config: Record<string, unknown>,
): Promise<void> {
  await invoke("governance_set_tool_config", { toolName: name, config });
  tools.update((list) =>
    list.map((t) => (t.name === name ? { ...t, config } : t)),
  );
}

/**
 * Stocke une credential pour un outil. La valeur claire est transmise une
 * unique fois à Rust et n'est pas conservée côté frontend.
 */
export async function setCredential(
  toolName: string,
  keyName: string,
  value: string,
): Promise<void> {
  await invoke("governance_set_credential", { toolName, keyName, value });
  tools.update((list) =>
    list.map((t) =>
      t.name === toolName && !t.credential_keys.includes(keyName)
        ? { ...t, credential_keys: [...t.credential_keys, keyName] }
        : t,
    ),
  );
}

/** Supprime une credential pour un outil. */
export async function deleteCredential(
  toolName: string,
  keyName: string,
): Promise<void> {
  await invoke("governance_delete_credential", { toolName, keyName });
  tools.update((list) =>
    list.map((t) =>
      t.name === toolName
        ? { ...t, credential_keys: t.credential_keys.filter((k) => k !== keyName) }
        : t,
    ),
  );
}

/** Teste une credential live (web_search/Brave aujourd'hui). */
export async function testCredential(
  toolName: string,
): Promise<CredentialTestResultDto> {
  return invoke<CredentialTestResultDto>("governance_test_credential", {
    toolName,
  });
}

/**
 * Recharge l'état des outils depuis la DB. Pas de redémarrage du dispatcher
 * exposé côté IPC : le bouton "Recharger" actualise la liste rendue.
 */
export async function reloadDispatcher(): Promise<void> {
  await loadTools();
}
