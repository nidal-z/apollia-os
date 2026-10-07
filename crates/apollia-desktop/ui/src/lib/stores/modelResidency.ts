/**
 * What the chat knows of the local models in memory: the resident models,
 * the last one loaded by hand, the pinned ones, and the model the loading
 * window is open on.
 *
 * Shared because the selector, the eject button, the reload line and the
 * loading window read the same state and each changes it.
 */
import { writable } from "svelte/store";
import { listLoadedModels } from "$lib/ipc/llm";
import { getLastLoadedModel, listModelPins } from "$lib/ipc/llmLoad";
import type { LoadForm } from "$lib/chat/modelLoad";
import type { ModelFormat } from "$lib/chat/chatTargets";
import type { LastLoadedModel, LoadedModelsResponse } from "$lib/types";

/** The resident models, `null` until read once. */
export const loadedModels = writable<LoadedModelsResponse | null>(null);
/** The last model loaded by hand, `null` when none. */
export const lastLoadedModel = writable<LastLoadedModel | null>(null);
/** The paths of the models kept in memory. */
export const modelPins = writable<string[]>([]);

/** Reads the three again; a read that fails leaves what was known. */
export async function refreshModelResidency(): Promise<void> {
  await Promise.all([
    listLoadedModels().then(
      (view) => loadedModels.set(view),
      () => undefined,
    ),
    getLastLoadedModel().then(
      (last) => lastLoadedModel.set(last),
      () => undefined,
    ),
    listModelPins().then(
      (view) => modelPins.set(view.pins),
      () => undefined,
    ),
  ]);
}

/** What the loading window is asked to open on. */
export interface ModelLoadAsk {
  /** Identifier of the model in the model index. */
  modelId: string;
  /** Storage format, shown as a badge; `null` when not known yet. */
  format: ModelFormat | null;
  /** Settings the window opens with, e.g. those of the last load. */
  settings?: LoadForm;
  /** Why the window opened in place of a load, shown above its buttons. */
  notice?: string;
  /** Called once the model is loaded. */
  onloaded?: () => void;
}

/** The model the loading window is open on, `null` when it is closed. */
export const modelLoadAsk = writable<ModelLoadAsk | null>(null);

/** Opens the loading window. */
export function askModelLoad(ask: ModelLoadAsk): void {
  modelLoadAsk.set(ask);
}

/** Requests to reload the last model, bumped by the chat shortcut. */
export const reloadLastRequests = writable(0);

/** Ask the reload line on screen to reload the last model. */
export function requestReloadLast(): void {
  reloadLastRequests.update((n) => n + 1);
}
