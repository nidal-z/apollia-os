/**
 * Typed Tauri IPC wrappers behind the launch of a batch: the sources a batch
 * may read, the sensitive accesses of the agent it runs, and the launch.
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
