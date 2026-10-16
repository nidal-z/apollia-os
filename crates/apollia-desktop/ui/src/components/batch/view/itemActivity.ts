// What the agent did, and is doing, on one item of a batch, read as the
// conversation reads a turn: its plan and steps, each tool with what it was
// given and what it gave back, what the model wrote, the accesses it asked for.
//
// The activity is rebuilt from the recorded timeline of the item's task when
// the item is opened, then follows the events of its run while it runs. An
// item runs as a task whose identifier is also the session identifier of its
// run, so the tool and token events carry the task id as `session_id`, and the
// plan events carry it as `task_id`.

import { parseTimestampMs } from "$lib/utils";
import type { TaskTimelineEvent } from "$lib/ipc/tasks";
import { previewTarget, type LiveEvent } from "./liveSteps";

/** One thing the agent did, in the order it did it. */
export type ActivityEntry =
  | {
      kind: "planning";
      status: "running" | "done";
      thinking: string;
      tokens: number | null;
      durationMs: number | null;
      at: number;
    }
  | { kind: "plan"; steps: number; at: number }
  | {
      kind: "step";
      stepId: string;
      num: number | null;
      total: number | null;
      desc: string;
      /** What the step's tool was about, when the record names it. */
      target: string | null;
      status: "running" | "done" | "failed";
      error: string | null;
      durationMs: number | null;
      at: number;
    }
  | {
      kind: "tool";
      tool: string;
      target: string | null;
      input: string | null;
      output: string | null;
      status: "running" | "done" | "failed";
      durationMs: number | null;
      at: number;
    }
  | { kind: "text"; text: string; at: number }
  | { kind: "model"; tokens: number | null; latencyMs: number | null; at: number }
  | { kind: "replan"; reason: string; at: number }
  | { kind: "approval"; prompt: string; approved: boolean | null; at: number };

export interface ItemActivity {
  entries: ActivityEntry[];
  /** What the model is writing now, before a tool or a step closes it. */
  writing: string;
}

export const EMPTY_ACTIVITY: ItemActivity = { entries: [], writing: "" };

function at(timestamp: string): number {
  const value = parseTimestampMs(timestamp);
  return Number.isNaN(value) ? 0 : value;
}

/** The activity a recorded timeline tells, oldest first. */
export function activityFromTimeline(timeline: readonly TaskTimelineEvent[]): ItemActivity {
  const entries: ActivityEntry[] = [];
  for (const event of timeline) {
    switch (event.type) {
      case "step_started":
        entries.push({
          kind: "step",
          stepId: event.step_id,
          num: null,
          total: null,
          desc: event.tool ?? event.step_id,
          target: previewTarget(event.input_preview),
          status: "running",
          error: null,
          durationMs: null,
          at: at(event.timestamp),
        });
        break;
      case "step_completed": {
        const step = lastStep(entries, event.step_id);
        if (step !== null) {
          step.status = event.success ? "done" : "failed";
          step.durationMs = event.duration_ms;
        }
        break;
      }
      case "tool_call":
        entries.push({
          kind: "tool",
          tool: event.tool_name,
          target: previewTarget(event.input_preview),
          input: event.input_preview ?? null,
          output: event.output_preview ?? null,
          status: event.success === false ? "failed" : event.success === true ? "done" : "running",
          durationMs: event.duration_ms,
          at: at(event.timestamp),
        });
        break;
      case "llm_call":
        entries.push({
          kind: "model",
          tokens: event.completion_tokens,
          latencyMs: event.latency_ms,
          at: at(event.timestamp),
        });
        break;
      case "hitl_suspended":
        entries.push({ kind: "approval", prompt: event.prompt, approved: null, at: at(event.timestamp) });
        break;
      case "hitl_resolved": {
        const asked = [...entries].reverse().find((entry) => entry.kind === "approval" && entry.approved === null);
        if (asked !== undefined && asked.kind === "approval") asked.approved = event.approved;
        break;
      }
      default:
        break;
    }
  }
  // A step the record never closed did not run on.
  return { entries: entries.sort((a, b) => a.at - b.at), writing: "" };
}

function lastStep(entries: ActivityEntry[], stepId: string) {
  for (let index = entries.length - 1; index >= 0; index -= 1) {
    const entry = entries[index];
    if (entry.kind === "step" && entry.stepId === stepId) return entry;
  }
  return null;
}

function lastRunningTool(entries: ActivityEntry[], tool: string) {
  for (let index = entries.length - 1; index >= 0; index -= 1) {
    const entry = entries[index];
    if (entry.kind === "tool" && entry.tool === tool && entry.status === "running") return entry;
  }
  return null;
}

/** The fields of an event, tagged by its variant or flat. */
function fieldsOf(event: LiveEvent): Record<string, unknown> {
  const tagged = event.payload[event.event_type];
  return typeof tagged === "object" && tagged !== null ? (tagged as Record<string, unknown>) : event.payload;
}

function text(value: unknown): string {
  return typeof value === "string" ? value : "";
}

function count(value: unknown): number | null {
  return typeof value === "number" && Number.isFinite(value) ? value : null;
}

/** The written text kept as an entry before something else happens. */
function settle(entries: ActivityEntry[], writing: string, now: number): void {
  if (writing.trim() !== "") entries.push({ kind: "text", text: writing, at: now });
}

/**
 * The activity once `event` is read, for the item run by `task`. An event
 * about another run leaves it as it is, and the same object comes back.
 */
export function applyActivityEvent(
  activity: ItemActivity,
  event: LiveEvent,
  task: string,
  now: number,
): ItemActivity {
  const fields = fieldsOf(event);
  const about = fields.session_id ?? fields.task_id ?? fields.turn_id;
  if (about !== task) return activity;

  const entries = activity.entries.map((entry) => ({ ...entry }));
  switch (event.event_type) {
    case "ChatToken":
      return { entries: activity.entries, writing: activity.writing + text(fields.token) };
    case "ThinkingStarted":
      settle(entries, activity.writing, now);
      entries.push({ kind: "planning", status: "running", thinking: "", tokens: null, durationMs: null, at: now });
      return { entries, writing: "" };
    case "ThinkingEnded": {
      const planning = [...entries].reverse().find((entry) => entry.kind === "planning" && entry.status === "running");
      if (planning === undefined || planning.kind !== "planning") return activity;
      planning.status = "done";
      planning.thinking = text(fields.raw_content);
      planning.tokens = count(fields.tokens);
      planning.durationMs = count(fields.duration_ms) ?? now - planning.at;
      return { entries, writing: activity.writing };
    }
    case "PlanGenerated":
      settle(entries, activity.writing, now);
      entries.push({ kind: "plan", steps: count(fields.step_count) ?? 0, at: now });
      return { entries, writing: "" };
    case "StepStarted":
      settle(entries, activity.writing, now);
      entries.push({
        kind: "step",
        stepId: text(fields.step_id),
        num: count(fields.step_num),
        total: count(fields.total),
        desc: text(fields.desc) || text(fields.step_id),
        target: null,
        status: "running",
        error: null,
        durationMs: null,
        at: now,
      });
      return { entries, writing: "" };
    case "StepCompleted":
    case "StepFailed": {
      settle(entries, activity.writing, now);
      const step = lastStep(entries, text(fields.step_id));
      if (step !== null) {
        step.status = event.event_type === "StepCompleted" ? "done" : "failed";
        step.durationMs = count(fields.duration_ms) ?? now - step.at;
        step.error = event.event_type === "StepFailed" ? text(fields.error) || null : null;
      }
      return { entries, writing: "" };
    }
    case "PlanReplanning":
      settle(entries, activity.writing, now);
      entries.push({ kind: "replan", reason: text(fields.reason), at: now });
      return { entries, writing: "" };
    case "ChatToolCallStarted":
      settle(entries, activity.writing, now);
      entries.push({
        kind: "tool",
        tool: text(fields.tool_name),
        target: previewTarget(fields.input_preview),
        input: text(fields.input_preview) || null,
        output: null,
        status: "running",
        durationMs: null,
        at: now,
      });
      return { entries, writing: "" };
    case "ChatToolCallCompleted": {
      const call = lastRunningTool(entries, text(fields.tool_name));
      if (call === null) return activity;
      call.status = fields.success === false ? "failed" : "done";
      call.output = text(fields.output_preview) || null;
      call.durationMs = now - call.at;
      return { entries, writing: activity.writing };
    }
    case "ChatApprovalRequired":
      settle(entries, activity.writing, now);
      entries.push({ kind: "approval", prompt: text(fields.prompt) || text(fields.tool_name), approved: null, at: now });
      return { entries, writing: "" };
    default:
      return activity;
  }
}

/**
 * A written text split into the reasoning the model wrote first, between
 * think markers, and the answer that follows. An unclosed marker means the
 * model is still reasoning.
 */
export function splitThinking(written: string): { thinking: string; answer: string } {
  const open = written.indexOf("<think>");
  if (open === -1) {
    const close = written.indexOf("</think>");
    // Some templates open the reasoning before the stream starts.
    if (close === -1) return { thinking: "", answer: written.trim() };
    return { thinking: written.slice(0, close).trim(), answer: written.slice(close + 8).trim() };
  }
  const close = written.indexOf("</think>", open);
  const before = written.slice(0, open);
  if (close === -1) return { thinking: written.slice(open + 7).trim(), answer: before.trim() };
  return {
    thinking: written.slice(open + 7, close).trim(),
    answer: (before + written.slice(close + 8)).trim(),
  };
}
