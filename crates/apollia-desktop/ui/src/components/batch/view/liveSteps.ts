// What the agent is doing, now, on each running item of a batch.
//
// An item runs as a task whose identifier is also the session identifier of
// its run, so the events the conversation reads its steps from (a tool call
// starts, ends, the answer is written) reach the interface with the task id of
// the item. Nothing here is stored: the state lives while the batch screen is
// open, and an item that ends is forgotten.

import { shortPath, stepActionKey } from "$lib/chat/toolStep";
import { parseTimestampMs } from "$lib/utils";

/** The step of its plan an item is at, when its agent planned. */
export interface PlanStep {
  num: number;
  total: number;
  desc: string;
}

/** The step an item is at, and the step of its plan. */
export type LiveStep = (
  | { kind: "tool"; tool: string; target: string | null; since: number }
  | { kind: "thinking"; since: number }
  | { kind: "writing"; since: number }
  | { kind: "planning"; since: number }
) & { plan?: PlanStep };

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
  const id = fields.session_id ?? fields.task_id ?? fields.turn_id;
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
  const plan = steps.get(task)?.plan;
  const kept = plan === undefined ? {} : { plan };
  switch (event.event_type) {
    case "ThinkingStarted":
      next.set(task, { kind: "planning", since: now, ...kept });
      return next;
    case "ThinkingEnded":
      next.set(task, { kind: "thinking", since: now, ...kept });
      return next;
    case "StepStarted": {
      const num = typeof fields.step_num === "number" ? fields.step_num : 0;
      const total = typeof fields.total === "number" ? fields.total : 0;
      const desc = typeof fields.desc === "string" ? fields.desc : "";
      next.set(task, { kind: "thinking", since: now, ...(num > 0 && total > 0 ? { plan: { num, total, desc } } : {}) });
      return next;
    }
    case "ChatToolCallStarted": {
      const tool = typeof fields.tool_name === "string" ? fields.tool_name : "";
      next.set(task, { kind: "tool", tool, target: previewTarget(fields.input_preview), since: now, ...kept });
      return next;
    }
    case "ChatToolCallCompleted":
      next.set(task, { kind: "thinking", since: now, ...kept });
      return next;
    case "ChatToken": {
      // A token every few milliseconds: the step changes once, not per token.
      if (steps.get(task)?.kind === "writing") return steps;
      next.set(task, { kind: "writing", since: now, ...kept });
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

/** A catalogue lookup, as `$t` answers it. */
export type Translate = (key: string, options?: { values: Record<string, string | number> }) => string;

/**
 * What the agent of a running item is doing and for how long, in one line:
 * the step of its plan when it planned, then the tool and its target, its
 * thinking or its writing. Before anything was reported since the screen
 * opened, it works since its run started.
 */
export function liveLineOf(
  item: { taskId: string | null; updatedAt?: string },
  step: LiveStep | undefined,
  now: number,
  tr: Translate,
): string {
  let doing: string;
  let since: number;
  if (step === undefined) {
    doing = tr("batch_view.live.working");
    since = item.updatedAt === undefined ? now : parseTimestampMs(item.updatedAt);
  } else if (step.kind === "tool") {
    const key = stepActionKey(step.tool);
    const action = key === null ? tr("batch_view.live.tool", { values: { tool: step.tool } }) : tr(key);
    doing = step.target === null ? action : `${action} · ${step.target}`;
    since = step.since;
  } else {
    doing = tr(
      step.kind === "planning"
        ? "batch_view.live.planning"
        : step.kind === "thinking"
          ? "batch_view.live.thinking"
          : "batch_view.live.writing",
    );
    since = step.since;
  }
  if (step?.plan !== undefined) {
    const of = tr("batch_view.activity.step_of", { values: { num: step.plan.num, total: step.plan.total } });
    doing = step.plan.desc === "" ? `${of} · ${doing}` : `${of} · ${step.plan.desc} · ${doing}`;
  }
  return tr("batch_view.live.line", { values: { doing, elapsed: elapsedLabel(secondsSince(since, now)) } });
}
