// Typed IPC wrappers specific to the LLM backend add/edit dialog.
//
// The shared `$lib/ipc/llm.ts` covers the list/ping/default/reload/delete
// surface reused by both LLM pages; it is read-only here. The create/update
// mutations and the HuggingFace generation-config lookup are only needed by
// `LlmBackendDialog`, so they live next to it instead of duplicating the
// shared module. GGUF scanning reuses `$lib/ipc/models.ts`.

import { invoke } from "@tauri-apps/api/core";
import type { LlmBackendConfig } from "$lib/types";

/** Mutable fields shared by the create and update payloads. */
export interface LlmBackendPayload {
  provider: LlmBackendConfig["provider"];
  model: string;
  config_json: Record<string, unknown>;
  enabled: boolean;
  is_default: boolean;
}

/** Creates a new backend row. `name` is immutable once created. */
export async function createLlmBackend(
  payload: LlmBackendPayload & { name: string },
): Promise<void> {
  return invoke<void>("create_llm_backend", { payload });
}

/** Updates the mutable fields of an existing backend, keyed by `name`. */
export async function updateLlmBackend(
  name: string,
  payload: LlmBackendPayload,
): Promise<void> {
  return invoke<void>("update_llm_backend", { name, payload });
}

/** Sampling defaults exposed by a HuggingFace model card. */
export interface HfGenerationConfig {
  temperature?: number;
  top_p?: number;
  top_k?: number;
  repetition_penalty?: number;
  max_new_tokens?: number;
}

/** The subset of a HuggingFace model card the dialog reads for auto-fill. */
export interface HfModelParams {
  generation_config: HfGenerationConfig | null;
}

/** Fetches a model card and returns its generation defaults, if published. */
export async function getHfModelParams(repoId: string): Promise<HfModelParams> {
  return invoke<HfModelParams>("get_hf_model", { repoId });
}
