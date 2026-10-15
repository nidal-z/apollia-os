import { describe, expect, test } from "vitest";
import { applyLiveEvent, keepRunning, previewTarget, secondsSince, type LiveStep } from "./liveSteps";

const WATCHED = new Set(["task-7"]);

function event(event_type: string, payload: Record<string, unknown>) {
  return { event_type, payload };
}

describe("the live step of a running batch item", () => {
  test("a tool call shows its tool and what it is about", () => {
    // GIVEN no step yet for the item
    const steps = new Map<string, LiveStep>();
    // WHEN its run starts reading a file
    const next = applyLiveEvent(
      steps,
      event("ChatToolCallStarted", {
        session_id: "task-7",
        tool_name: "file_read",
        input_preview: '{"path": "/Users/ada/Documents/Quotes/quote-07-joinery.pdf"}',
      }),
      WATCHED,
      1_000,
    );
    // THEN the item reads that file, named by its end
    const step = next.get("task-7");
    expect(step?.kind).toBe("tool");
    expect(step && step.kind === "tool" ? step.tool : null).toBe("file_read");
    expect(step && step.kind === "tool" ? step.target : null).toContain("quote-07-joinery.pdf");
  });

  test("the end of a call is thinking, and the first token is writing, once", () => {
    // GIVEN an item whose call just ended
    let steps: ReadonlyMap<string, LiveStep> = applyLiveEvent(
      new Map(),
      event("ChatToolCallCompleted", { session_id: "task-7", tool_name: "file_read" }),
      WATCHED,
      2_000,
    );
    // WHEN two tokens of the answer arrive
    steps = applyLiveEvent(steps, event("ChatToken", { session_id: "task-7", token: "Con" }), WATCHED, 3_000);
    const afterSecond = applyLiveEvent(steps, event("ChatToken", { session_id: "task-7", token: "forme" }), WATCHED, 4_000);
    // THEN it thought, then writes since the first token, and the second
    // token changes nothing
    expect(steps.get("task-7")).toEqual({ kind: "writing", since: 3_000 });
    expect(afterSecond).toBe(steps);
  });

  test("a runtime event tagged by its variant reads like a flat one", () => {
    // GIVEN the envelope the desktop bridge sends: the event under its name
    const tagged = event("ChatToolCallStarted", {
      ChatToolCallStarted: { session_id: "task-7", tool_name: "source_search", input_preview: '{"query": "acompte"}' },
    });
    // WHEN it is read
    const step = applyLiveEvent(new Map(), tagged, WATCHED, 1_000).get("task-7");
    // THEN the item searches its sources for that
    expect(step).toEqual({ kind: "tool", tool: "source_search", target: "acompte", since: 1_000 });
  });

  test("an event about another conversation or task is ignored", () => {
    // GIVEN the steps of the batch
    const steps = new Map<string, LiveStep>([["task-7", { kind: "thinking", since: 0 }]]);
    // WHEN a tool call starts in a conversation
    const next = applyLiveEvent(
      steps,
      event("ChatToolCallStarted", { session_id: "conversation-1", tool_name: "web_search" }),
      WATCHED,
      5_000,
    );
    // THEN nothing changes
    expect(next).toBe(steps);
  });

  test("an item that ends, or stops running, is forgotten", () => {
    // GIVEN two items with a step
    const steps = new Map<string, LiveStep>([
      ["task-7", { kind: "thinking", since: 0 }],
      ["task-8", { kind: "writing", since: 0 }],
    ]);
    // WHEN the first completes, and the second is no longer running
    const ended = applyLiveEvent(steps, event("TaskCompleted", { task_id: "task-7" }), WATCHED, 6_000);
    const kept = keepRunning(ended, new Set());
    // THEN neither keeps a step
    expect(ended.has("task-7")).toBe(false);
    expect(kept.size).toBe(0);
  });

  test("a preview names its target only when it reads as arguments", () => {
    // GIVEN previews of a search, of an address, and two that name nothing
    // WHEN their target is read
    // THEN the search and the address come back as written
    expect(previewTarget('{"query": "payment on order"}')).toBe("payment on order");
    expect(previewTarget('{"url": "https://example.org/cga"}')).toBe("https://example.org/cga");
    expect(previewTarget('{"path": "/tmp/a.pdf", "trunc')).toBeNull();
    expect(previewTarget(undefined)).toBeNull();
  });

  test("the time on a step counts whole seconds, never below zero", () => {
    // GIVEN a step started at one second
    // WHEN it is read later, and earlier by a clock that moved back
    // THEN it says the whole seconds, and zero rather than less
    expect(secondsSince(1_000, 4_900)).toBe(3);
    expect(secondsSince(5_000, 4_000)).toBe(0);
  });
});
