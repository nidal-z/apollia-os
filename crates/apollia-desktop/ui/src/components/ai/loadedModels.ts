/**
 * What the "Loaded" section of the AI screen can say about a resident model.
 *
 * Pure and i18n-agnostic: labels are catalogue keys resolved by the caller.
 */
import type { LoadedModel, LoadedModelsResponse } from "$lib/types";

/** The block shows only where a local engine exists on this host. */
export function showLoadedModels(view: LoadedModelsResponse | null): boolean {
  return view !== null && view.engineAvailable;
}

/** A model answering a request cannot be unloaded; the runtime refuses it. */
export function canUnload(model: LoadedModel): boolean {
  return model.activeRequests === 0;
}

/**
 * Catalogue key for a rejected unload, when the refusal is one the block can
 * phrase itself. The runtime refuses a model that started answering between the
 * list and the click with a message naming it "in use".
 */
export function unloadErrorKey(message: string): string | null {
  return /\bin use\b/i.test(message) ? "ai.loaded.in_use_error" : null;
}
