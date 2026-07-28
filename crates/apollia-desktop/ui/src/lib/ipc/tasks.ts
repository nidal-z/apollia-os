// Typed IPC wrappers for the Tasks surface.
//
// Keeps every `invoke` for the tasks route in one place, so the `.svelte`
// files call typed helpers instead of stringly-typed Tauri commands. Each
// helper maps to a `#[tauri::command]` in `crates/apollia-desktop/src/commands/tasks.rs`.

import { invoke } from "@tauri-apps/api/core";
import type { TaskSummary } from "$lib/types";

/** Fetches the full task list (no server-side filter; the UI filters locally). */
export async function listTasks(): Promise<TaskSummary[]> {
  return invoke<TaskSummary[]>("list_tasks", { filter: null });
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
export async function retryTask(agentId: string, input: string): Promise<string> {
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
