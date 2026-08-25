// Typed IPC wrappers for the Tasks surface.
//
// Keeps every `invoke` for the tasks route in one place, so the `.svelte`
// files call typed helpers instead of stringly-typed Tauri commands. Each
// helper maps to a `#[tauri::command]` in `crates/apollia-desktop/src/commands/tasks.rs`.

import { invoke } from "@tauri-apps/api/core";
import type { TaskSummary } from "$lib/types";

/** Fetches the task list; `filter` narrows it server-side (null = everything). */
export async function listTasks(filter: unknown = null): Promise<TaskSummary[]> {
  return invoke<TaskSummary[]>("list_tasks", { filter });
}

/**
 * Cancels an in-flight task. Maps to `DELETE /api/v1/tasks/{id}` on the runtime.
 * Resolves `true` when cancelled, `false` when the task had already finished.
 */
export async function cancelTask(taskId: string): Promise<boolean> {
  return invoke<boolean>("cancel_task", { taskId });
}

/**
 * Re-submits a task's input to its agent, yielding a fresh task id. Used by the
 * "retry" affordance on a failed or cancelled task.
 */
export async function submitTask(agentId: string, input: string): Promise<string> {
  return invoke<string>("submit_task", { agentId, input });
}

/**
 * Hard-removes a task record. Distinct from {@link cancelTask}: cancel stops an
 * in-flight run, whereas delete erases the record from history and is
 * irreversible. Resolves `true` when a row was removed, `false` otherwise.
 */
export async function deleteTask(taskId: string): Promise<boolean> {
  return invoke<boolean>("delete_task", { taskId });
}

// ───────────────────────────────────────────────────────────────────────────
// Task timeline
// ───────────────────────────────────────────────────────────────────────────

/**
 * One event of a single task's execution timeline.
 *
 * Mirrors the `TimelineEvent` enum served by `GET /api/v1/tasks/{id}/timeline`
 * and relayed verbatim by the `get_task_timeline` command: an internally tagged
 * union whose discriminant is `type` in snake_case. The runtime aggregates five
 * SQLite sources (task transitions, plan steps, LLM calls, tool invocations,
 * HITL approvals) and sorts the result by timestamp ascending.
 *
 * Every field below exists on the wire. `input_preview` and `output_preview` on
 * `tool_call` are the only ones the runtime omits when absent, hence optional
 * rather than nullable.
 */
export type TaskTimelineEvent =
  | { type: "task_transition"; status: string; timestamp: string }
  | {
      type: "step_started";
      step_id: string;
      tool: string | null;
      input_preview: string | null;
      timestamp: string;
    }
  | {
      type: "step_completed";
      step_id: string;
      duration_ms: number | null;
      success: boolean;
      timestamp: string;
    }
  | {
      type: "llm_call";
      backend: string;
      model: string;
      prompt_tokens: number | null;
      completion_tokens: number | null;
      cost_usd: number | null;
      latency_ms: number | null;
      timestamp: string;
    }
  | {
      type: "tool_call";
      tool_name: string;
      duration_ms: number | null;
      exit_code: number | null;
      truncated: boolean;
      input_preview?: string;
      output_preview?: string;
      timestamp: string;
    }
  | { type: "hitl_suspended"; prompt: string; timestamp: string }
  | {
      type: "hitl_resolved";
      approved: boolean;
      reason: string | null;
      wait_ms: number | null;
      timestamp: string;
    }
  | {
      type: "task_completed";
      output_preview: string | null;
      duration_ms: number | null;
      timestamp: string;
    };

/** Discriminants the viewer knows how to render. */
const TIMELINE_EVENT_TYPES: ReadonlySet<string> = new Set([
  "task_transition",
  "step_started",
  "step_completed",
  "llm_call",
  "tool_call",
  "hitl_suspended",
  "hitl_resolved",
  "task_completed",
]);

/**
 * Narrows one raw timeline entry.
 *
 * The command hands the JSON over untouched, so an entry emitted by a newer
 * runtime than this build is dropped rather than rendered as a blank row.
 * Exported for unit tests.
 */
export function isTaskTimelineEvent(raw: unknown): raw is TaskTimelineEvent {
  if (typeof raw !== "object" || raw === null) return false;
  const candidate = raw as { type?: unknown; timestamp?: unknown };
  return (
    typeof candidate.type === "string" &&
    TIMELINE_EVENT_TYPES.has(candidate.type) &&
    typeof candidate.timestamp === "string"
  );
}

/**
 * Reads the aggregated timeline of a single task, oldest event first.
 *
 * Rejects when the task is unknown to every source: the runtime answers 404 and
 * the command turns it into an error string.
 */
export async function getTaskTimeline(
  taskId: string,
): Promise<TaskTimelineEvent[]> {
  const raw = await invoke<unknown[]>("get_task_timeline", { taskId });
  return Array.isArray(raw) ? raw.filter(isTaskTimelineEvent) : [];
}
