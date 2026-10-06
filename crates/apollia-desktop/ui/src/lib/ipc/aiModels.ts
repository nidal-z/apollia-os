/**
 * Typed IPC wrappers of the AI screen: the default model of the machine, the
 * embeddings model of search, the pin of an installed model, and showing a
 * model in the file manager.
 */
import { invoke } from "@tauri-apps/api/core";
import { setDefaultModel as setDefaultModelOf, type DefaultModel } from "$lib/ipc/llmServers";

/** The embeddings model search uses, as the runtime answers it. */
export interface EmbeddingsDefault {
  model: string | null;
  installed: boolean;
  path: string | null;
}

/** Makes an installed language model, named by its index identifier, the default. */
export function setDefaultModel(modelId: string): Promise<DefaultModel> {
  return setDefaultModelOf("local", modelId);
}

/** Makes a registry entry the embeddings model search uses; sources and memory are indexed again. */
export function setEmbeddingsDefault(entry: string): Promise<EmbeddingsDefault> {
  return invoke<EmbeddingsDefault>("set_embeddings_default", { entry });
}

/** Pins an installed model to the top of its category, or removes the pin. */
export function setInstalledModelPinned(id: string, pinned: boolean): Promise<void> {
  return invoke<void>("set_installed_model_pinned", { id, pinned });
}

/** Shows a model, or the models folder, in the file manager of this machine. */
export function revealInstalledModel(path: string): Promise<void> {
  return invoke<void>("reveal_installed_model", { path });
}
