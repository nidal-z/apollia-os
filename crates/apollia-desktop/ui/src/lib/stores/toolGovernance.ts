import { writable, get } from "svelte/store";
import { invoke } from "@tauri-apps/api/core";

/** Snapshot of one native tool, as `governance_list_tools` returns it. */
export interface ToolStatusDto {
  name: string;
  enabled: boolean;
  config: Record<string, unknown> | null;
  credential_keys: string[];
  active_backend: string | null;
}

/** Result of a live credential validation. */
export interface CredentialTestResultDto {
  ok: boolean;
  latency_ms: number | null;
  error: string | null;
}

/** Global store of the `/settings/tools` page. */
export const tools = writable<ToolStatusDto[]>([]);
export const loadingTools = writable<boolean>(false);
export const toolsError = writable<string | null>(null);

function toErrorMessage(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

/** Reloads the full tool list from the backend. */
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
 * Enables or disables a tool. The store applies the optimistic state before
 * the call, and rolls back automatically when the IPC fails.
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

/** Reads the JSON configuration of one tool. */
export async function getToolConfig(
  name: string,
): Promise<Record<string, unknown> | null> {
  const value = await invoke<Record<string, unknown> | null>(
    "governance_get_tool_config",
    { toolName: name },
  );
  return value;
}

/** Persists the JSON configuration of one tool and syncs the local store. */
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
 * Stores a credential for one tool. The clear value is passed to Rust once,
 * and is not kept on the frontend side.
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

/** Deletes a credential of one tool. */
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

/** Tests a live credential (web_search/Brave today). */
export async function testCredential(
  toolName: string,
): Promise<CredentialTestResultDto> {
  return invoke<CredentialTestResultDto>("governance_test_credential", {
    toolName,
  });
}

/**
 * Reloads the tool state from the DB. No dispatcher restart is exposed over
 * IPC: the "Reload" button refreshes the rendered list.
 */
export async function reloadDispatcher(): Promise<void> {
  await loadTools();
}
