/**
 * Which batch the view of a batch shows, and where it was opened from.
 *
 * A launch and a row of the history both open the view through `openBatch`;
 * the view reads `openedBatch` and offers the way back its origin calls for.
 */
import { get, writable } from "svelte/store";
import { navigateTo } from "$lib/stores/navigation";

/** Where the view of a batch was opened from. */
export type BatchOrigin = "launch" | "history";

export interface OpenedBatch {
  batchId: string;
  origin: BatchOrigin;
}

export const openedBatch = writable<OpenedBatch | null>(null);

/** Opens the view of the batch `batchId`. */
export function openBatch(batchId: string, origin: BatchOrigin): void {
  openedBatch.set({ batchId, origin });
  navigateTo("batch");
}

/** Opens the history of the batches. */
export function openBatchHistory(): void {
  navigateTo("batches");
}

/** The batch the view shows, `null` when none was opened. */
export function currentBatch(): OpenedBatch | null {
  return get(openedBatch);
}
