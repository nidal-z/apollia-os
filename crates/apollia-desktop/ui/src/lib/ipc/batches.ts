/**
 * Typed Tauri IPC wrappers behind the batches: the sources a batch
 * may read, the sensitive accesses of the agent it runs, the launch, then the
 * view of a batch (its items, its controls, its export, the decision an item
 * waits on) and the history of the batches.
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

/** The decision the item `rank` of a batch waits on, `null` when it waits on none. */
export function batchItemDecision(batchId: string, rank: number): Promise<BatchItemDecision | null> {
  return invoke<BatchItemDecision | null>("batch_item_decision", { batchId, rank });
}

/** Answers the decision of a waiting item. */
export function decideBatchItem(id: string, answer: BatchItemAnswer): Promise<BatchItemDecided> {
  return invoke<BatchItemDecided>("decide_batch_item", { id, answer });
}
