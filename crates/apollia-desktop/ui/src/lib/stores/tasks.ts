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

/** Number of tasks that have completed successfully. */
export const completedTaskCount = derived(tasks, ($tasks) =>
  $tasks.filter((t) => t.status === "completed").length,
);
