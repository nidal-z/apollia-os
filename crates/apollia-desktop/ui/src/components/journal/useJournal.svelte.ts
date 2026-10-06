/**
 * The live state of the journal screen: the lines read from the route for the
 * filters in force, the pages loaded since, and the detail of the entries the
 * user opened.
 */
import { getJournalEntry, listJournalEntries } from "$lib/ipc/journal";
import type { JournalLine } from "$lib/ipc/journal";
import type { JournalDetailState } from "$lib/stores/journalSelection";
import {
  fetchPage,
  hasCallDetail,
  lineKey,
  ownDetail,
  type JournalFilters,
  type JournalIpc,
} from "./journal";

export const shellIpc: JournalIpc = { list: listJournalEntries, entry: getJournalEntry };

export interface Journal {
  /** Every line read for the filters in force, newest first. */
  readonly lines: JournalLine[];
  readonly loading: boolean;
  /** `true` while an older page is being read. */
  readonly loadingMore: boolean;
  /** Why the journal could not be read, `null` when it was. */
  readonly error: string | null;
  /** Whether the route may hold older matching entries. */
  readonly hasMore: boolean;
  /** Reads the first page for `filters`, replacing what was shown. */
  load(filters: JournalFilters, developerMode: boolean): Promise<void>;
  /** Reads the next older page for the filters of the last load. */
  loadMore(): Promise<void>;
  /** Where the detail of `line` stands; `null` before it was asked for. */
  detailOf(line: JournalLine): JournalDetailState | null;
  /** Reads the detail of `line` once. */
  ensureDetail(line: JournalLine): Promise<void>;
}

export function createJournal(ipc: JournalIpc = shellIpc): Journal {
  let lines = $state<JournalLine[]>([]);
  let loading = $state(false);
  let loadingMore = $state(false);
  let error = $state<string | null>(null);
  let hasMore = $state(false);
  let details = $state<Record<string, JournalDetailState>>({});
  let request = 0;
  let last: { filters: JournalFilters; developerMode: boolean } | null = null;

  async function load(filters: JournalFilters, developerMode: boolean): Promise<void> {
    const mine = ++request;
    last = { filters: { ...filters }, developerMode };
    loading = true;
    error = null;
    try {
      const page = await fetchPage(ipc, filters, developerMode, new Date(), 0);
      if (mine !== request) return;
      lines = page.lines;
      hasMore = page.hasMore;
      details = {};
    } catch (err) {
      if (mine !== request) return;
      error = err instanceof Error ? err.message : String(err);
      lines = [];
      hasMore = false;
    } finally {
      if (mine === request) loading = false;
    }
  }

  async function loadMore(): Promise<void> {
    if (last === null || loading || loadingMore || !hasMore) return;
    const mine = request;
    loadingMore = true;
    try {
      const page = await fetchPage(ipc, last.filters, last.developerMode, new Date(), lines.length);
      if (mine !== request) return;
      lines = [...lines, ...page.lines];
      hasMore = page.hasMore;
    } catch (err) {
      if (mine !== request) return;
      error = err instanceof Error ? err.message : String(err);
    } finally {
      loadingMore = false;
    }
  }

  async function ensureDetail(line: JournalLine): Promise<void> {
    const key = lineKey(line);
    if (details[key] !== undefined) return;
    if (!hasCallDetail(line)) {
      details[key] = ownDetail(line);
      return;
    }
    details[key] = { status: "loading" };
    try {
      const detail = await ipc.entry(line.run_id, line.seq);
      details[key] = detail === null ? { status: "missing" } : { status: "ready", detail };
    } catch {
      details[key] = { status: "error" };
    }
  }

  return {
    get lines() { return lines; },
    get loading() { return loading; },
    get loadingMore() { return loadingMore; },
    get error() { return error; },
    get hasMore() { return hasMore; },
    load,
    loadMore,
    detailOf: (line) => details[lineKey(line)] ?? null,
    ensureDetail,
  };
}
