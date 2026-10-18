/**
 * Typed Tauri IPC wrappers behind the batches: the sources a batch
 * may read, the preview of a workbook or a CSV file, the sensitive accesses of
 * the agent it runs, the launch, then the view of a batch (its items, its
 * controls, its export, the decision an item waits on) and the history of the
 * batches.
 *
 * Thin wrappers over `crates/apollia-desktop/src/commands/batches.rs` and
 * `batch_view.rs`, each of
 * which calls one runtime route. Error handling stays at the call site.
 */
import { invoke } from "@tauri-apps/api/core";

/** What a source of the registry is. */
export type BatchSourceKind = "folder" | "file" | "mail" | "profile" | "memory" | "code_folder";

/** A source of the registry, as `GET /api/v1/sources` answers it. */
export interface RegistrySource {
  id: string;
  kind: BatchSourceKind;
  /** The name the user sees. */
  label: string;
  /** A folder connection, a file path or a mailbox. */
  reference: string | null;
}

/** One sheet of a previewed file. */
export interface PreviewSheet {
  name: string;
  /** How many of its rows hold at least one non-blank cell. */
  nonEmptyRows: number;
}

/** One column of the table under the header row. */
export interface PreviewColumn {
  /** Column letter, as the spreadsheet shows it; a batch checks a column by it. */
  letter: string;
  /** The name its header cell gives it. */
  name: string;
}

/** One row of a preview. */
export interface PreviewRow {
  /** Row number in the file, from 1. */
  row: number;
  /** The text of each cell, column A first. */
  cells: string[];
}

/** What a batch on a workbook or a CSV file would read, as `POST /api/v1/sources/preview` answers it. */
export interface SourcePreview {
  /** `xlsx`, `xlsm`, `xlsb`, `xls`, `ods` or `csv`. */
  format: string;
  /** The reader of the file: `spreadsheet` or `csv`. */
  sourceType: string;
  /** Every sheet, in the order of the file. */
  sheets: PreviewSheet[];
  proposedSheet: string | null;
  /** The sheet shown: the one asked for, or the proposed one. */
  sheet: string | null;
  proposedHeaderRow: number | null;
  /** The header row used: the one asked for, or the proposed one. */
  headerRow: number | null;
  columns: PreviewColumn[];
  /** The first rows of the shown sheet, from its top. */
  rows: PreviewRow[];
  /** How many items a batch would read. */
  estimatedItems: number;
  canModifyOriginal: boolean;
  /** The copy of results is a new workbook without the original formatting. */
  formattingNotKept: boolean;
}

/** The file to preview, and the sheet and header row the user picked. */
export interface PreviewRequest {
  path: string;
  /** The proposed sheet when absent. */
  sheet?: string;
  /** From 1, on the shown sheet; the proposed row when absent. */
  headerRow?: number;
}

/** One sensitive access of the agent a batch runs. */
export interface BatchAccess {
  /** The access, as the agent names it. */
  accessId: string;
  /** What it lets the agent do. */
  permits: string;
  /** `read_personal`, `write_or_delete`, `send_external`, `execute_code` or `unclassified`. */
  family: string;
  /** Whether it can be granted for the duration of a batch. */
  grantable: boolean;
}

/** Where the results of a batch are written. */
export type WriteDestination = "copy" | "original";

/** What a batch reads, and what it writes back. */
export interface BatchSourceDescriptor {
  /** The reader: `spreadsheet`, `csv` or `mail`. */
  kind: string;
  /** A file path, or the mailbox of a connection. */
  location: string;
  options: Record<string, string>;
  /** The columns added to the file; nothing is written when absent. */
  write?: { destination: WriteDestination; columns: string[] };
}

/** The body of a launch. */
export interface BatchLaunch {
  agent: string;
  source: BatchSourceDescriptor;
  /** The accesses granted for the duration of the batch. */
  grants: string[];
  /** Failed items tolerated before the batch is interrupted. */
  maxFailures: number;
}

/** A launched batch, as the runtime answers it. */
export interface LaunchedBatch {
  batchId: string;
  agent: string;
  state: string;
  /** How many items the batch holds. */
  total: number;
  maxFailures: number;
}

/** Every source of the registry, oldest first. */
export function listBatchSources(): Promise<RegistrySource[]> {
  return invoke<RegistrySource[]>("list_batch_sources");
}

/**
 * What a batch on the file of `request` would read. A file the runtime cannot
 * read rejects with its reason; an answer that is not a preview reads as none.
 */
export async function previewBatchSource(request: PreviewRequest): Promise<SourcePreview | null> {
  const preview = await invoke<SourcePreview | null>("preview_batch_source", {
    path: request.path,
    sheet: request.sheet ?? null,
    headerRow: request.headerRow ?? null,
  });
  return preview && Array.isArray(preview.columns) && Array.isArray(preview.rows) ? preview : null;
}

/** The sensitive accesses of `agent`, those a launch may grant. */
export function listBatchAccesses(agent: string): Promise<BatchAccess[]> {
  return invoke<BatchAccess[]>("list_batch_accesses", { agent });
}

/** Launches a batch; a launch the runtime refuses rejects with its reason. */
export function launchBatch(launch: BatchLaunch): Promise<LaunchedBatch> {
  return invoke<LaunchedBatch>("launch_batch", { launch });
}

/** A batch: what it was launched with, its state and its counters. */
export interface BatchSummary {
  batchId: string;
  agent: string;
  source: BatchSourceDescriptor;
  /** The accesses granted for the duration of the batch. */
  grants: string[];
  concurrency: number;
  /** Failed items tolerated before the batch is interrupted. */
  maxFailures: number;
  /** `running`, `paused`, `completed`, `interrupted` or `stopped`. */
  state: string;
  total: number;
  /** Items that succeeded. */
  completed: number;
  failed: number;
  /** Items never run because the batch was stopped or interrupted. */
  canceled: number;
  /** Items waiting for an access to be granted. */
  awaiting: number;
  createdAt: string;
  updatedAt: string;
  endedAt: string | null;
}

/** One item of a batch. */
export interface BatchItem {
  /** Its position in the source, from 0. */
  rank: number;
  /** Where it comes from inside the source. */
  origin: string;
  /** Its fields as read from the source. */
  input: unknown;
  /** `pending`, `running`, `completed`, `failed`, `canceled` or `awaiting`. */
  status: string;
  /** The answer, or the cause of the failure. */
  output: string | null;
  /** The sources the answer cites. */
  sources: unknown;
  /** The run behind it, when it ran. */
  taskId: string | null;
  /** When it last changed state, UTC: for a running item, when its run started. */
  updatedAt?: string;
  /** How long its last run took, once it ended. */
  durationMs?: number | null;
}

/** A batch with its items. */
export interface BatchDetail {
  batch: BatchSummary;
  items: BatchItem[];
}

/** What a batch can be told to do. */
export type BatchControl = "pause" | "resume" | "stop" | "retry";

/** The formats a batch is exported in. */
export type BatchExportFormat = "xlsx" | "csv" | "json";

/** An answer the decision of a waiting item takes. */
export type BatchItemAnswer = "once" | "always" | "refuse";

/** The decision an item waits on. */
export interface BatchItemDecision {
  /** Identifier to answer it by. */
  id: string;
  /** What is asked, in a sentence. */
  summary: string;
  /** The access asked for. */
  access: string | null;
  /** The answers it takes. */
  answers: string[];
}

/** What an answer to a waiting item did. */
export interface BatchItemDecided {
  id: string;
  /** How many waiting items went on. */
  resumed: number;
  message: string;
}

/** The `limit` most recent batches, newest first. */
export function listBatches(limit: number): Promise<BatchSummary[]> {
  return invoke<BatchSummary[]>("list_batches", { limit });
}

/** A batch with its items, `null` when no batch bears that id. */
export function getBatch(batchId: string): Promise<BatchDetail | null> {
  return invoke<BatchDetail | null>("get_batch", { batchId });
}

/** Pauses, resumes, stops a batch or runs its failed items again; a refusal rejects with its reason. */
export function controlBatch(batchId: string, action: BatchControl): Promise<BatchSummary> {
  return invoke<BatchSummary>("control_batch", { batchId, action });
}

/** The file an export wrote. */
export interface BatchExported {
  path: string;
  bytes: number;
}

/** Writes the result of a batch to `path` in `format`. */
export function exportBatch(batchId: string, format: BatchExportFormat, path: string): Promise<BatchExported> {
  return invoke<BatchExported>("export_batch", { batchId, format, path });
}

/** A batch deleted, and how many items went with it. */
export interface BatchDeleted {
  batchId: string;
  items: number;
}

/**
 * Deletes a batch and its items; an active batch is refused with its reason.
 * Files the batch was exported to are not touched.
 */
export function deleteBatch(batchId: string): Promise<BatchDeleted> {
  return invoke<BatchDeleted>("delete_batch", { batchId });
}

/** The decision the item `rank` of a batch waits on, `null` when it waits on none. */
export function batchItemDecision(batchId: string, rank: number): Promise<BatchItemDecision | null> {
  return invoke<BatchItemDecision | null>("batch_item_decision", { batchId, rank });
}

/** Answers the decision of a waiting item. */
export function decideBatchItem(id: string, answer: BatchItemAnswer): Promise<BatchItemDecided> {
  return invoke<BatchItemDecided>("decide_batch_item", { id, answer });
}
