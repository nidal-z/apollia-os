// Typed IPC wrappers for the engines of the AI screen.
//
// Centralises the `invoke` calls that read the configured engines and the
// local models resident in memory, so `.svelte` files and stores never call
// `invoke` directly (frontend rule). The servers live in `./llmServers.ts`,
// the cost breakdown in `./llmCosts.ts`.

import { invoke } from "@tauri-apps/api/core";
import type { LlmBackendConfig, LoadedModelsResponse, UnloadedModel } from "$lib/types";

/** Lists the configured engines. The one reader of that list is `stores/sse.ts`. */
export async function listLlmBackends(): Promise<LlmBackendConfig[]> {
  return invoke<LlmBackendConfig[]>("list_llm_backends");
}

/** Lists the local models resident in memory. */
export async function listLoadedModels(): Promise<LoadedModelsResponse> {
  return invoke<LoadedModelsResponse>("list_loaded_models");
}

/**
 * Unloads one resident model by the path the list reports. Rejects with the
 * runtime's message when the model is answering a request.
 */
export async function unloadLoadedModel(modelPath: string): Promise<UnloadedModel> {
  return invoke<UnloadedModel>("unload_loaded_model", { modelPath });
}

/**
 * Rebuilds the in-memory router from `system.db` so a config change (default
 * switch, edit) takes effect immediately, without freeing the loaded GGUF.
 */
export async function reloadLlmFromDb(): Promise<void> {
  return invoke<void>("reload_llm_from_db");
}
