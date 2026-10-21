/**
 * The decisions of the journal screen: what a line says, which state it
 * carries, how the filters become the query of the route, how the entries are
 * grouped by day, and what the detail of an entry shows.
 *
 * Nothing here filters entries: the filters travel to the route, which applies
 * them before paging.
 */
import type {
  JournalEntryDetail,
  JournalKind,
  JournalLine,
  JournalNature,
  JournalOutcome,
  JournalQuery,
} from "$lib/ipc/journal";
import type { JournalConnectionFocus } from "$lib/stores/journalFocus";
import type { JournalDetailState } from "$lib/stores/journalSelection";

/** Entries asked for in one page. */
export const PAGE_SIZE = 50;

/** The two shell calls the screen makes, replaceable in tests. */
export interface JournalIpc {
  list: (query: JournalQuery) => Promise<JournalLine[]>;
  entry: (runId: string, seq: number) => Promise<JournalEntryDetail | null>;
}

// ── Filters ────────────────────────────────────────────────────────────

export type JournalPeriod = "today" | "7d" | "30d" | "all";

/**
 * The filters of the screen: the five of its bar, and the connection another
 * screen opened it on. An empty string, or no connection, keeps everything.
 */
export interface JournalFilters {
  agent: string;
  family: string;
  nature: JournalNature | "";
  result: JournalOutcome | "";
  period: JournalPeriod;
  connection: JournalConnectionFocus | null;
}

export const DEFAULT_PERIOD: JournalPeriod = "7d";

export const NO_FILTER: JournalFilters = {
  agent: "",
  family: "",
  nature: "",
  result: "",
  period: DEFAULT_PERIOD,
  connection: null,
};

/** The filters the screen opens with: none, or the connection it was asked to show. */
export function openingFilters(connection: JournalConnectionFocus | null): JournalFilters {
  return { ...NO_FILTER, connection };
}

/** `filters` without their connection, every other filter kept. */
export function withoutConnection(filters: JournalFilters): JournalFilters {
  return { ...filters, connection: null };
}

/** The families a line can be filtered on, in the order of the screens. */
export const FAMILY_OPTIONS = [
  "read",
  "read_personal",
  "write_or_delete",
  "send_external",
  "execute_code",
] as const;

const FAMILY_LABEL: Record<string, string> = {
  read: "agent_sheet.family_read",
  read_personal: "agent_sheet.family_read_personal",
  write_or_delete: "agent_sheet.family_write_or_delete",
  send_external: "agent_sheet.family_send_external",
  execute_code: "agent_sheet.family_execute_code",
  unclassified: "agent_sheet.family_unclassified",
};

/** The label of a family, `null` for one the screens do not name. */
export function familyLabelKey(family: string | null): string | null {
  return family === null ? null : (FAMILY_LABEL[family] ?? null);
}

/** What the journal is about for every user: not its own bookkeeping. */
export const READER_NATURES: readonly JournalNature[] = ["action", "access_change", "policy"];

/** The natures offered by the filter: the bookkeeping only in developer mode. */
export function natureOptions(developerMode: boolean): JournalNature[] {
  return developerMode ? [...READER_NATURES, "system"] : [...READER_NATURES];
}

export const NATURE_LABEL: Record<JournalNature, string> = {
  action: "journal.nature.action",
  access_change: "journal.nature.access_change",
  policy: "journal.nature.policy",
  system: "dev.journal.nature_system",
};

export const RESULT_OPTIONS: readonly JournalOutcome[] = ["success", "failure", "refused"];

export const RESULT_LABEL: Record<JournalOutcome, string> = {
  success: "journal.state.success",
  failure: "journal.state.failure",
  refused: "journal.state.refused",
};

export const PERIOD_OPTIONS: readonly JournalPeriod[] = ["today", "7d", "30d", "all"];

export const PERIOD_LABEL: Record<JournalPeriod, string> = {
  today: "journal.period.today",
  "7d": "journal.period.last_7_days",
  "30d": "journal.period.last_30_days",
  all: "journal.period.all",
};

const PERIOD_DAYS: Record<JournalPeriod, number | null> = { today: 0, "7d": 7, "30d": 30, all: null };

/** First instant of the period, read on the clock of this machine. */
export function periodStart(period: JournalPeriod, now: Date): string | undefined {
  const days = PERIOD_DAYS[period];
  if (days === null) return undefined;
  const start = new Date(now.getFullYear(), now.getMonth(), now.getDate() - days);
  return start.toISOString();
}

/** How many filters narrow the list, the period counted when it is not the default. */
export function activeFilterCount(filters: JournalFilters): number {
  return (
    [filters.agent, filters.family, filters.nature, filters.result].filter((value) => value !== "")
      .length +
    (filters.period === DEFAULT_PERIOD ? 0 : 1) +
    (filters.connection === null ? 0 : 1)
  );
}

/**
 * The query of the route for `filters`. With no nature chosen, every user
 * reads the actions, the access changes and the policy; the developer mode
 * adds what the journal keeps for itself.
 */
export function queryOf(
  filters: JournalFilters,
  developerMode: boolean,
  now: Date,
  offset: number,
): JournalQuery {
  const nature = filters.nature !== "" && natureOptions(developerMode).includes(filters.nature)
    ? filters.nature
    : "";
  const natures: JournalNature[] = nature !== "" ? [nature] : developerMode ? [] : [...READER_NATURES];
  const query: JournalQuery = { natures, limit: PAGE_SIZE, offset };
  if (filters.agent !== "") query.agents = [filters.agent];
  if (filters.family !== "") query.families = [filters.family];
  if (filters.result !== "") query.results = [filters.result];
  if (filters.connection !== null) query.tool_prefixes = [filters.connection.prefix];
  const since = periodStart(filters.period, now);
  if (since !== undefined) query.since = since;
  return query;
}

export interface JournalPage {
  lines: JournalLine[];
  /** Whether the route may hold older matching entries. */
  hasMore: boolean;
}

/** Reads the page at `offset` of the entries matching `filters`, from the route. */
export async function fetchPage(
  ipc: JournalIpc,
  filters: JournalFilters,
  developerMode: boolean,
  now: Date,
  offset: number,
): Promise<JournalPage> {
  const lines = await ipc.list(queryOf(filters, developerMode, now, offset));
  return { lines, hasMore: lines.length >= PAGE_SIZE };
}

// ── Lines ──────────────────────────────────────────────────────────────

/** The stable tag of a kind, the raw event name for an unmapped one. */
export function kindTag(kind: JournalKind): string {
  return typeof kind === "string" ? kind : kind.unknown.raw_kind;
}

/** What identifies a line on the screen. */
export function lineKey(line: Pick<JournalLine, "run_id" | "seq">): string {
  return `${line.run_id}#${line.seq}`;
}

function text(payload: Record<string, unknown>, key: string): string | null {
  const value = payload[key];
  return typeof value === "string" && value !== "" ? value : null;
}

function callKey(line: JournalLine): string | null {
  const id = text(line.payload, "tool_call_id");
  return id === null ? null : `${line.run_id}#${id}`;
}

/**
 * The lines the screen shows. A tool call is journaled when it starts and when
 * it ends: outside the developer mode its start is left out once its end is
 * among the loaded lines, so a call reads as one line. A call still running,
 * or one that never ended, keeps its start.
 */
export function visibleLines(lines: JournalLine[], developerMode: boolean): JournalLine[] {
  if (developerMode) return lines;
  const ended = new Set<string>();
  for (const line of lines) {
    if (kindTag(line.kind) !== "tool_call_completed") continue;
    const key = callKey(line);
    if (key !== null) ended.add(key);
  }
  return lines.filter((line) => {
    if (kindTag(line.kind) !== "tool_call_started") return true;
    const key = callKey(line);
    return key === null || !ended.has(key);
  });
}

/** A sentence to translate, with the values it names. */
export interface Sentence {
  key: string;
  values: Record<string, string>;
}

type ToolForm = "done" | "tried" | "started";

/** The sentence of a tool call, by the family of the action and by how far it went. */
const TOOL_SENTENCE: Record<string, Record<ToolForm, string>> = {
  read: {
    done: "journal.sentence.tool_done_read",
    tried: "journal.sentence.tool_tried_read",
    started: "journal.sentence.tool_started_read",
  },
  read_personal: {
    done: "journal.sentence.tool_done_read_personal",
    tried: "journal.sentence.tool_tried_read_personal",
    started: "journal.sentence.tool_started_read_personal",
  },
  write_or_delete: {
    done: "journal.sentence.tool_done_write_or_delete",
    tried: "journal.sentence.tool_tried_write_or_delete",
    started: "journal.sentence.tool_started_write_or_delete",
  },
  send_external: {
    done: "journal.sentence.tool_done_send_external",
    tried: "journal.sentence.tool_tried_send_external",
    started: "journal.sentence.tool_started_send_external",
  },
  execute_code: {
    done: "journal.sentence.tool_done_execute_code",
    tried: "journal.sentence.tool_tried_execute_code",
    started: "journal.sentence.tool_started_execute_code",
  },
};

/** The sentence of a call whose family the screens do not name. */
const TOOL_SENTENCE_OTHER: Record<ToolForm, string> = {
  done: "journal.sentence.tool_done_other",
  tried: "journal.sentence.tool_tried_other",
  started: "journal.sentence.tool_started_other",
};

function toolSentence(family: string | null, form: ToolForm): string {
  return (family === null ? undefined : TOOL_SENTENCE[family])?.[form] ?? TOOL_SENTENCE_OTHER[form];
}

const FIXED_SENTENCE: Record<string, string> = {
  always_withdrawn: "journal.sentence.always_withdrawn",
  policy_refused: "journal.sentence.policy_refused",
  rule_refused: "journal.sentence.rule_refused",
  rule_added: "journal.sentence.rule_added",
  rule_removed: "journal.sentence.rule_removed",
  batch_access_granted: "journal.sentence.batch_access_granted",
  agent_installed: "journal.sentence.agent_installed",
  agent_updated: "journal.sentence.agent_updated",
  agent_uninstalled: "journal.sentence.agent_uninstalled",
  agent_override_changed: "journal.sentence.agent_override_changed",
  package_installed: "journal.sentence.package_installed",
  package_updated: "journal.sentence.package_updated",
  package_uninstalled: "journal.sentence.package_uninstalled",
  connection_added: "journal.sentence.connection_added",
  connection_modified: "journal.sentence.connection_modified",
  connection_removed: "journal.sentence.connection_removed",
  connection_warning_accepted: "journal.sentence.connection_warning_accepted",
  connection_secret_set: "journal.sentence.connection_secret_set",
  connection_secret_removed: "journal.sentence.connection_secret_removed",
  account_connected: "journal.sentence.account_connected",
  account_disconnected: "journal.sentence.account_disconnected",
  folder_added: "journal.sentence.folder_added",
  policy_applied: "journal.sentence.policy_applied",
  policy_changed: "journal.sentence.policy_changed",
  policy_unreadable: "journal.sentence.policy_unreadable",
  fail_closed_triggered: "journal.sentence.fail_closed_triggered",
  journal_exported: "journal.sentence.journal_exported",
  journal_purged: "journal.sentence.journal_purged",
  journal_unsigned: "journal.sentence.journal_unsigned",
  journal_export_failed: "journal.sentence.journal_export_failed",
};

const ACCESS_SENTENCE: Record<string, string> = {
  granted: "journal.sentence.access_granted",
  revoked: "journal.sentence.access_revoked",
  rescoped: "journal.sentence.access_rescoped",
};

const CONSENT_SENTENCE: Record<string, string> = {
  once: "journal.sentence.consent_once",
  always: "journal.sentence.consent_always",
  refuse: "journal.sentence.consent_refused",
};

/**
 * What a line says, in plain language: the agent is the subject of what it
 * did, and the sentence holds no argument of the call. `agentName` gives the
 * name an agent is shown under.
 */
export function sentenceOf(
  line: Pick<JournalLine, "kind" | "payload" | "summary">,
  agentName: (agent: string | null) => string,
): Sentence {
  const tag = kindTag(line.kind);
  const values = { agent: agentName(line.summary.agent) };
  if (tag === "tool_call_completed") {
    const form = line.summary.outcome === "failure" ? "tried" : "done";
    return { key: toolSentence(line.summary.family, form), values };
  }
  if (tag === "tool_call_started") {
    return { key: toolSentence(line.summary.family, "started"), values };
  }
  if (tag === "access_changed") {
    const change = text(line.payload, "change") ?? "";
    return { key: ACCESS_SENTENCE[change] ?? "journal.sentence.access_rescoped", values };
  }
  if (tag === "consent_answered") {
    const answer = text(line.payload, "answer") ?? "";
    return { key: CONSENT_SENTENCE[answer] ?? "journal.sentence.consent_once", values };
  }
  const fixed = FIXED_SENTENCE[tag];
  if (fixed !== undefined) return { key: fixed, values };
  return { key: "dev.journal.sentence_system", values: { kind: tag, what: line.summary.what } };
}

export type LineState = "success" | "failure" | "refused" | "blocked" | "granted" | "added";

export const STATE_LABEL: Record<LineState, string> = {
  success: "journal.state.success",
  failure: "journal.state.failure",
  refused: "journal.state.refused",
  blocked: "journal.state.blocked",
  granted: "journal.state.granted",
  added: "journal.state.added",
};

/** The tone of the state badge. */
export const STATE_TONE: Record<LineState, "success" | "warning" | "danger" | "neutral"> = {
  success: "success",
  failure: "warning",
  refused: "danger",
  blocked: "danger",
  granted: "neutral",
  added: "neutral",
};

const ADDED_KINDS = new Set([
  "connection_added",
  "account_connected",
  "folder_added",
  "rule_added",
  "agent_installed",
  "package_installed",
]);

/** The state a line carries, `null` when it records no ending. */
export function stateOf(line: JournalLine): LineState | null {
  const tag = kindTag(line.kind);
  if (tag === "policy_refused") return "blocked";
  if (line.summary.outcome === "refused") return "refused";
  if (line.summary.outcome === "failure") return "failure";
  if (tag === "consent_answered" || tag === "batch_access_granted") return "granted";
  if (tag === "access_changed" && text(line.payload, "change") === "granted") return "granted";
  if (ADDED_KINDS.has(tag)) return "added";
  return line.summary.outcome === "success" ? "success" : null;
}

/** Which glyph stands before a line. */
export type LineIcon = "agent" | "access" | "connection" | "policy" | "system";

const CONNECTION_KINDS = /^(connection_|account_|folder_)/;

export function iconOf(line: JournalLine): LineIcon {
  if (line.summary.nature === "policy") return "policy";
  if (line.summary.nature === "system") return "system";
  if (line.summary.nature === "action") return "agent";
  return CONNECTION_KINDS.test(kindTag(line.kind)) ? "connection" : "access";
}

/** What the discreet line under the sentence names, the hour aside. */
export interface LineMeta {
  /** What was acted on, `null` when the entry names none. */
  target: string | null;
  /** Label of the family, or of the nature when the entry has no family. */
  categoryKey: string;
}

export function metaOf(line: JournalLine): LineMeta {
  return {
    target: line.summary.target,
    categoryKey: familyLabelKey(line.summary.family) ?? NATURE_LABEL[line.summary.nature],
  };
}

// ── Days ───────────────────────────────────────────────────────────────

export interface DayGroup {
  /** Local day, `YYYY-MM-DD`; empty for entries whose date cannot be read. */
  day: string;
  relative: "today" | "yesterday" | "other";
  /** First instant met of that day, for the label. */
  date: Date | null;
  lines: JournalLine[];
}

function localDay(date: Date): string {
  const month = String(date.getMonth() + 1).padStart(2, "0");
  const day = String(date.getDate()).padStart(2, "0");
  return `${date.getFullYear()}-${month}-${day}`;
}

/** The lines grouped by local day, in the order received (newest first). */
export function groupByDay(lines: JournalLine[], now: Date): DayGroup[] {
  const today = localDay(now);
  const yesterday = localDay(new Date(now.getFullYear(), now.getMonth(), now.getDate() - 1));
  const groups: DayGroup[] = [];
  for (const line of lines) {
    const date = new Date(line.ts);
    const readable = !Number.isNaN(date.getTime());
    const day = readable ? localDay(date) : "";
    const last = groups.at(-1);
    if (last !== undefined && last.day === day) {
      last.lines.push(line);
      continue;
    }
    groups.push({
      day,
      relative: day === today ? "today" : day === yesterday ? "yesterday" : "other",
      date: readable ? date : null,
      lines: [line],
    });
  }
  return groups;
}

// ── Detail ─────────────────────────────────────────────────────────────

/**
 * How the detail of the entries opens. Every user opens the detail of one
 * entry with "see more"; in developer mode every entry arrives unfolded, and
 * `flipped` holds the ones the user turned the other way.
 */
export interface Expansion {
  unfoldedByDefault: boolean;
  flipped: ReadonlySet<string>;
}

/** The expansion a mode starts with: unfolded by default in developer mode. */
export function expansionFor(developerMode: boolean): Expansion {
  return { unfoldedByDefault: developerMode, flipped: new Set() };
}

export function isUnfolded(expansion: Expansion, key: string): boolean {
  return expansion.unfoldedByDefault !== expansion.flipped.has(key);
}

/** `expansion` with the entry `key` turned the other way. */
export function flip(expansion: Expansion, key: string): Expansion {
  const flipped = new Set(expansion.flipped);
  if (!flipped.delete(key)) flipped.add(key);
  return { unfoldedByDefault: expansion.unfoldedByDefault, flipped };
}

/** Every entry folded, or every entry unfolded. */
export function foldAll(unfolded: boolean): Expansion {
  return { unfoldedByDefault: unfolded, flipped: new Set() };
}

/** Whether at least one of the entries `keys` is unfolded. */
export function anyUnfolded(expansion: Expansion, keys: string[]): boolean {
  return keys.some((key) => isUnfolded(expansion, key));
}

const CALL_KINDS = new Set(["tool_call_started", "tool_call_completed", "tool_output"]);

/** Whether the detail of a line holds more than the line: a tool call does. */
export function hasCallDetail(line: JournalLine): boolean {
  return CALL_KINDS.has(kindTag(line.kind)) && text(line.payload, "tool_call_id") !== null;
}

/** The detail of a line that is about no tool call: the line itself. */
export function ownDetail(line: JournalLine): JournalDetailState {
  return { status: "ready", detail: { ...line, tool_call: null } };
}

/** One named value of the arguments of a call, or of a record. */
export interface NamedValue {
  name: string;
  value: string;
}

function shown(value: unknown): string {
  if (typeof value === "string") return value;
  if (value === null || value === undefined) return "";
  if (typeof value === "number" || typeof value === "boolean") return String(value);
  return JSON.stringify(value);
}

/** The fields of `value` one by one; a value that is no object stands alone. */
export function namedValues(value: unknown, without: readonly string[] = []): NamedValue[] {
  if (value === null || value === undefined) return [];
  if (typeof value !== "object" || Array.isArray(value)) return [{ name: "", value: shown(value) }];
  return Object.entries(value as Record<string, unknown>)
    .filter(([name]) => !without.includes(name))
    .map(([name, field]) => ({ name, value: shown(field) }));
}

/** A value as the journal holds it, indented, for the developer mode. */
export function rawJson(value: unknown): string {
  return value === null || value === undefined ? "" : JSON.stringify(value, null, 2);
}

/** What "see more" shows of an entry, to every user. */
export interface EntryDetailView {
  tool: string | null;
  target: string | null;
  familyKey: string | null;
  ts: string;
  state: LineState | null;
  /** `true` when the entry is about a tool call. */
  isCall: boolean;
  /** The arguments of the call, `null` when the journal kept none. */
  parameters: NamedValue[] | null;
  /** The output of the call, `null` when the journal kept none. */
  output: string | null;
  /** The fields of the record of an entry that is about no call. */
  record: NamedValue[];
}

export function detailView(detail: JournalEntryDetail): EntryDetailView {
  const call = detail.tool_call;
  const isCall = hasCallDetail(detail);
  return {
    tool: detail.summary.tool,
    target: detail.summary.target,
    familyKey: familyLabelKey(detail.summary.family),
    ts: detail.ts,
    state: stateOf(detail),
    isCall,
    parameters: call !== null && call.arguments !== null && call.arguments !== undefined
      ? namedValues(call.arguments)
      : null,
    output: call !== null && call.output !== null && call.output !== undefined
      ? shown(call.output)
      : null,
    record: isCall ? [] : namedValues(detail.payload, ["event"]),
  };
}

/** What the developer mode adds under the detail of an entry. */
export interface TechnicalView {
  kind: string;
  tool: string | null;
  seq: number;
  runId: string;
  callId: string | null;
  arguments: string;
  payload: string;
  hash: string;
  prevHash: string;
}

export function technicalView(detail: JournalEntryDetail): TechnicalView {
  return {
    kind: kindTag(detail.kind),
    tool: detail.summary.tool,
    seq: detail.seq,
    runId: detail.run_id,
    callId: detail.tool_call?.tool_call_id ?? null,
    arguments: rawJson(detail.tool_call?.arguments),
    payload: rawJson(detail.payload),
    hash: detail.hash,
    prevHash: detail.prev_hash,
  };
}
