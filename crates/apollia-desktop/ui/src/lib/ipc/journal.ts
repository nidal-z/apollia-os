/**
 * Typed Tauri IPC wrappers behind the journal screen.
 *
 * Thin wrappers over the commands of
 * `crates/apollia-desktop/src/commands/journal.rs`, each of which reads one
 * runtime route: a page of the chained journal narrowed by its filters, and
 * the detail of one entry.
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
