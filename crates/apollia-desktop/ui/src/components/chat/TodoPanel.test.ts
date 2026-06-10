import { describe, test, expect } from "vitest";
import {
  TODO_SECTION_ORDER,
  bucketTodos,
  todoErrorMessage,
  resolveTodoUpdate,
} from "./TodoPanel.svelte";
import type { TodoItem, TodoUpdatedPayload } from "$lib/ipc/todo";

// The repo has no DOM test environment (vitest runs in `node`), so the rendered
// markup is exercised by the Playwright E2E layer. These unit tests lock the
// pure logic the component relies on: status bucketing, section ordering, the
// session-scoped live-update filter, and error normalization.

function item(id: string, status: TodoItem["status"]): TodoItem {
  return { id, content: `work ${id}`, status, depends_on: [] };
}

describe("TodoPanel - bucketTodos", () => {
  test("groups items into the three status buckets", () => {
    // GIVEN 2 pending, 1 in_progress, 1 completed
    const items = [
      item("s1", "pending"),
      item("s2", "pending"),
      item("s3", "in_progress"),
      item("s4", "completed"),
    ];

    // WHEN bucketing
    const buckets = bucketTodos(items);

    // THEN each bucket holds the matching items
    expect(buckets.pending.map((i) => i.id)).toEqual(["s1", "s2"]);
    expect(buckets.in_progress.map((i) => i.id)).toEqual(["s3"]);
    expect(buckets.completed.map((i) => i.id)).toEqual(["s4"]);
  });

  test("an empty list yields three empty buckets", () => {
    // GIVEN no items
    // WHEN bucketing
    const buckets = bucketTodos([]);

    // THEN every bucket is empty (drives the empty-state branch)
    expect(buckets.pending).toHaveLength(0);
    expect(buckets.in_progress).toHaveLength(0);
    expect(buckets.completed).toHaveLength(0);
  });

  test("section order renders in_progress, then pending, then completed", () => {
    // GIVEN the documented display order
    // THEN the constant matches it exactly
    expect(TODO_SECTION_ORDER).toEqual(["in_progress", "pending", "completed"]);
  });
});

describe("TodoPanel - resolveTodoUpdate", () => {
  test("applies a payload that targets the panel session", () => {
    // GIVEN a panel bound to session s-1
    const payload: TodoUpdatedPayload = {
      session_id: "s-1",
      items: [item("s1", "in_progress")],
    };

    // WHEN a matching update arrives
    const next = resolveTodoUpdate(payload, "s-1");

    // THEN the new snapshot is returned (s1 now in_progress)
    expect(next).not.toBeNull();
    expect(next?.[0]?.status).toBe("in_progress");
  });

  test("ignores a payload from another session", () => {
    // GIVEN a panel bound to session s-1
    const payload: TodoUpdatedPayload = {
      session_id: "s-2",
      items: [item("x", "pending")],
    };

    // WHEN an update for s-2 arrives
    const next = resolveTodoUpdate(payload, "s-1");

    // THEN it is ignored, keeping the current list untouched
    expect(next).toBeNull();
  });
});

describe("TodoPanel - todoErrorMessage", () => {
  test("unwraps an Error to its message", () => {
    // GIVEN a rejected initial read
    // WHEN normalizing the Error
    // THEN the message is surfaced verbatim
    expect(todoErrorMessage(new Error("ECONNREFUSED"))).toBe("ECONNREFUSED");
  });

  test("stringifies a non-Error rejection", () => {
    // GIVEN a non-Error thrown value (e.g. a string from a Tauri bridge)
    // WHEN normalizing it
    // THEN it is coerced to a string without throwing
    expect(todoErrorMessage("connection lost")).toBe("connection lost");
  });
});
