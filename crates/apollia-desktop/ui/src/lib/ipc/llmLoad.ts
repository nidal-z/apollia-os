// Typed IPC wrappers for loading a local model by hand and for its
// residency: the estimate and the load of the loading window, the last
// model loaded, the lease an open conversation holds on its model, and the
// pin that keeps a model in memory.

import { invoke } from "@tauri-apps/api/core";
import type {
  LastLoadedModel,
  LoadEstimate,
  ModelLease,
  ModelLeaseRelease,
  ModelLoadOutcome,
  ModelLoadRequest,
  ModelPin,
  ModelPins,
} from "$lib/types";

/** Says what a load would take, and what the load guardrail makes of it. */
export async function estimateModelLoad(request: ModelLoadRequest): Promise<LoadEstimate> {
  return invoke<LoadEstimate>("estimate_model_load", { request });
}

/**
 * Loads a model under the settings asked. Rejects with the runtime's message
 * when the load guardrail refuses it and the request is not confirmed.
 */
export async function loadModel(request: ModelLoadRequest): Promise<ModelLoadOutcome> {
  return invoke<ModelLoadOutcome>("load_model", { request });
}

/** The last model loaded by hand, `null` when none was. */
export async function getLastLoadedModel(): Promise<LastLoadedModel | null> {
  return invoke<LastLoadedModel | null>("get_last_loaded_model");
}

/** Loads the last model loaded by hand again, under the settings it had. */
export async function reloadLastLoadedModel(confirmed: boolean): Promise<ModelLoadOutcome> {
  return invoke<ModelLoadOutcome>("reload_last_loaded_model", { confirmed });
}

/** Opens the lease of a conversation on the local model it talks to. */
export async function openModelLease(
  conversationId: string,
  modelPath: string,
): Promise<ModelLease> {
  return invoke<ModelLease>("open_model_lease", { conversationId, modelPath });
}

/** Releases the lease of a conversation. */
export async function releaseModelLease(conversationId: string): Promise<ModelLeaseRelease> {
  return invoke<ModelLeaseRelease>("release_model_lease", { conversationId });
}

/** Lists the paths of the models kept in memory. */
export async function listModelPins(): Promise<ModelPins> {
  return invoke<ModelPins>("list_model_pins");
}

/** Keeps a model in memory, or stops keeping it. */
export async function setModelPin(modelPath: string, pinned: boolean): Promise<ModelPin> {
  return invoke<ModelPin>("set_model_pin", { modelPath, pinned });
}
