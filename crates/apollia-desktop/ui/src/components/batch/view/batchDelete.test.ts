import { describe, expect, it } from "vitest";
import type { BatchItem, BatchSummary } from "$lib/ipc/batches";
import { deleteBlockOf } from "./batchDelete";

function batch(state: string): BatchSummary {
  return {
    batchId: "b-1",
    agent: "clerk",
    source: { kind: "spreadsheet", location: "/data/requests.xlsx", options: {} },
    grants: [],
    concurrency: 1,
    maxFailures: 10,
    state,
    total: 2,
    completed: 1,
    failed: 0,
    canceled: 1,
    awaiting: 0,
    createdAt: "2026-10-15T09:00:00Z",
    updatedAt: "2026-10-15T09:05:00Z",
    endedAt: "2026-10-15T09:05:00Z",
  } as BatchSummary;
}

function item(rank: number, status: string): BatchItem {
  return { rank, origin: `row-${rank + 2}`, input: {}, status, output: null, sources: null, taskId: null } as BatchItem;
}

describe("deleteBlockOf", () => {
  it("blocks a running and a paused batch by their state", () => {
    // GIVEN a running batch and a paused one
    const running = batch("running");
    const paused = batch("paused");

    // WHEN their deletion is asked about
    const blocks = [deleteBlockOf(running), deleteBlockOf(paused)];

    // THEN each is blocked by its own state
    expect(blocks).toEqual(["running", "paused"]);
  });

  it("blocks an ended batch while one of its items still runs", () => {
    // GIVEN a stopped batch whose second item is still ending
    const stopped = batch("stopped");
    const items = [item(0, "completed"), item(1, "running")];

    // WHEN its deletion is asked about with its items, then without them
    const withItems = deleteBlockOf(stopped, items);
    const withoutItems = deleteBlockOf(stopped);

    // THEN the items in flight block it, and the state alone does not
    expect(withItems).toBe("in_flight");
    expect(withoutItems).toBeNull();
  });

  it("lets every ended batch with no item running be deleted", () => {
    // GIVEN a completed, an interrupted and a stopped batch, none of their items running
    const items = [item(0, "completed"), item(1, "canceled")];

    // WHEN their deletion is asked about
    const blocks = ["completed", "interrupted", "stopped"].map((state) => deleteBlockOf(batch(state), items));

    // THEN none is blocked
    expect(blocks).toEqual([null, null, null]);
  });
});
