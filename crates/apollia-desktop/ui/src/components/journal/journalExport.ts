/**
 * The decisions of the export of the journal: what the organisation policy
 * settles about the detail, which query the export carries, what the file is
 * called, and what the page to print shows of each entry.
 */
import type {
  ExportedJournalEntry,
  JournalExportDocument,
  JournalQuery,
} from "$lib/ipc/journal";
import type { PolicyView } from "$lib/ipc/policy";
import {
  NATURE_LABEL,
  PERIOD_LABEL,
  RESULT_LABEL,
  familyLabelKey,
  queryOf,
  sentenceOf,
  type JournalFilters,
  type Sentence,
} from "./journal";

export type ExportFormat = "csv" | "pdf";

/** Who decides whether the detail goes with an export. */
export type DetailRule =
  /** The user, with the box. */
  | "free"
  /** The policy requires it: the box is checked and frozen. */
  | "required"
  /** The policy forbids it: the box is not shown. */
  | "forbidden";

/**
 * What the policy in force settles about the detail. A policy that is absent,
 * refused or unread settles nothing here: the runtime still refuses an export
 * its policy does not allow.
 */
export function detailRule(policy: PolicyView | null): DetailRule {
  if (policy === null || policy.state !== "active") return "free";
  const settled = policy.policy.journal.export.detail;
  if (settled === "require") return "required";
  if (settled === "forbid") return "forbidden";
  return "free";
}

/** Whether the export carries the detail, given what the user ticked. */
export function detailFor(rule: DetailRule, wanted: boolean): boolean {
  if (rule === "required") return true;
  if (rule === "forbidden") return false;
  return wanted;
}

/**
 * The query of an export for the filters of the screen: the same criteria as
 * the list, without the bounds of a page, since an export holds every entry
 * that matches.
 */
export function exportQueryOf(filters: JournalFilters, developerMode: boolean, now: Date): JournalQuery {
  const query = queryOf(filters, developerMode, now, 0);
  delete query.limit;
  delete query.offset;
  return query;
}

/** The name proposed for the file, dated on the clock of this machine. */
export function exportFileName(now: Date): string {
  const two = (value: number): string => String(value).padStart(2, "0");
  return `journal-${now.getFullYear()}-${two(now.getMonth() + 1)}-${two(now.getDate())}.csv`;
}

/** One filter in force, as the export recalls it: a label to translate, or a name. */
export type FilterMention = { key: string } | { text: string };

/** The filters an export takes over, the period always among them. */
export function filterMentions(
  filters: JournalFilters,
  agentName: (agent: string | null) => string,
): FilterMention[] {
  const mentions: FilterMention[] = [];
  if (filters.connection !== null) mentions.push({ text: filters.connection.label });
  if (filters.agent !== "") mentions.push({ text: agentName(filters.agent) });
  if (filters.family !== "") {
    const key = familyLabelKey(filters.family);
    mentions.push(key === null ? { text: filters.family } : { key });
  }
  if (filters.nature !== "") mentions.push({ key: NATURE_LABEL[filters.nature] });
  if (filters.result !== "") mentions.push({ key: RESULT_LABEL[filters.result] });
  mentions.push({ key: PERIOD_LABEL[filters.period] });
  return mentions;
}

// ── The page to print ──────────────────────────────────────────────────

/** A cell holding a label to translate, a text as written, or nothing. */
export type PrintCell = { key: string } | { text: string } | null;

/** What the detail of an entry adds under its row, each value as JSON text. */
export interface PrintDetail {
  payload: string;
  /** `null` when the entry is about no tool call, or the journal kept none. */
  arguments: string | null;
  output: string | null;
  status: string | null;
}

/** One entry of the page to print. */
export interface PrintRow {
  /** Stable key of the row. */
  id: string;
  /** RFC 3339 UTC, as the journal wrote it. */
  ts: string;
  agent: string | null;
  nature: PrintCell;
  family: PrintCell;
  /** What happened: the sentence of the screen, or the words of the journal. */
  what: Sentence | { text: string };
  tool: string | null;
  target: string | null;
  outcome: PrintCell;
  runId: string;
  seq: number;
  hash: string;
  detail: PrintDetail | null;
}

/** Kinds whose sentence depends on what the payload says. */
const PAYLOAD_SENTENCES = new Set(["access_changed", "consent_answered"]);

function jsonText(value: unknown): string | null {
  return value === null || value === undefined ? null : JSON.stringify(value, null, 2);
}

function whatOf(
  entry: ExportedJournalEntry,
  agentName: (agent: string | null) => string,
): Sentence | { text: string } {
  const payload = entry.detail?.payload;
  if (payload === undefined && PAYLOAD_SENTENCES.has(entry.kind)) return { text: entry.what };
  const sentence = sentenceOf({ kind: entry.kind, payload: payload ?? {}, summary: entry }, agentName);
  // An entry the screens have no sentence for keeps the words of the journal.
  return sentence.key.startsWith("dev.") ? { text: entry.what } : sentence;
}

function rowOf(
  entry: ExportedJournalEntry,
  agentName: (agent: string | null) => string,
): PrintRow {
  const familyKey = familyLabelKey(entry.family);
  const detail = entry.detail;
  return {
    id: `${entry.run_id}#${entry.seq}`,
    ts: entry.ts,
    agent: entry.agent === null ? null : agentName(entry.agent),
    nature: { key: NATURE_LABEL[entry.nature] },
    family: entry.family === null ? null : familyKey === null ? { text: entry.family } : { key: familyKey },
    what: whatOf(entry, agentName),
    tool: entry.tool,
    target: entry.target,
    outcome: entry.outcome === null ? null : { key: RESULT_LABEL[entry.outcome] },
    runId: entry.run_id,
    seq: entry.seq,
    hash: entry.hash,
    detail:
      detail === undefined
        ? null
        : {
            payload: JSON.stringify(detail.payload, null, 2),
            arguments: jsonText(detail.tool_call?.arguments),
            output: jsonText(detail.tool_call?.output),
            status: detail.tool_call?.status ?? null,
          },
  };
}

/** The rows of the page to print, in the order of the export: newest first. */
export function printRows(
  document: JournalExportDocument,
  agentName: (agent: string | null) => string,
): PrintRow[] {
  return document.entries.map((entry) => rowOf(entry, agentName));
}
