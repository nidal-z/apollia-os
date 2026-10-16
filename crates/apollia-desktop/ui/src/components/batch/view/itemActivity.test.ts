import { describe, expect, test } from "vitest";
import type { TaskTimelineEvent } from "$lib/ipc/tasks";
import { EMPTY_ACTIVITY, activityFromTimeline, applyActivityEvent, splitThinking, type ItemActivity } from "./itemActivity";

const TASK = "task-7";

function tagged(variant: string, fields: Record<string, unknown>) {
  return { event_type: variant, payload: { [variant]: fields } };
}

function run(events: ReturnType<typeof tagged>[], from: ItemActivity = EMPTY_ACTIVITY): ItemActivity {
  return events.reduce((activity, event, index) => applyActivityEvent(activity, event, TASK, 1_000 * (index + 1)), from);
}

describe("the activity of a batch item", () => {
  test("a plan, its steps and a tool call read in the order they happened", () => {
    // GIVEN the events of a run that plans, reads a file and ends its step
    const events = [
      tagged("PlanGenerated", { task_id: TASK, step_count: 2 }),
      tagged("StepStarted", { task_id: TASK, step_id: "s1", step_num: 1, total: 2, desc: "Read the quote" }),
      tagged("ChatToolCallStarted", { session_id: TASK, tool_name: "file_read", input_preview: '{"path": "/a/Quotes/q-07.pdf"}' }),
      tagged("ChatToolCallCompleted", { session_id: TASK, tool_name: "file_read", success: true, output_preview: "QUOTE Q-07" }),
      tagged("StepCompleted", { task_id: TASK, step_id: "s1", duration_ms: 4200 }),
    ];

    // WHEN they are read
    const activity = run(events);

    // THEN the plan, the step, done, and the call with what it was given and gave back are there
    expect(activity.entries.map((entry) => entry.kind)).toEqual(["plan", "step", "tool"]);
    const [, step, tool] = activity.entries;
    expect(step).toMatchObject({ desc: "Read the quote", num: 1, total: 2, status: "done", durationMs: 4200 });
    expect(tool).toMatchObject({ tool: "file_read", target: "Quotes/q-07.pdf", output: "QUOTE Q-07", status: "done" });
  });

  test("what the model writes is kept as text once a tool or a step follows it", () => {
    // GIVEN tokens, then a tool call
    const events = [
      { event_type: "ChatToken", payload: { session_id: TASK, token: "<think>Look at " } },
      { event_type: "ChatToken", payload: { session_id: TASK, token: "the price</think>" } },
      tagged("ChatToolCallStarted", { session_id: TASK, tool_name: "file_read", input_preview: "" }),
    ];

    // WHEN they are read
    const activity = run(events as ReturnType<typeof tagged>[]);

    // THEN the written text became an entry before the call, and nothing is being written
    expect(activity.entries.map((entry) => entry.kind)).toEqual(["text", "tool"]);
    expect(activity.writing).toBe("");
  });

  test("a failed step keeps its cause, and an event of another run changes nothing", () => {
    // GIVEN a started step, then its failure, then an event of another task
    const before = run([tagged("StepStarted", { task_id: TASK, step_id: "s1", desc: "Read" })]);
    const failed = applyActivityEvent(before, tagged("StepFailed", { task_id: TASK, step_id: "s1", error: "unauthorized" }), TASK, 5_000);

    // WHEN an event about another run is read
    const after = applyActivityEvent(failed, tagged("StepStarted", { task_id: "other", step_id: "x" }), TASK, 6_000);

    // THEN the step says why it failed, and the other run left the activity as it was
    expect(failed.entries[0]).toMatchObject({ status: "failed", error: "unauthorized" });
    expect(after).toBe(failed);
  });

  test("a recorded timeline rebuilds the steps, the calls, the model answers and the approvals", () => {
    // GIVEN the timeline of an item that ran
    const timeline: TaskTimelineEvent[] = [
      { type: "step_started", step_id: "s1", tool: "file_read", input_preview: null, timestamp: "2026-10-16 10:00:01" },
      { type: "tool_call", tool_name: "file_read", duration_ms: 12, exit_code: null, success: true, truncated: false, input_preview: '{"path":"/q.pdf"}', output_preview: "QUOTE", timestamp: "2026-10-16 10:00:02" },
      { type: "step_completed", step_id: "s1", duration_ms: 900, success: true, timestamp: "2026-10-16 10:00:03" },
      { type: "llm_call", backend: "local", model: "m", prompt_tokens: 10, completion_tokens: 640, cost_usd: null, latency_ms: 7000, timestamp: "2026-10-16 10:00:10" },
      { type: "hitl_suspended", prompt: "Write synthese.md", timestamp: "2026-10-16 10:00:11" },
      { type: "hitl_resolved", approved: true, reason: null, wait_ms: 3000, timestamp: "2026-10-16 10:00:14" },
    ];

    // WHEN it is read
    const activity = activityFromTimeline(timeline);

    // THEN each happening is an entry, in order, with its outcome
    expect(activity.entries.map((entry) => entry.kind)).toEqual(["step", "tool", "model", "approval"]);
    expect(activity.entries[0]).toMatchObject({ status: "done", durationMs: 900 });
    expect(activity.entries[2]).toMatchObject({ tokens: 640, latencyMs: 7000 });
    expect(activity.entries[3]).toMatchObject({ approved: true });
  });

  test("a written text splits into its reasoning and its answer", () => {
    // GIVEN texts closed, still reasoning, and with no marker
    // WHEN they are split
    // THEN the reasoning and the answer are told apart
    expect(splitThinking("<think>Compare</think>Compliant.")).toEqual({ thinking: "Compare", answer: "Compliant." });
    expect(splitThinking("<think>Still comparing")).toEqual({ thinking: "Still comparing", answer: "" });
    expect(splitThinking("Compare first</think>Compliant.")).toEqual({ thinking: "Compare first", answer: "Compliant." });
    expect(splitThinking("Compliant.")).toEqual({ thinking: "", answer: "Compliant." });
  });

  test("the minute a plan takes reads as the agent preparing it, then as its thinking", () => {
    // GIVEN the planner's thinking events, under the task
    const events = [
      tagged("ThinkingStarted", { turn_id: TASK, ts_ms: 1 }),
      tagged("ThinkingEnded", { turn_id: TASK, ts_ms: 2, duration_ms: 16000, raw_content: "<think>Read the quote first</think>{}", tokens: 1616 }),
    ];

    // WHEN they are read
    const activity = run(events);

    // THEN one entry says the plan was prepared, with its time and its thinking
    expect(activity.entries).toHaveLength(1);
    expect(activity.entries[0]).toMatchObject({ kind: "planning", status: "done", durationMs: 16000, tokens: 1616 });
  });
});
