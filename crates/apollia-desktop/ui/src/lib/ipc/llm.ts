// Typed IPC wrappers for the LLM surface.
//
// Centralises every `invoke` the top-level "AI Models" page and its cards
// need, so `.svelte` files never call `invoke` directly (frontend rule). The
// cost-breakdown call already lives in `./llmCosts.ts`; this module covers the
// backend list, ping, default selection, engine reload and deletion.

import { invoke } from "@tauri-apps/api/core";
import type {
  LlmBackendConfig,
  LlmPingResult,
  LlmCostStatsResponse,
} from "$lib/types";

/** Lists the configured LLM backends (CRUD view). */
export async function listLlmBackends(): Promise<LlmBackendConfig[]> {
  return invoke<LlmBackendConfig[]>("list_llm_backends");
}

/** Pings a backend by name and returns availability + latency. */
export async function pingLlmBackend(name: string): Promise<LlmPingResult> {
  return invoke<LlmPingResult>("ping_llm_backend", { name });
}

/** Promotes a backend to the default and returns once persisted. */
export async function setDefaultLlmBackend(name: string): Promise<void> {
  return invoke<void>("set_default_llm_backend", { name });
}

/**
 * Rebuilds the in-memory router from `system.db` so a config change (default
 * switch, edit) takes effect immediately, without freeing the loaded GGUF.
 */
export async function reloadLlmFromDb(): Promise<void> {
  return invoke<void>("reload_llm_from_db");
}

/**
 * Full engine reload: frees the current GGUF from RAM and rebuilds the router.
 * Heavier than {@link reloadLlmFromDb}; used by the explicit "Reload" action.
 */
export async function reloadLlm(): Promise<void> {
  return invoke<void>("reload_llm");
}

/** Deletes a backend by name. */
export async function deleteLlmBackend(name: string): Promise<void> {
  return invoke<void>("delete_llm_backend", { name });
}

/** Reads aggregated cost/token stats over a trailing window (days). */
export async function getLlmCostStats(days: number): Promise<LlmCostStatsResponse> {
  return invoke<LlmCostStatsResponse>("get_llm_cost_stats", { days });
}
