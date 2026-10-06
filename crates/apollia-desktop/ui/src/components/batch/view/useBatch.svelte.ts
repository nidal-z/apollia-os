/**
 * The live state of the view of a batch: the batch and its items as last
 * read, the command in flight, and why the last one was refused.
 *
 * The view asks for a reading whenever the runtime reports a change and at a
 * slow pace besides; a reading that comes back after a newer one was asked
 * for is dropped.
 */
import {
  batchItemDecision,
  controlBatch,
  decideBatchItem,
  exportBatch,
  getBatch,
  listBatches,
} from "$lib/ipc/batches";
import type { BatchControl, BatchDetail, BatchExportFormat } from "$lib/ipc/batches";
import type { BatchIpc } from "./batchView";

export const shellIpc: BatchIpc = {
  list: listBatches,
  get: getBatch,
  control: controlBatch,
  exportTo: exportBatch,
  decision: batchItemDecision,
  decide: decideBatchItem,
};

function reason(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

export interface BatchView {
  /** The batch and its items as last read, `null` before the first reading. */
  readonly detail: BatchDetail | null;
  /** `true` until the first reading comes back. */
  readonly loading: boolean;
  /** Why the batch could not be read, `null` when it was. */
  readonly error: string | null;
  /** No batch bears the id asked for. */
  readonly missing: boolean;
  /** The command being applied, `null` when none is. */
  readonly busy: BatchControl | "export" | null;
  /** Why the last command was refused, `null` when it went through. */
  readonly refusal: string | null;
  /** Reads the batch `batchId`, replacing what was shown when the id changes. */
  load(batchId: string): Promise<void>;
  /** Reads again the batch shown, keeping it on screen meanwhile. */
  refresh(): Promise<void>;
  /** Applies `action`, then reads the batch again; `true` when it went through. */
  act(action: BatchControl): Promise<boolean>;
  /** Writes the batch to `path` in `format`; the file written, or `null` when refused. */
  exportTo(format: BatchExportFormat, path: string): Promise<string | null>;
  clearRefusal(): void;
}

export function createBatchView(ipc: BatchIpc = shellIpc): BatchView {
  let detail = $state<BatchDetail | null>(null);
  let loading = $state(false);
  let error = $state<string | null>(null);
  let missing = $state(false);
  let busy = $state<BatchControl | "export" | null>(null);
  let refusal = $state<string | null>(null);
  let shown: string | null = null;
  let request = 0;

  async function read(batchId: string): Promise<void> {
    const mine = ++request;
    try {
      const read = await ipc.get(batchId);
      if (mine !== request) return;
      detail = read;
      missing = read === null;
      error = null;
    } catch (err) {
      if (mine !== request) return;
      // A reading that fails once a batch is on screen keeps it there.
      if (detail === null) error = reason(err);
    } finally {
      if (mine === request) loading = false;
    }
  }

  async function load(batchId: string): Promise<void> {
    if (shown !== batchId) {
      shown = batchId;
      detail = null;
      missing = false;
      error = null;
      refusal = null;
      loading = true;
    }
    await read(batchId);
  }

  async function refresh(): Promise<void> {
    if (shown !== null) await read(shown);
  }

  async function act(action: BatchControl): Promise<boolean> {
    if (shown === null || busy !== null) return false;
    busy = action;
    refusal = null;
    try {
      const batch = await ipc.control(shown, action);
      if (detail !== null && detail.batch.batchId === batch.batchId) detail = { ...detail, batch };
      await refresh();
      return true;
    } catch (err) {
      refusal = reason(err);
      return false;
    } finally {
      busy = null;
    }
  }

  async function exportTo(format: BatchExportFormat, path: string): Promise<string | null> {
    if (shown === null || busy !== null) return null;
    busy = "export";
    refusal = null;
    try {
      return (await ipc.exportTo(shown, format, path)).path;
    } catch (err) {
      refusal = reason(err);
      return null;
    } finally {
      busy = null;
    }
  }

  return {
    get detail() {
      return detail;
    },
    get loading() {
      return loading;
    },
    get error() {
      return error;
    },
    get missing() {
      return missing;
    },
    get busy() {
      return busy;
    },
    get refusal() {
      return refusal;
    },
    load,
    refresh,
    act,
    exportTo,
    clearRefusal() {
      refusal = null;
    },
  };
}
