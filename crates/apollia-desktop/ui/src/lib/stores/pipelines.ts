/**
 * Derived pipeline stores for Apollia Desktop.
 *
 * Re-exports the base pipelineRuns store from sse.ts and provides
 * derived stores for filtered views (active runs, history).
 */
import { derived } from "svelte/store";
import { pipelineRuns } from "./sse";

export { pipelineRuns } from "./sse";

/** Pipeline runs currently in progress (running or waiting_approval). */
export const activePipelineRuns = derived(pipelineRuns, ($runs) =>
  $runs.filter((r) => r.status === "running" || r.status === "waiting_approval"),
);

/** Pipeline runs that have reached a terminal state (completed or failed). */
export const historicPipelineRuns = derived(pipelineRuns, ($runs) =>
  $runs.filter((r) => r.status === "completed" || r.status === "failed"),
);

/** Number of currently active pipeline runs. */
export const activePipelineCount = derived(
  activePipelineRuns,
  ($active) => $active.length,
);
