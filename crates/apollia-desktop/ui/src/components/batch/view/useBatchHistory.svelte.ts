/**
 * The live state of the history of the batches: the batches read so far,
 * newest first, and whether an older one exists.
 *
 * The route takes a count and no offset, so reading on asks for one page
 * more than before; no ceiling is set on how far the user reads.
 */
import type { BatchSummary } from "$lib/ipc/batches";
import { historyLimit, historyPage, type BatchIpc } from "./batchView";
import { shellIpc } from "./useBatch.svelte";

export interface BatchHistory {
  /** The batches read so far, newest first. */
  readonly batches: BatchSummary[];
  readonly loading: boolean;
  /** `true` while an older page is being read. */
  readonly loadingMore: boolean;
  /** Why the history could not be read, `null` when it was. */
  readonly error: string | null;
  /** Whether an older batch exists beyond those read. */
  readonly hasMore: boolean;
  /** Reads again the pages read so far, the first one at least. */
  load(): Promise<void>;
  /** Reads one page more. */
  loadMore(): Promise<void>;
}

export function createBatchHistory(ipc: BatchIpc = shellIpc): BatchHistory {
  let batches = $state<BatchSummary[]>([]);
  let loading = $state(false);
  let loadingMore = $state(false);
  let error = $state<string | null>(null);
  let hasMore = $state(false);
  let pages = 1;
  let request = 0;

  async function read(wanted: number): Promise<void> {
    const mine = ++request;
    try {
      const page = historyPage(await ipc.list(historyLimit(wanted)), wanted);
      if (mine !== request) return;
      batches = page.batches;
      hasMore = page.hasMore;
      pages = wanted;
      error = null;
    } catch (err) {
      if (mine !== request) return;
      error = err instanceof Error ? err.message : String(err);
    }
  }

  async function load(): Promise<void> {
    loading = batches.length === 0;
    await read(pages);
    loading = false;
  }

  async function loadMore(): Promise<void> {
    if (loading || loadingMore || !hasMore) return;
    loadingMore = true;
    await read(pages + 1);
    loadingMore = false;
  }

  return {
    get batches() {
      return batches;
    },
    get loading() {
      return loading;
    },
    get loadingMore() {
      return loadingMore;
    },
    get error() {
      return error;
    },
    get hasMore() {
      return hasMore;
    },
    load,
    loadMore,
  };
}
