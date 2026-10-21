/**
 * Typed Tauri IPC wrappers behind the journal screen.
 *
 * Thin wrappers over the commands of
 * `crates/apollia-desktop/src/commands/journal.rs`, each of which reads one
 * runtime route: a page of the chained journal narrowed by its filters, the
 * detail of one entry, the verification of the whole chain and the export.
 */
import { invoke } from "@tauri-apps/api/core";

/** What kind of fact an entry records. */
export type JournalNature = "action" | "access_change" | "policy" | "system";

/** How what an entry records ended. */
export type JournalOutcome = "success" | "failure" | "refused";

/** One entry said in a line, without content. */
export interface JournalSummary {
  /** Agent the entry is about, when it is about one. */
  agent: string | null;
  nature: JournalNature;
  /** Action family under its wire name (`read`, `send_external`, ...). */
  family: string | null;
  /** What happened, in a few English words. */
  what: string;
  tool: string | null;
  /** What was acted on; never an argument that carries content. */
  target: string | null;
  /** `null` for an entry that records no ending, such as the start of a call. */
  outcome: JournalOutcome | null;
  /** RFC 3339 UTC. */
  ts: string;
}

/**
 * The kind of an entry: its stable tag (`tool_call_completed`,
 * `access_changed`, ...), or the raw name of an event the journal has no
 * mapping for.
 */
export type JournalKind = string | { unknown: { raw_kind: string } };

/** A chained journal entry with its summary. */
export interface JournalLine {
  /** Position of the entry within its run. */
  seq: number;
  run_id: string;
  /** RFC 3339 UTC. */
  ts: string;
  kind: JournalKind;
  payload: Record<string, unknown>;
  prev_hash: string;
  hash: string;
  signature?: string | null;
  signing_key_id?: string | null;
  summary: JournalSummary;
}

/** The arguments and the output of one tool call. */
export interface JournalToolCall {
  tool_call_id: string;
  /** `null` when no model response of the run carries the call. */
  arguments: unknown;
  /** `null` when the run captured no output. */
  output: unknown;
  /** `success`, `error` or `rejected`, as the capture wrote it. */
  status: string | null;
}

/** An entry with everything it records. */
export interface JournalEntryDetail extends JournalLine {
  /** Arguments and output, when the entry is about a tool call. */
  tool_call: JournalToolCall | null;
}

/**
 * What a page is narrowed to. Within one criterion the values are
 * alternatives; the criteria add up. The route applies them before paging.
 */
export interface JournalQuery {
  agents?: string[];
  families?: string[];
  natures?: JournalNature[];
  results?: JournalOutcome[];
  /** Prefixes of tool names, such as `mcp:notion/` for one connection. */
  tool_prefixes?: string[];
  /** Start of the period, included: a day or an RFC 3339 instant. */
  since?: string;
  /** End of the period, excluded: a day or an RFC 3339 instant. */
  until?: string;
  limit?: number;
  /** Matching entries to skip, newest first. */
  offset?: number;
}

/** A page of the journal narrowed by `query`, newest first. */
export async function listJournalEntries(query: JournalQuery): Promise<JournalLine[]> {
  return invoke<JournalLine[]>("list_journal_entries", { query });
}

/** The entry `seq` of `runId` in full, `null` when the journal holds none. */
export async function getJournalEntry(
  runId: string,
  seq: number,
): Promise<JournalEntryDetail | null> {
  return invoke<JournalEntryDetail | null>("get_journal_entry", { runId, seq });
}

/** Why the verification stopped on an entry, under the name the route gives it. */
export type JournalBreakReason =
  | "global_seq_gap"
  | "global_prev_hash_mismatch"
  | "global_hash_mismatch"
  | "global_signature_invalid"
  | "unknown_signing_key"
  | "per_run_broken";

/** What the verification of the whole journal found. */
export interface VerifyJournalReport {
  /** `true` when the chain, every run and the recorded end all hold. */
  ok: boolean;
  /** Entries read. */
  entries_checked: number;
  runs_checked: number;
  /** The first broken link found, `null` when none was. */
  first_break: {
    /** Position of the entry in the whole journal, counted from zero. */
    global_seq: number;
    run_id: string;
    reason: JournalBreakReason | string;
  } | null;
  /** Whether the last entry is the one the journal recorded as its end. */
  head_matches_state: boolean;
  /** Where the verification started when a purge removed the beginning. */
  start?: { global_seq: number; ts: string; purged_at: string } | null;
}

/** One entry of an export: its place in the chain and its summary. */
export interface ExportedJournalEntry extends JournalSummary {
  run_id: string;
  seq: number;
  /** Kind of the entry, under its stable tag. */
  kind: string;
  hash: string;
  /** What the detail adds, in an export asked with it. */
  detail?: {
    payload: Record<string, unknown>;
    tool_call: JournalToolCall | null;
  };
}

/** The export of the journal as a document. */
export interface JournalExportDocument {
  /** Whether each entry holds its detail. */
  detail: boolean;
  count: number;
  /** Newest first. */
  entries: ExportedJournalEntry[];
}

/** The file an export was written to. */
export interface JournalExported {
  path: string;
  bytes: number;
}

/** Verifies the whole journal, every run together. */
export async function verifyJournal(): Promise<VerifyJournalReport> {
  return invoke<VerifyJournalReport>("verify_journal");
}

/**
 * Writes every entry `query` narrows to `path` as a CSV file, with the detail
 * of each when `detail` is set. Rejects with the reason of the runtime when it
 * refuses, and then writes nothing.
 */
export async function exportJournal(
  query: JournalQuery,
  detail: boolean,
  path: string,
): Promise<JournalExported> {
  return invoke<JournalExported>("export_journal", { query, detail, path });
}

/** The same export as a document, which the screen lays out as a page to print. */
export async function readJournalExport(
  query: JournalQuery,
  detail: boolean,
): Promise<JournalExportDocument> {
  return invoke<JournalExportDocument>("read_journal_export", { query, detail });
}

/** Opens the print dialog of the system on this window. */
export async function printJournal(): Promise<void> {
  return invoke<void>("print_journal");
}
