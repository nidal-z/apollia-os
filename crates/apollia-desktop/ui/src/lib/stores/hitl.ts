/**
 * Derived HITL (Human-in-the-Loop) stores for Apollia Desktop.
 *
 * Re-exports the base pendingApprovals store from sse.ts and provides
 * a derived count store used by the Sidebar badge.
 */
import { derived } from "svelte/store";
import { pendingApprovals } from "./sse";

export { pendingApprovals } from "./sse";

/** Number of pending HITL approvals. */
export const pendingCount = derived(
  pendingApprovals,
  ($approvals) => $approvals.length,
);
