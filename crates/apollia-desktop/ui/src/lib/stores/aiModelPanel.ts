/**
 * The installed model whose panel the detail sidebar shows.
 *
 * The AI screen writes it when a row is opened and clears it when it is left;
 * the detail sidebar reads it to offer its "Model" tab. `null` means no model
 * is open, and the tab is not offered.
 */
import { writable } from "svelte/store";
import type { InstalledModel } from "$lib/ipc/modelHub";

export interface AiModelSelection {
  model: InstalledModel;
  /** Whether the model is resident in memory. */
  loaded: boolean;
}

export const aiModelSelection = writable<AiModelSelection | null>(null);

/**
 * Bumped by the panel when it changed what the screen lists (a model loaded
 * or unloaded, its type or its source file overridden): the screen reads its
 * lists again.
 */
export const aiModelsChanged = writable(0);

/** Tells the AI screen that its lists are stale. */
export function notifyAiModelsChanged(): void {
  aiModelsChanged.update((n) => n + 1);
}
