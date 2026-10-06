/**
 * Typed Tauri IPC wrappers behind the launch of a batch: the sources a batch
 * may read, the preview of a workbook or a CSV file, the sensitive accesses of
 * the agent it runs, and the launch.
 *
 * Thin wrappers over `crates/apollia-desktop/src/commands/batches.rs`, each of
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
