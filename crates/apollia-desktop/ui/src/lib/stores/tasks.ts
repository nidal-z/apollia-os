/**
 * Derived task stores for Apollia Desktop.
 *
 * Re-exports the base tasks store from sse.ts and provides derived
 * stores for filtered views (running tasks, completed count, etc.).
 */
import { derived } from "svelte/store";
import { tasks } from "./sse";

export { tasks } from "./sse";

/** Tasks currently in 'working' state. */
export const runningTasks = derived(tasks, ($tasks) =>
  $tasks.filter((t) => t.status === "working"),
);

/**
 * Count of tasks considered "in-flight". The runtime exposes `submitted`
 * (queued, waiting for the actor pool) and `working` (actively running); both
 * count, so the figure is the real backlog.
 */
export const tasksRunningCount = derived(tasks, ($tasks) =>
  $tasks.filter((t) => t.status === "submitted" || t.status === "working").length,
);
