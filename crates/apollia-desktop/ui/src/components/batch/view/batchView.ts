/**
 * The decisions of the view of a batch and of the history of the batches:
 * how far a batch went, which commands its state offers, what a row of its
 * table says, how the table is narrowed, how the history is paged, grouped
 * and filtered.
 *
 * Every figure comes from the counters and the items the batch engine
 * recorded: the progress is the share of items actually dealt with, never a
 * percentage chosen per state.
 */
import type {
  BatchControl,
  BatchDetail,
  BatchExportFormat,
  BatchExported,
  BatchItem,
  BatchItemAnswer,
  BatchItemDecided,
  BatchItemDecision,
  BatchSummary,
} from "$lib/ipc/batches";

/** The shell calls the two screens make, replaceable in tests. */
export interface BatchIpc {
  list: (limit: number) => Promise<BatchSummary[]>;
  get: (batchId: string) => Promise<BatchDetail | null>;
  control: (batchId: string, action: BatchControl) => Promise<BatchSummary>;
  exportTo: (batchId: string, format: BatchExportFormat, path: string) => Promise<BatchExported>;
  decision: (batchId: string, rank: number) => Promise<BatchItemDecision | null>;
  decide: (id: string, answer: BatchItemAnswer) => Promise<BatchItemDecided>;
}

// ── Progress ───────────────────────────────────────────────────────────

/** Where a batch stands, counted on its items. */
export interface BatchProgress {
  /** Items dealt with: succeeded, failed, or waiting for a decision. */
  treated: number;
  total: number;
  /** Items not started yet, the one running included. */
  remaining: number;
  /** Share of the items dealt with, from 0 to 100. */
  percent: number;
}

export function progressOf(batch: BatchSummary): BatchProgress {
  const treated = batch.completed + batch.failed + batch.awaiting;
  const remaining = Math.max(0, batch.total - treated - batch.canceled);
  const percent = batch.total > 0 ? Math.min(100, (treated / batch.total) * 100) : 0;
  return { treated, total: batch.total, remaining, percent };
}

/** The five states a batch can be in; an unknown one reads as running. */
export type BatchState = "running" | "paused" | "completed" | "interrupted" | "stopped";

const STATES: readonly BatchState[] = ["running", "paused", "completed", "interrupted", "stopped"];

export function stateOf(batch: BatchSummary): BatchState {
  return STATES.find((state) => state === batch.state) ?? "running";
}

/** Whether the batch may still change without the user acting on it. */
export function isLive(batch: BatchSummary): boolean {
  const state = stateOf(batch);
  return state === "running" || state === "paused";
}

export type Tone = "info" | "warning" | "success" | "danger" | "neutral";

export const STATE_TONE: Record<BatchState, Tone> = {
  running: "info",
  paused: "warning",
  completed: "success",
  interrupted: "danger",
  stopped: "neutral",
};

/** The tone of the progress bar, as the bar names its variants. */
export const BAR_VARIANT: Record<BatchState, "primary" | "warning" | "success" | "destructive" | "info"> = {
  running: "primary",
  paused: "warning",
  completed: "success",
  interrupted: "destructive",
  stopped: "info",
};

// ── Commands ───────────────────────────────────────────────────────────

/** A command of the summary card. */
export interface BatchCommand {
  id: BatchControl | "export";
  enabled: boolean;
  /** The one action the state calls for. */
  primary: boolean;
}

/**
 * The commands a batch offers, in display order. Retry is shown but inert
 * while the batch runs; an interrupted batch only exports and retries, the
 * retry also running the items left; a stopped batch does not start again.
 */
export function commandsFor(batch: BatchSummary): BatchCommand[] {
  const retryable = batch.failed + batch.awaiting > 0;
  const exportCommand: BatchCommand = { id: "export", enabled: true, primary: false };
  const retry = (enabled: boolean, primary: boolean): BatchCommand => ({ id: "retry", enabled, primary });
  switch (stateOf(batch)) {
    case "running":
      return [
        { id: "pause", enabled: true, primary: false },
        { id: "stop", enabled: true, primary: false },
        retry(false, false),
        exportCommand,
      ];
    case "paused":
      return [
        { id: "stop", enabled: true, primary: false },
        retry(retryable, false),
        exportCommand,
        { id: "resume", enabled: true, primary: true },
      ];
    case "interrupted":
      return [exportCommand, retry(retryable, retryable)];
    case "completed":
      return retryable ? [exportCommand, retry(true, true)] : [exportCommand];
    case "stopped":
      return [exportCommand];
  }
}

// ── Schedule ───────────────────────────────────────────────────────────

function time(value: string | null): number | null {
  if (value === null) return null;
  const at = new Date(value).getTime();
  return Number.isNaN(at) ? null : at;
}

/**
 * When a running batch should end at the pace observed so far, `null` when
 * nothing was dealt with yet or the batch is not running. An approximation:
 * the pace counts the pauses the batch went through.
 */
export function estimatedEnd(batch: BatchSummary, now: Date): Date | null {
  const started = time(batch.createdAt);
  const { treated, remaining } = progressOf(batch);
  if (stateOf(batch) !== "running" || started === null || treated === 0 || remaining === 0) return null;
  const elapsed = now.getTime() - started;
  if (elapsed <= 0) return null;
  return new Date(now.getTime() + (elapsed / treated) * remaining);
}

/** How long the batch ran, in whole minutes, until its end or until `now`. */
export function durationMinutes(batch: BatchSummary, now: Date): number | null {
  const started = time(batch.createdAt);
  if (started === null) return null;
  const end = time(batch.endedAt) ?? now.getTime();
  return Math.max(0, Math.round((end - started) / 60_000));
}

// ── Source ─────────────────────────────────────────────────────────────

/** The last segment of a path, or the location itself for a mailbox. */
export function sourceName(location: string): string {
  const parts = location.split(/[\\/]/).filter((part) => part !== "");
  return parts.at(-1) ?? location;
}

/** The columns the batch writes into its file, empty when it writes nothing. */
export function writtenColumns(batch: BatchSummary): string[] {
  return batch.source.write?.columns ?? [];
}

// ── Items ──────────────────────────────────────────────────────────────

export type ItemStatus = "pending" | "running" | "completed" | "failed" | "canceled" | "awaiting";

const ITEM_STATUSES: readonly ItemStatus[] = ["pending", "running", "completed", "failed", "canceled", "awaiting"];

export function statusOf(item: BatchItem): ItemStatus {
  return ITEM_STATUSES.find((status) => status === item.status) ?? "pending";
}

export const ITEM_TONE: Record<ItemStatus, Tone> = {
  pending: "neutral",
  running: "info",
  completed: "success",
  failed: "danger",
  canceled: "neutral",
  awaiting: "warning",
};

function text(value: unknown): string {
  if (typeof value === "string") return value.trim();
  if (value === null || value === undefined) return "";
  return typeof value === "object" ? JSON.stringify(value) : String(value);
}

/** The fields of an item, in the order its source read them. */
export function fieldsOf(item: BatchItem): { name: string; value: string }[] {
  if (typeof item.input !== "object" || item.input === null || Array.isArray(item.input)) return [];
  return Object.entries(item.input as Record<string, unknown>).map(([name, value]) => ({ name, value: text(value) }));
}

/** What a row shows of an input: its first filled field, then the next ones. */
export function entryOf(item: BatchItem): { title: string; summary: string } {
  const filled = fieldsOf(item)
    .map((field) => field.value)
    .filter((value) => value !== "");
  if (filled.length === 0) return { title: text(item.input), summary: "" };
  return { title: filled[0], summary: filled.slice(1).join(" · ") };
}

const AWAITING_PREFIX = "Waiting for a decision: the agent asked to use '";
const AWAITING_SUFFIX = "', which is not granted";

/** The access a waiting item reached for, read from the output recorded for it. */
export function awaitedAccess(item: BatchItem): string | null {
  const output = item.output ?? "";
  if (!output.startsWith(AWAITING_PREFIX) || !output.endsWith(AWAITING_SUFFIX)) return null;
  const access = output.slice(AWAITING_PREFIX.length, output.length - AWAITING_SUFFIX.length);
  return access === "" ? null : access;
}

const SOURCE_NAME_KEYS = ["title", "url", "uri", "path", "location", "name"] as const;

function citedName(source: unknown): string {
  if (typeof source === "object" && source !== null && !Array.isArray(source)) {
    const fields = source as Record<string, unknown>;
    const key = SOURCE_NAME_KEYS.find((candidate) => typeof fields[candidate] === "string");
    if (key !== undefined) return fields[key] as string;
  }
  return text(source);
}

/** The sources the answer of an item cites, each by its name. */
export function citedSources(item: BatchItem): string[] {
  if (item.sources === null || item.sources === undefined) return [];
  const list = Array.isArray(item.sources) ? item.sources : [item.sources];
  return list.map(citedName).filter((name) => name !== "");
}

// ── Table ──────────────────────────────────────────────────────────────

/** A tab of the table: every item, or those in one group of statuses. */
export type ItemTab = "all" | "completed" | "failed" | "todo" | "awaiting" | "canceled";

const TAB_STATUSES: Record<Exclude<ItemTab, "all">, readonly ItemStatus[]> = {
  completed: ["completed"],
  failed: ["failed"],
  todo: ["pending", "running"],
  awaiting: ["awaiting"],
  canceled: ["canceled"],
};

function inTab(item: BatchItem, tab: ItemTab): boolean {
  return tab === "all" || TAB_STATUSES[tab].includes(statusOf(item));
}

/**
 * The tabs of the table with their counts. "All", "Succeeded" and "Failed"
 * always show; the three others only while they hold an item, so a finished
 * batch no longer offers "To process".
 */
export function tabsFor(items: BatchItem[]): { key: ItemTab; count: number }[] {
  const count = (tab: ItemTab): number => items.filter((item) => inTab(item, tab)).length;
  const always: ItemTab[] = ["all", "completed", "failed"];
  const onDemand: ItemTab[] = ["todo", "awaiting", "canceled"];
  return [
    ...always.map((key) => ({ key, count: count(key) })),
    ...onDemand.map((key) => ({ key, count: count(key) })).filter((tab) => tab.count > 0),
  ];
}

/** The tab shown: the one asked for while the table still offers it. */
export function shownItemTab(asked: ItemTab, items: BatchItem[]): ItemTab {
  return tabsFor(items).some((tab) => tab.key === asked) ? asked : "all";
}

/** The items of a tab whose input, output or sources hold `query`. */
export function filterItems(items: BatchItem[], tab: ItemTab, query: string): BatchItem[] {
  const needle = query.trim().toLocaleLowerCase();
  return items.filter((item) => {
    if (!inTab(item, tab)) return false;
    if (needle === "") return true;
    const haystack = [
      String(item.rank + 1),
      ...fieldsOf(item).map((field) => field.value),
      item.output ?? "",
      item.origin,
      ...citedSources(item),
    ];
    return haystack.some((value) => value.toLocaleLowerCase().includes(needle));
  });
}

// ── History ────────────────────────────────────────────────────────────

/** Batches asked for per page of the history. */
export const HISTORY_PAGE = 50;

/**
 * How many batches the history asks for to show `pages` pages: one more than
 * it shows, which is how it knows whether an older batch exists.
 */
export function historyLimit(pages: number): number {
  return Math.max(1, pages) * HISTORY_PAGE + 1;
}

/** The batches to show for `pages` pages, and whether an older one exists. */
export function historyPage(read: BatchSummary[], pages: number): { batches: BatchSummary[]; hasMore: boolean } {
  const shown = Math.max(1, pages) * HISTORY_PAGE;
  return { batches: read.slice(0, shown), hasMore: read.length > shown };
}

/** How a batch ended, or where it stands, as the history names it. */
export type BatchOutcome = "running" | "paused" | "completed" | "completed_with_failures" | "interrupted" | "stopped";

export function outcomeOf(batch: BatchSummary): BatchOutcome {
  const state = stateOf(batch);
  return state === "completed" && batch.failed > 0 ? "completed_with_failures" : state;
}

export const OUTCOME_TONE: Record<BatchOutcome, Tone> = {
  running: "info",
  paused: "warning",
  completed: "success",
  completed_with_failures: "warning",
  interrupted: "danger",
  stopped: "neutral",
};

export const OUTCOMES: readonly BatchOutcome[] = [
  "running",
  "paused",
  "completed",
  "completed_with_failures",
  "interrupted",
  "stopped",
];

/** The filters of the history. An empty string keeps everything. */
export interface HistoryFilters {
  agent: string;
  outcome: BatchOutcome | "";
  query: string;
}

export const NO_HISTORY_FILTER: HistoryFilters = { agent: "", outcome: "", query: "" };

export function filterBatches(batches: BatchSummary[], filters: HistoryFilters): BatchSummary[] {
  const needle = filters.query.trim().toLocaleLowerCase();
  return batches.filter((batch) => {
    if (filters.agent !== "" && batch.agent !== filters.agent) return false;
    if (filters.outcome !== "" && outcomeOf(batch) !== filters.outcome) return false;
    if (needle === "") return true;
    return [batch.agent, batch.source.location].some((value) => value.toLocaleLowerCase().includes(needle));
  });
}

/** The agents the batches read so far were run by, in alphabetical order. */
export function agentsOf(batches: BatchSummary[]): string[] {
  return [...new Set(batches.map((batch) => batch.agent))].sort((a, b) => a.localeCompare(b));
}

export type HistoryPeriod = "this_week" | "older";

/** The Monday, at midnight local time, of the week `now` falls in. */
function weekStart(now: Date): number {
  const start = new Date(now.getFullYear(), now.getMonth(), now.getDate());
  start.setDate(start.getDate() - ((start.getDay() + 6) % 7));
  return start.getTime();
}

/** The batches in the order read, split between this week and before. */
export function groupByPeriod(
  batches: BatchSummary[],
  now: Date,
): { period: HistoryPeriod; batches: BatchSummary[] }[] {
  const since = weekStart(now);
  const recent = batches.filter((batch) => (time(batch.createdAt) ?? 0) >= since);
  const older = batches.filter((batch) => (time(batch.createdAt) ?? 0) < since);
  return [
    { period: "this_week" as const, batches: recent },
    { period: "older" as const, batches: older },
  ].filter((group) => group.batches.length > 0);
}

// ── Export ─────────────────────────────────────────────────────────────

export const EXPORT_FORMATS: readonly BatchExportFormat[] = ["xlsx", "csv", "json"];

/** The file name the save dialog proposes for an export. */
export function exportFileName(batch: BatchSummary, format: BatchExportFormat): string {
  return `batch-${batch.batchId}.${format}`;
}
