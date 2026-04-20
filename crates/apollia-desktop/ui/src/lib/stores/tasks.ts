/**
 * Derived task stores for Apollia Desktop.
 *
 * Re-exports the base tasks store from sse.ts and provides derived
 * stores for filtered views (running tasks, completed count, etc.).
 */
import { derived, writable } from "svelte/store";
import { tasks } from "./sse";

export { tasks } from "./sse";

/** Tasks currently in 'working' state. */
export const runningTasks = derived(tasks, ($tasks) =>
  $tasks.filter((t) => t.status === "working"),
);

/**
 * Count of tasks considered "in-flight" — displayed in the sidebar next to
 * "Mon travail". The runtime exposes `submitted` (queued, waiting for the
 * actor pool) and `working` (actively running); both should count toward the
 * visible badge so the operator sees the real backlog.
 */
export const tasksRunningCount = derived(tasks, ($tasks) =>
  $tasks.filter((t) => t.status === "submitted" || t.status === "working").length,
);

/** Number of tasks that have completed successfully. */
export const completedTaskCount = derived(tasks, ($tasks) =>
  $tasks.filter((t) => t.status === "completed").length,
);

/**
 * Cross-component signal to open the "new task" dialog on the Tasks route.
 * The sidebar / command palette / FAB set this to `Date.now()`; `TaskList`
 * subscribes and opens the dialog on change.
 */
export const openNewTaskRequested = writable<number>(0);
