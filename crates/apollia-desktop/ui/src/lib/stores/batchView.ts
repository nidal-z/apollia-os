/**
 * Which batch the view of a batch shows, where it was opened from, and the
 * item whose detail the detail sidebar shows.
 *
 * A launch and a row of the history both open the view through `openBatch`;
 * the view reads `openedBatch` and offers the way back its origin calls for.
 */
import { get, writable } from "svelte/store";
import type { BatchItem, BatchSummary } from "$lib/ipc/batches";
import type { TaskTimelineEvent } from "$lib/ipc/tasks";
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

/** Where the steps of the run behind an item stand. */
export type ItemStepsState =
  | { status: "none" }
  | { status: "loading" }
  | { status: "ready"; steps: TaskTimelineEvent[] }
  | { status: "error" };

export interface BatchItemSelection {
  batch: BatchSummary;
  item: BatchItem;
  steps: ItemStepsState;
}

/** The item the "Item" tab of the detail sidebar shows; `null` hides the tab. */
export const batchItemSelection = writable<BatchItemSelection | null>(null);
