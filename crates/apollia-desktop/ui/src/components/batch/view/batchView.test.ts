import { readFileSync } from "node:fs";
import { describe, expect, it, vi } from "vitest";
import { get } from "svelte/store";
import type { BatchDetail, BatchItem, BatchSummary } from "$lib/ipc/batches";
import { batchNotice } from "$lib/stores/batchNotifications";
import { currentBatch, openBatch, openBatchHistory } from "$lib/stores/batchView";
import { currentRoute, sidebarRoute } from "$lib/stores/navigation";
import {
  HISTORY_PAGE,
  agentsOf,
  answerFields,
  isMoving,
  synthesisOf,
  awaitedAccess,
  citedSources,
  commandsFor,
  durationMinutes,
  entryOf,
  itemPace,
  exportFileName,
  filterBatches,
  filterItems,
  groupByPeriod,
  historyLimit,
  historyPage,
  outcomeOf,
  progressOf,
  shownItemTab,
  sourceName,
  tabsFor,
  type BatchIpc,
} from "./batchView";
import { createBatchView } from "./useBatch.svelte";
import { createBatchHistory } from "./useBatchHistory.svelte";

function batch(over: Partial<BatchSummary> = {}): BatchSummary {
  return {
    batchId: "b-1",
    agent: "triage",
    source: { kind: "csv", location: "/data/requests.csv", options: {} },
    grants: [],
    concurrency: 1,
    maxFailures: 10,
    state: "running",
    total: 140,
    completed: 57,
    failed: 4,
    canceled: 0,
    awaiting: 1,
    createdAt: "2026-10-02T14:05:00Z",
    updatedAt: "2026-10-02T14:12:00Z",
    endedAt: null,
    ...over,
  };
}

function item(rank: number, status: string, over: Partial<BatchItem> = {}): BatchItem {
  return {
    rank,
    origin: `row ${rank + 1}`,
    input: { client: `Client ${rank + 1}`, request: "A quote" },
    status,
    output: null,
    sources: null,
    taskId: null,
    ...over,
  };
}

function ipcOf(over: Partial<BatchIpc> = {}): BatchIpc {
  const refused = (): Promise<never> => Promise.reject(new Error("not stubbed"));
  return { list: refused, get: refused, control: refused, exportTo: refused, decision: refused, decide: refused, ...over };
}

describe("the progress of a batch", () => {
  it("counts the items dealt with, never a percentage chosen per state", () => {
    // GIVEN a running batch of 140 items: 57 succeeded, 4 failed, 1 waiting
    const running = batch();

    // WHEN its progress is computed
    const progress = progressOf(running);

    // THEN 62 items are dealt with, 78 are left, and the bar shows that share
    expect(progress).toEqual({ treated: 62, total: 140, remaining: 78, percent: (62 / 140) * 100 });
  });

  it("gives two batches in the same state two different shares", () => {
    // GIVEN two running batches at different points
    const early = batch({ completed: 1, failed: 0, awaiting: 0 });
    const late = batch({ completed: 130, failed: 0, awaiting: 0 });

    // WHEN their progress is computed
    // THEN the share follows the items, not the state
    expect(progressOf(early).percent).toBeLessThan(1);
    expect(progressOf(late).percent).toBeGreaterThan(90);
    expect(progressOf(batch({ total: 0, completed: 0, failed: 0, awaiting: 0 })).percent).toBe(0);
  });

  it("leaves the items a stop canceled out of both the dealt with and the left", () => {
    // GIVEN a stopped batch: 11 succeeded, 19 never ran
    const stopped = batch({ state: "stopped", total: 30, completed: 11, failed: 0, awaiting: 0, canceled: 19 });

    // WHEN its progress is computed
    // THEN 11 are dealt with and none is left to process
    expect(progressOf(stopped)).toMatchObject({ treated: 11, remaining: 0 });
  });

  it("reads the pace on the last items that ended, pauses left out", () => {
    // GIVEN six ended items, the oldest one slow, and one still running
    const ended = (rank: number, ms: number, at: string): BatchItem => ({
      rank, origin: `row ${rank}`, input: {}, status: "completed", output: "ok", sources: null,
      taskId: `t-${rank}`, updatedAt: at, durationMs: ms,
    });
    const items: BatchItem[] = [
      ended(0, 600_000, "2026-10-16 14:00:00"),
      ended(1, 60_000, "2026-10-18 09:00:00"),
      ended(2, 60_000, "2026-10-18 09:01:00"),
      ended(3, 120_000, "2026-10-18 09:03:00"),
      ended(4, 60_000, "2026-10-18 09:04:00"),
      ended(5, 100_000, "2026-10-18 09:06:00"),
      { rank: 6, origin: "row 6", input: {}, status: "running", output: null, sources: null, taskId: "t-6" },
    ];

    // WHEN the pace is read
    // THEN it is the mean of the five last ended, the old slow one left out, and nothing before any ended
    expect(itemPace(items)).toBe(80_000);
    expect(itemPace(items.slice(6))).toBeNull();
    const running = batch({ total: 100, completed: 50, failed: 0, awaiting: 0, createdAt: "2026-10-02T14:00:00Z" });
    const now = new Date("2026-10-02T14:10:00Z");
    expect(durationMinutes({ ...running, endedAt: "2026-10-02T14:24:00Z" }, now)).toBe(24);
    expect(durationMinutes(running, now)).toBe(10);
  });
});

describe("the commands of a batch", () => {
  const ids = (b: BatchSummary) => commandsFor(b).map((c) => `${c.id}${c.enabled ? "" : "(off)"}${c.primary ? "*" : ""}`);

  it("offers pause, stop and export while running, with retry shown inert", () => {
    // GIVEN a running batch with failures
    // WHEN its commands are listed
    // THEN retry is there but cannot be used yet
    expect(ids(batch())).toEqual(["pause", "stop", "retry(off)", "export"]);
  });

  it("makes resume the action of a paused batch, whose failures can be retried", () => {
    // GIVEN a paused batch with failures, then one without
    // WHEN their commands are listed
    // THEN resume leads, and retry follows the failures
    expect(ids(batch({ state: "paused" }))).toEqual(["stop", "retry", "export", "resume*"]);
    expect(ids(batch({ state: "paused", failed: 0, awaiting: 0 }))).toEqual(["stop", "retry(off)", "export", "resume*"]);
  });

  it("leaves an interrupted batch export and retry only", () => {
    // GIVEN a batch interrupted past its threshold
    // WHEN its commands are listed
    // THEN neither resume nor stop is offered, which the runtime would refuse
    expect(ids(batch({ state: "interrupted", failed: 10 }))).toEqual(["export", "retry*"]);
  });

  it("offers retry on a completed batch only when an item failed or waits, and never on a stopped one", () => {
    // GIVEN completed batches with and without failures, and a stopped batch
    // WHEN their commands are listed
    // THEN a stopped batch does not start again
    expect(ids(batch({ state: "completed" }))).toEqual(["export", "retry*"]);
    expect(ids(batch({ state: "completed", failed: 0, awaiting: 0 }))).toEqual(["export"]);
    expect(ids(batch({ state: "stopped" }))).toEqual(["export"]);
  });
});

describe("the table of a batch", () => {
  const items = [
    item(0, "completed", { output: "Information, low priority", sources: [{ title: "Contract 2024-118" }, "Client sheet"] }),
    item(1, "failed", { output: "The attached file is an image" }),
    item(2, "awaiting", { output: "Waiting for a decision: the agent asked to use 'send_email', which is not granted" }),
    item(3, "running"),
    item(4, "pending"),
  ];

  it("reads a row from its input: the first filled field, then the others", () => {
    // GIVEN an item whose first field is empty, and a bare input
    const sparse = item(0, "pending", { input: { id: "", client: "Acme", request: "A quote", note: 3 } });
    const bare = item(1, "pending", { input: "one line" });

    // WHEN their rows are read
    // THEN the title is the first field that says something
    expect(entryOf(sparse)).toEqual({ title: "Acme", summary: "A quote · 3" });
    expect(entryOf(bare)).toEqual({ title: "one line", summary: "" });
  });

  it("names the sources an answer cites, and the access a waiting item reached for", () => {
    // GIVEN an item with two cited sources and one that waits
    // WHEN they are read
    // THEN each source is named, and the access comes out of the recorded output
    expect(citedSources(items[0])).toEqual(["Contract 2024-118", "Client sheet"]);
    expect(citedSources(items[1])).toEqual([]);
    expect(awaitedAccess(items[2])).toBe("send_email");
    expect(awaitedAccess(items[1])).toBeNull();
  });

  it("offers a tab per group that holds an item, with its real count", () => {
    // GIVEN items in five statuses
    // WHEN the tabs are listed, live then once everything ended
    const live = tabsFor(items);
    const ended = tabsFor(items.slice(0, 2));

    // THEN the item running counts as to process, and a finished batch drops that tab
    expect(live).toEqual([
      { key: "all", count: 5 },
      { key: "completed", count: 1 },
      { key: "failed", count: 1 },
      { key: "todo", count: 2 },
      { key: "awaiting", count: 1 },
    ]);
    expect(ended.map((tab) => tab.key)).toEqual(["all", "completed", "failed"]);
    expect(shownItemTab("todo", items.slice(0, 2))).toBe("all");
    expect(shownItemTab("failed", items)).toBe("failed");
  });

  it("narrows the rows by tab, then by what the input, the output or the sources hold", () => {
    // GIVEN the five items
    // WHEN a tab and a search are applied
    // THEN only the matching rows stay
    expect(filterItems(items, "failed", "").map((i) => i.rank)).toEqual([1]);
    expect(filterItems(items, "all", "contract").map((i) => i.rank)).toEqual([0]);
    expect(filterItems(items, "all", "client 4").map((i) => i.rank)).toEqual([3]);
    expect(filterItems(items, "completed", "image")).toEqual([]);
  });
});

describe("the history of the batches", () => {
  const many = (count: number): BatchSummary[] =>
    Array.from({ length: count }, (_, index) => batch({ batchId: `b-${index}` }));

  it("asks for one batch more than it shows, and grows with no ceiling", () => {
    // GIVEN the first three pages
    // WHEN their limits are computed
    // THEN each asks for its pages plus the one batch that says more exist
    expect([1, 2, 3].map(historyLimit)).toEqual([HISTORY_PAGE + 1, 2 * HISTORY_PAGE + 1, 3 * HISTORY_PAGE + 1]);
    expect(historyLimit(40)).toBe(40 * HISTORY_PAGE + 1);
  });

  it("knows an older batch exists from the extra one read", () => {
    // GIVEN a reading one batch longer than the page, and a shorter one
    // WHEN the page is cut
    // THEN the first shows a full page and more to come, the second everything
    expect(historyPage(many(HISTORY_PAGE + 1), 1)).toMatchObject({ hasMore: true });
    expect(historyPage(many(HISTORY_PAGE + 1), 1).batches).toHaveLength(HISTORY_PAGE);
    expect(historyPage(many(37), 1)).toMatchObject({ hasMore: false });
    expect(historyPage(many(37), 1).batches).toHaveLength(37);
  });

  it("reads past the first fifty batches when the user reads on", async () => {
    // GIVEN 120 batches behind the route
    const all = many(120);
    const list = vi.fn((limit: number) => Promise.resolve(all.slice(0, limit)));
    const history = createBatchHistory(ipcOf({ list }));

    // WHEN the history is read, then read on twice
    await history.load();
    const first = history.batches.length;
    await history.loadMore();
    await history.loadMore();

    // THEN every batch is shown, and nothing older is announced
    expect(first).toBe(HISTORY_PAGE);
    expect(history.batches).toHaveLength(120);
    expect(history.hasMore).toBe(false);
    expect(list.mock.calls.map(([limit]) => limit)).toEqual([51, 101, 151]);
  });

  it("names how a batch ended, and filters on it, on the agent and on the file", () => {
    // GIVEN four batches of two agents
    const batches = [
      batch({ batchId: "a", state: "completed", failed: 3 }),
      batch({ batchId: "b", state: "completed", failed: 0, agent: "watch", source: { kind: "csv", location: "/data/rivals.csv", options: {} } }),
      batch({ batchId: "c", state: "interrupted" }),
      batch({ batchId: "d", state: "paused" }),
    ];

    // WHEN they are read and filtered
    // THEN a completed batch with failures has its own outcome
    expect(batches.map(outcomeOf)).toEqual(["completed_with_failures", "completed", "interrupted", "paused"]);
    expect(agentsOf(batches)).toEqual(["triage", "watch"]);
    expect(filterBatches(batches, { agent: "watch", outcome: "", query: "" }).map((b) => b.batchId)).toEqual(["b"]);
    expect(filterBatches(batches, { agent: "", outcome: "completed_with_failures", query: "" }).map((b) => b.batchId)).toEqual(["a"]);
    expect(filterBatches(batches, { agent: "", outcome: "", query: "RIVALS" }).map((b) => b.batchId)).toEqual(["b"]);
  });

  it("splits the batches between this week and before, in the order read", () => {
    // GIVEN a Thursday, a batch of that Monday and one of the Sunday before
    const now = new Date(2026, 9, 8, 12, 0);
    const monday = batch({ batchId: "new", createdAt: new Date(2026, 9, 5, 9, 0).toISOString() });
    const sunday = batch({ batchId: "old", createdAt: new Date(2026, 9, 4, 23, 0).toISOString() });

    // WHEN they are grouped
    const groups = groupByPeriod([monday, sunday], now);

    // THEN the week starts on Monday, and an empty period is not listed
    expect(groups.map((g) => [g.period, g.batches.map((b) => b.batchId)])).toEqual([
      ["this_week", ["new"]],
      ["older", ["old"]],
    ]);
    expect(groupByPeriod([sunday], now).map((g) => g.period)).toEqual(["older"]);
  });

  it("names a source by its file and an export by its batch and format", () => {
    // GIVEN paths of both separators and a mailbox
    // WHEN they are named
    // THEN the file name alone is shown
    expect(sourceName("/data/requests.csv")).toBe("requests.csv");
    expect(sourceName("C:\\data\\requests.xlsx")).toBe("requests.xlsx");
    expect(sourceName("imap:support@example.org")).toBe("imap:support@example.org");
    expect(exportFileName(batch(), "xlsx")).toBe("batch-b-1.xlsx");
  });
});

describe("the live view of a batch", () => {
  const detail = (state: string): BatchDetail => ({ batch: batch({ state }), items: [item(0, "completed")] });

  it("reads the batch, applies a command, then reads it again", async () => {
    // GIVEN a running batch the runtime pauses on demand
    let state = "running";
    const get = vi.fn(() => Promise.resolve(detail(state)));
    const control = vi.fn((_id: string, action: string) => {
      state = action === "pause" ? "paused" : state;
      return Promise.resolve(batch({ state }));
    });
    const view = createBatchView(ipcOf({ get, control }));

    // WHEN it is loaded, then paused
    await view.load("b-1");
    const went = await view.act("pause");

    // THEN the view shows the batch paused, as read after the command
    expect(went).toBe(true);
    expect(control).toHaveBeenCalledWith("b-1", "pause");
    expect(view.detail?.batch.state).toBe("paused");
    expect(get).toHaveBeenCalledTimes(2);
    expect(view.busy).toBeNull();
  });

  it("shows the reason of a refused command and keeps the batch on screen", async () => {
    // GIVEN a stopped batch the runtime refuses to resume
    const view = createBatchView(
      ipcOf({
        get: () => Promise.resolve(detail("stopped")),
        control: () => Promise.reject(new Error("cannot resume batch 'b-1': it is stopped")),
      }),
    );
    await view.load("b-1");

    // WHEN it is resumed
    const went = await view.act("resume");

    // THEN the refusal is kept, and cleared on demand
    expect(went).toBe(false);
    expect(view.refusal).toContain("it is stopped");
    expect(view.detail?.batch.state).toBe("stopped");
    view.clearRefusal();
    expect(view.refusal).toBeNull();
  });

  it("tells a batch that does not exist from one that could not be read", async () => {
    // GIVEN an id no batch bears, then a runtime that does not answer
    const missing = createBatchView(ipcOf({ get: () => Promise.resolve(null) }));
    const unread = createBatchView(ipcOf({ get: () => Promise.reject(new Error("unreachable")) }));

    // WHEN both are loaded
    await missing.load("b-0");
    await unread.load("b-1");

    // THEN each has its own state
    expect(missing.missing).toBe(true);
    expect(missing.error).toBeNull();
    expect(unread.missing).toBe(false);
    expect(unread.error).toBe("unreachable");
  });

  it("exports through the shell to the path chosen, in the format asked", async () => {
    // GIVEN a loaded batch
    const exportTo = vi.fn((_id: string, _format: string, path: string) => Promise.resolve({ path, bytes: 12 }));
    const view = createBatchView(ipcOf({ get: () => Promise.resolve(detail("completed")), exportTo }));
    await view.load("b-1");

    // WHEN it is exported as CSV
    const written = await view.exportTo("csv", "/tmp/batch-b-1.csv");

    // THEN the shell wrote that file
    expect(written).toBe("/tmp/batch-b-1.csv");
    expect(exportTo).toHaveBeenCalledWith("b-1", "csv", "/tmp/batch-b-1.csv");
  });
});

describe("opening a batch", () => {
  it("opens the view under Agents, from a launch or from the history", () => {
    // GIVEN the application on another screen
    currentRoute.set("chat");

    // WHEN a launch opens its batch, then the history is opened
    openBatch("b-7", "launch");
    const opened = { route: get(currentRoute), batch: currentBatch() };
    openBatchHistory();

    // THEN both screens show, and the sidebar keeps its Batches entry marked
    // for each
    expect(opened).toEqual({ route: "batch", batch: { batchId: "b-7", origin: "launch" } });
    expect(get(currentRoute)).toBe("batches");
    expect(sidebarRoute("batch")).toBe("batches");
    expect(sidebarRoute("batches")).toBe("batches");
    expect(sidebarRoute("journal")).toBe("journal");
  });

});

describe("the notifications of the batches", () => {
  it("announces the end of a batch with its successes and its failures", () => {
    // GIVEN the end of a batch as the bridge emits it
    const ended = {
      event_type: "BatchEnded",
      payload: { BatchEnded: { batch_id: "b-1", agent_id: "triage", state: "completed", total: 140, completed: 136, failed: 4, canceled: 0, awaiting: 0 } },
    };

    // WHEN its notification is built
    // THEN it carries both counts, and the title follows how the batch ended
    expect(batchNotice(ended)).toEqual({
      titleKey: "notifications.native.batch_completed_title",
      bodyKey: "notifications.native.batch_ended_body",
      values: { agent: "triage", completed: 136, failed: 4, awaiting: 0 },
    });
    const interrupted = { ...ended, payload: { BatchEnded: { ...ended.payload.BatchEnded, state: "interrupted", awaiting: 2 } } };
    expect(batchNotice(interrupted)).toMatchObject({
      titleKey: "notifications.native.batch_interrupted_title",
      bodyKey: "notifications.native.batch_ended_body_waiting",
    });
  });

  it("announces an item that waits for a decision, and nothing for another event", () => {
    // GIVEN an item waiting on an access, and the completion of a run
    const waiting = {
      event_type: "BatchItemAwaitingApproval",
      payload: { BatchItemAwaitingApproval: { batch_id: "b-1", rank: 60, agent_id: "triage", access: "send_email" } },
    };

    // WHEN their notifications are built
    // THEN the item is numbered as the table numbers it
    expect(batchNotice(waiting)?.values).toEqual({ agent: "triage", number: 61, access: "send_email" });
    expect(batchNotice({ event_type: "TaskCompleted", payload: {} })).toBeNull();
  });

  it("is sent by the store of runtime events, through the plugin the approvals use", () => {
    // GIVEN the sources of the event store and of the notifications
    const store = readFileSync(new URL("../../../lib/stores/sse.ts", import.meta.url), "utf8");
    const notifications = readFileSync(new URL("../../../lib/stores/batchNotifications.ts", import.meta.url), "utf8");

    // WHEN the wiring is read
    // THEN both event categories of the batch engine reach the sender
    expect(store.match(/void notifyBatchEvent\(event\);/g)).toHaveLength(2);
    expect(notifications).toContain('from "@tauri-apps/plugin-notification"');
  });
});

describe("the answer of an item given as fields", () => {
  it("reads a JSON object, fenced or not, as its fields in order", () => {
    // GIVEN an answer fenced as JSON, and one in plain words
    const fenced = '```json\n{"Supplier": "Ada SA", "Total": 1200}\n```';

    // WHEN its fields are read
    const fields = answerFields(fenced);

    // THEN each field comes with its value, a number said as text
    expect(fields).toEqual([
      { name: "Supplier", value: "Ada SA" },
      { name: "Total", value: "1200" },
    ]);
    // AND an answer in plain words has none
    expect(answerFields("The quote fits the budget.")).toBeNull();
    expect(answerFields("{not json")).toBeNull();
  });
});

describe("the synthesis of a batch", () => {
  it("says it waits, then keeps the batch read until it is written", () => {
    // GIVEN a completed batch that asked for a synthesis not written yet
    const asked = { kind: "folder", location: "/data/quotes", options: {}, task: { instruction: "", outputs: [], synthesis: "Rank them" } };
    const waiting = batch({ state: "completed", source: asked });

    // WHEN its synthesis is read
    // THEN it waits and the batch is still read again
    expect(synthesisOf(waiting)).toEqual({ state: "waiting" });
    expect(isMoving(waiting)).toBe(true);

    // AND once written, it shows the text and the file, and reading stops
    const done = batch({
      state: "completed",
      source: { ...asked, synthesis: { state: "completed", text: "Ada SA first", path: "/data/quotes (synthesis 2026-10-18).md" } },
    });
    expect(synthesisOf(done)).toEqual({ state: "completed", text: "Ada SA first", path: "/data/quotes (synthesis 2026-10-18).md" });
    expect(isMoving(done)).toBe(false);
    // AND a batch that asked for none has no synthesis
    expect(synthesisOf(batch({ state: "completed" }))).toBeNull();
  });
});
