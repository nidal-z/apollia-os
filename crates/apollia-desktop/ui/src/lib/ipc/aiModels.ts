/**
 * Typed IPC wrappers of the AI screen: the default model of the machine, the
 * embeddings model of search, the pin of an installed model, and showing a
 * model in the file manager.
 */
import { invoke } from "@tauri-apps/api/core";

/** The default model of the machine, as the runtime answers it. */
export interface DefaultModel {
  backend: string;
  model_source: "local" | "server";
  model_id: string;
}

/** The embeddings model search uses, as the runtime answers it. */
export interface EmbeddingsDefault {
  model: string | null;
  installed: boolean;
  path: string | null;
}

/** Makes an installed language model, named by its index identifier, the default. */
export function setDefaultModel(modelId: string): Promise<DefaultModel> {
  return invoke<DefaultModel>("set_default_model", { modelId });
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
