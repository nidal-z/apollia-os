// What the agent is doing, now, on each running item of a batch.
//
// An item runs as a task whose identifier is also the session identifier of
// its run, so the events the conversation reads its steps from (a tool call
// starts, ends, the answer is written) reach the interface with the task id of
// the item. Nothing here is stored: the state lives while the batch screen is
// open, and an item that ends is forgotten.

import { shortPath } from "$lib/chat/toolStep";

/** The step an item is at. */
export type LiveStep =
  | { kind: "tool"; tool: string; target: string | null; since: number }
  | { kind: "thinking"; since: number }
  | { kind: "writing"; since: number };

/** One event of the runtime as the interface receives it. */
export interface LiveEvent {
  event_type: string;
  payload: Record<string, unknown>;
}

/** The fields of a tool call whose value says what the call is about. */
const TARGET_FIELDS = ["path", "file", "dir", "url", "query", "pattern", "name"] as const;

/**
 * What a tool call is about, read from the start of its arguments: a path cut
 * to its end, an address or a search. `null` when the preview names none.
 */
export function previewTarget(preview: unknown): string | null {
  if (typeof preview !== "string" || preview.trim() === "") return null;
  let args: unknown;
  try {
    args = JSON.parse(preview);
  } catch {
    return null;
  }
  if (typeof args !== "object" || args === null) return null;
  for (const field of TARGET_FIELDS) {
    const value = (args as Record<string, unknown>)[field];
    if (typeof value === "string" && value.trim() !== "") {
      return field === "path" || field === "file" || field === "dir" ? shortPath(value) : value;
    }
  }
  return null;
}

/**
 * The fields of an event. A runtime event arrives tagged by its variant
 * (`{"ChatToolCallStarted": {...}}`), a token of the token channel arrives
 * flat: both read the same here.
 */
function fieldsOf(event: LiveEvent): Record<string, unknown> {
  const tagged = event.payload[event.event_type];
  return typeof tagged === "object" && tagged !== null ? (tagged as Record<string, unknown>) : event.payload;
}

/** The task an event is about, when it is one of `watched`. */
function taskOf(fields: Record<string, unknown>, watched: ReadonlySet<string>): string | null {
  const id = fields.session_id ?? fields.task_id;
  return typeof id === "string" && watched.has(id) ? id : null;
}

/**
 * The steps once `event` is read. An event about no watched item leaves them
 * as they are, and the same map comes back.
 */
export function applyLiveEvent(
  steps: ReadonlyMap<string, LiveStep>,
  event: LiveEvent,
  watched: ReadonlySet<string>,
  now: number,
): ReadonlyMap<string, LiveStep> {
  const fields = fieldsOf(event);
  const task = taskOf(fields, watched);
  if (task === null) return steps;
  const next = new Map(steps);
  switch (event.event_type) {
    case "ChatToolCallStarted": {
      const tool = typeof fields.tool_name === "string" ? fields.tool_name : "";
      next.set(task, { kind: "tool", tool, target: previewTarget(fields.input_preview), since: now });
      return next;
    }
    case "ChatToolCallCompleted":
      next.set(task, { kind: "thinking", since: now });
      return next;
    case "ChatToken": {
      // A token every few milliseconds: the step changes once, not per token.
      if (steps.get(task)?.kind === "writing") return steps;
      next.set(task, { kind: "writing", since: now });
      return next;
    }
    case "TaskCompleted":
    case "TaskCanceled":
      next.delete(task);
      return next;
    default:
      return steps;
  }
}

/** The steps of the items still running, the others dropped. */
export function keepRunning(
  steps: ReadonlyMap<string, LiveStep>,
  running: ReadonlySet<string>,
): ReadonlyMap<string, LiveStep> {
  if ([...steps.keys()].every((task) => running.has(task))) return steps;
  return new Map([...steps].filter(([task]) => running.has(task)));
}

/** Whole seconds since `since`, never negative. */
export function secondsSince(since: number, now: number): number {
  return Math.max(0, Math.floor((now - since) / 1000));
}

/** A duration read at a glance: seconds under a minute, then minutes and seconds. */
export function elapsedLabel(seconds: number): string {
  if (seconds < 60) return `${seconds} s`;
  const minutes = Math.floor(seconds / 60);
  const rest = seconds % 60;
  return rest === 0 ? `${minutes} min` : `${minutes} min ${rest} s`;
}
