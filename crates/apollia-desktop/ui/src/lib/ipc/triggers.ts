/**
 * Typed Tauri IPC wrappers for automation triggers.
 *
 * The backend entity is a "trigger" (cron, interval, file-watch, webhook). The
 * operator vocabulary surfaces it as "automation". Keeping the `invoke` calls
 * here removes direct Tauri usage from the `.svelte` shell.
 */
import { invoke } from "@tauri-apps/api/core";
import type {
  CreateTriggerRequest,
  TriggerDefinitionView,
  TriggerFireResult,
  TriggerLogEntry,
  TriggerStatus,
  UpdateTriggerRequest,
} from "$lib/types";

/**
 * A stored definition as the webview receives it.
 *
 * The host strips secret material from `source_config` before answering, so a
 * webhook HMAC secret never reaches the renderer context. `has_secret` is the
 * presence marker that replaces it.
 */
export interface RedactedTriggerDefinition extends TriggerDefinitionView {
  /** True when a secret is stored for this trigger. Its value stays host-side. */
  has_secret: boolean;
}

/** Every trigger with its current status. */
export function listTriggers(): Promise<TriggerStatus[]> {
  return invoke<TriggerStatus[]>("list_triggers");
}

/**
 * Full stored definition of a trigger, secret material removed.
 *
 * Answers a 404 for a trigger declared in `apollia.toml`: those run in the
 * engine but have no row in the definition store.
 */
export function getTriggerDefinition(id: string): Promise<RedactedTriggerDefinition> {
  return invoke<RedactedTriggerDefinition>("get_trigger_definition", { id });
}

/**
 * Update an existing trigger and return the stored definition.
 *
 * The identifier is immutable: it addresses the row and is not part of the
 * payload. The engine is reloaded by the runtime once the write lands.
 *
 * A webhook `secret` left empty means "keep the stored one": the host reads it
 * back and completes the payload, so the webview never has to hold the value.
 */
export function updateTrigger(
  id: string,
  definition: UpdateTriggerRequest,
): Promise<RedactedTriggerDefinition> {
  return invoke<RedactedTriggerDefinition>("update_trigger", { id, definition });
}

/** Delete a trigger by id. */
export function deleteTrigger(id: string): Promise<void> {
  return invoke<void>("delete_trigger", { id });
}

/** Create a trigger and return the stored definition. */
export function createTrigger(
  definition: CreateTriggerRequest,
): Promise<TriggerDefinitionView> {
  return invoke<TriggerDefinitionView>("create_trigger", { definition });
}

/** Fire a trigger immediately, outside its schedule. */
export function fireTrigger(id: string): Promise<TriggerFireResult> {
  return invoke<TriggerFireResult>("fire_trigger", { id });
}

/** Pause or resume a trigger. */
export function setTriggerEnabled(id: string, enabled: boolean): Promise<void> {
  return invoke<void>("set_trigger_enabled", { id, enabled });
}

/** Recent firing log of one trigger, newest first. */
export function getTriggerLogs(id: string, limit: number): Promise<TriggerLogEntry[]> {
  return invoke<TriggerLogEntry[]>("get_trigger_logs", { id, limit });
}
