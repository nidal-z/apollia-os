/**
 * Typed Tauri IPC wrappers for automation triggers.
 *
 * The backend entity is a "trigger" (cron, interval, file-watch, webhook). The
 * operator vocabulary surfaces it as "automation". Keeping the `invoke` calls
 * here removes direct Tauri usage from the `.svelte` shell.
 */
import { invoke } from "@tauri-apps/api/core";
import type { TriggerStatus } from "$lib/types";

/** Every trigger with its current status. */
export function listTriggers(): Promise<TriggerStatus[]> {
  return invoke<TriggerStatus[]>("list_triggers");
}

/** Delete a trigger by id. */
export function deleteTrigger(id: string): Promise<void> {
  return invoke<void>("delete_trigger", { id });
}
