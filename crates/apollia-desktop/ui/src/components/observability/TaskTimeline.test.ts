import { describe, test, expect } from "vitest";
import { formatMs, toTimelineRow } from "./TaskTimeline.svelte";
import { formatCost } from "$lib/format";
import { isTaskTimelineEvent, type TaskTimelineEvent } from "$lib/ipc/tasks";

// DOM rendering is exercised by the gestural E2E layer (vitest runs in `node`).
// These tests lock the wire-to-row projection: tone, title key and facts drive
// the icon, the colour token and everything the operator reads on a row.

describe("TaskTimeline - toTimelineRow", () => {
  test("a failed status transition tints the row", () => {
    // GIVEN a transition to the failed status
    const event: TaskTimelineEvent = {
      type: "task_transition",
      status: "failed",
      timestamp: "2026-08-05T10:00:00Z",
    };

    // WHEN projecting it
    const row = toTimelineRow(event);

    // THEN the row is a task-toned failure carrying the status verbatim
    expect(row.tone).toBe("task");
    expect(row.failed).toBe(true);
    expect(row.titleValues).toEqual({ status: "failed" });
  });

  test("a non-zero exit code turns a tool call into a failure", () => {
    // GIVEN a tool invocation that exited with 1
    const event: TaskTimelineEvent = {
      type: "tool_call",
      tool_name: "bash",
      duration_ms: 1500,
      exit_code: 1,
      truncated: true,
      output_preview: "command not found",
      timestamp: "2026-08-05T10:00:01Z",
    };

    // WHEN projecting it
    const row = toTimelineRow(event);

    // THEN it reads as a failure, keeps the output preview and the cut flag
    expect(row.failed).toBe(true);
    expect(row.titleKey).toBe("observability.task_timeline_row_tool_call_failed");
    expect(row.preview).toBe("command not found");
    expect(row.truncated).toBe(true);
    expect(row.facts).toEqual([
      { labelKey: "observability.task_timeline_fact_duration", value: "1.5s" },
      { labelKey: "observability.task_timeline_fact_exit_code", value: "1" },
    ]);
  });

  test("an MCP tool call with no exit code is not a failure", () => {
    // GIVEN a tool invocation the runtime recorded without an exit code
    const event: TaskTimelineEvent = {
      type: "tool_call",
      tool_name: "mcp.search",
      duration_ms: null,
      exit_code: null,
      truncated: false,
      timestamp: "2026-08-05T10:00:02Z",
    };

    // WHEN projecting it
    const row = toTimelineRow(event);

    // THEN it stays neutral and carries no fact at all
    expect(row.failed).toBe(false);
    expect(row.titleKey).toBe("observability.task_timeline_row_tool_call");
    expect(row.preview).toBeNull();
    expect(row.facts).toEqual([]);
  });

  test("a model call reports backend, tokens, cost and latency", () => {
    // GIVEN a recorded LLM call with every optional field present
    const event: TaskTimelineEvent = {
      type: "llm_call",
      backend: "local",
      model: "qwen3",
      prompt_tokens: 120,
      completion_tokens: 40,
      cost_usd: 0.0025,
      latency_ms: 800,
      timestamp: "2026-08-05T10:00:03Z",
    };

    // WHEN projecting it
    const row = toTimelineRow(event);

    // THEN the four facts appear in reading order, cost kept at sub-cent detail
    expect(row.tone).toBe("llm");
    expect(row.facts.map((fact) => fact.value)).toEqual([
      "local",
      "120 / 40",
      "$0.0025",
      "800ms",
    ]);
  });

  test("a rejected approval is a failure carrying the operator reason", () => {
    // GIVEN a HITL resolution the operator rejected
    const event: TaskTimelineEvent = {
      type: "hitl_resolved",
      approved: false,
      reason: "out of scope",
      wait_ms: 45_000,
      timestamp: "2026-08-05T10:00:04Z",
    };

    // WHEN projecting it
    const row = toTimelineRow(event);

    // THEN it reads as a rejection with the wait time and the reason
    expect(row.titleKey).toBe("observability.task_timeline_row_hitl_rejected");
    expect(row.failed).toBe(true);
    expect(row.preview).toBe("out of scope");
    expect(row.facts).toEqual([
      { labelKey: "observability.task_timeline_fact_wait", value: "45.0s" },
    ]);
  });

  test("the terminal event tolerates the empty timestamp the runtime may emit", () => {
    // GIVEN a completion of a task with no other recorded event: the runtime
    // derives the terminal timestamp from the last sibling, so it is empty
    const event: TaskTimelineEvent = {
      type: "task_completed",
      output_preview: null,
      duration_ms: null,
      timestamp: "",
    };

    // WHEN projecting it
    const row = toTimelineRow(event);

    // THEN nothing is fabricated and the row stays renderable
    expect(row.timestamp).toBe("");
    expect(row.facts).toEqual([]);
    expect(row.preview).toBeNull();
  });
});

describe("TaskTimeline - formatting", () => {
  test("durations switch from milliseconds to seconds at one second", () => {
    // GIVEN durations on both sides of the threshold
    // WHEN formatting them
    // THEN the unit follows the audit table convention
    expect(formatMs(999)).toBe("999ms");
    expect(formatMs(1000)).toBe("1.0s");
  });

  test("sub-cent costs keep four decimals", () => {
    // GIVEN a cheap call and an expensive one
    // WHEN formatting them
    // THEN the cheap one stays readable instead of collapsing to $0.00
    expect(formatCost(0.0004)).toBe("$0.0004");
    expect(formatCost(1.5)).toBe("$1.50");
  });
});

describe("tasks IPC - isTaskTimelineEvent", () => {
  test("an entry from an unknown variant is rejected", () => {
    // GIVEN a tagged entry this build does not know how to render
    const raw = { type: "quantum_leap", timestamp: "2026-08-05T10:00:00Z" };

    // WHEN narrowing it
    // THEN it is dropped rather than rendered as a blank row
    expect(isTaskTimelineEvent(raw)).toBe(false);
  });

  test("a known variant with a timestamp is accepted", () => {
    // GIVEN a well-formed transition entry
    const raw = { type: "task_transition", status: "working", timestamp: "t" };

    // WHEN narrowing it
    // THEN it passes
    expect(isTaskTimelineEvent(raw)).toBe(true);
  });

  test("null and primitives are rejected", () => {
    // GIVEN payloads that are not objects
    // WHEN narrowing them
    // THEN none passes
    expect(isTaskTimelineEvent(null)).toBe(false);
    expect(isTaskTimelineEvent("task_transition")).toBe(false);
  });
});
