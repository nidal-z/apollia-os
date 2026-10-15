/**
 * Whether a batch can be deleted, and why not when it cannot.
 *
 * The runtime refuses to delete an active batch: running or paused, or with
 * an item still running after it was stopped or interrupted. The screens
 * disable the command on the same grounds, so the reason is shown before the
 * user asks. A deletion removes the batch and its items from the history
 * only: the files it was exported or written to stay as they are.
 */
import type { BatchItem, BatchSummary } from "$lib/ipc/batches";
import { stateOf } from "./batchView";

/** Why a batch cannot be deleted now. */
export type DeleteBlock = "running" | "paused" | "in_flight";

/**
 * Why `batch` cannot be deleted, `null` when it can. `items`, when the
 * screen has read them, tells an ended batch whose last item still runs.
 */
export function deleteBlockOf(batch: BatchSummary, items: readonly BatchItem[] = []): DeleteBlock | null {
  const state = stateOf(batch);
  if (state === "running" || state === "paused") return state;
  if (items.some((item) => item.status === "running")) return "in_flight";
  return null;
}
