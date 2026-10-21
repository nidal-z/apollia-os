import { describe, test, expect, vi } from "vitest";
import { get, writable } from "svelte/store";

const ipc = vi.hoisted(() => vi.fn(async (_cmd: string, _args?: unknown): Promise<unknown> => null));
vi.mock("@tauri-apps/api/core", () => ({ invoke: ipc }));

import { refreshPendingDecisions, toReviewCount } from "./pendingDecisions";

describe("a list of decisions that is not one", () => {
  test("is refused, and every other store of the interface still moves", async () => {
    // GIVEN the sidebar count subscribed, and a runtime that answers the list
    // of waiting decisions without its `items` (a bare array)
    const counts: number[] = [];
    const stopCount = toReviewCount.subscribe((count) => counts.push(count));
    ipc.mockResolvedValueOnce([]);

    // WHEN the list is read, then an unrelated store of the interface is set
    const read = refreshPendingDecisions();
    await expect(read).rejects.toThrow("list_pending_decisions");
    const route = writable("chat");
    const seen: string[] = [];
    const stopRoute = route.subscribe((value) => seen.push(value));
    route.set("integrations");

    // THEN the count stays at the last list published, and the other store
    // reaches its subscriber: `svelte/store` queues every notification in one
    // module-wide queue, and a subscriber that throws leaves it non-empty, so
    // a count computed on `undefined` would silence every store of the window
    expect({ counts, seen, current: get(toReviewCount) }).toEqual({
      counts: [0],
      seen: ["chat", "integrations"],
      current: 0,
    });
    stopRoute();
    stopCount();
  });

  test("a list with its items is published and counted", async () => {
    // GIVEN the count subscribed and a runtime that lists one waiting decision
    const stop = toReviewCount.subscribe(() => {});
    ipc.mockResolvedValueOnce({ items: [{ id: "d1" }] });

    // WHEN the list is read
    const listed = await refreshPendingDecisions();

    // THEN the decision is returned and counted
    expect({ ids: listed.map((d) => d.id), count: get(toReviewCount) }).toEqual({ ids: ["d1"], count: 1 });
    stop();
  });
});
