/**
 * Derived HITL (Human-in-the-Loop) stores for Apollia Desktop.
 *
 * Re-exports the base pendingApprovals store from sse.ts and provides
 * a derived count store used by the Sidebar badge.
 *
 * Includes browser notification support for new approvals when the
 * window is not focused (AC-7).
 */
import { derived } from "svelte/store";
import { pendingApprovals } from "./sse";

export { pendingApprovals } from "./sse";

/** Number of pending HITL approvals. */
export const pendingCount = derived(
  pendingApprovals,
  ($approvals) => $approvals.length,
);

/** Request notification permission if not already granted or denied. */
export function requestNotificationPermission(): void {
  if ("Notification" in window && Notification.permission === "default") {
    Notification.requestPermission();
  }
}

/** Send a browser notification for a new approval when the window is not focused. */
export function notifyNewApproval(agentName: string): void {
  if (document.hidden && Notification.permission === "granted") {
    new Notification("Action requise", {
      body: `Agent ${agentName} attend une approbation`,
      icon: "/favicon.png",
    });
  }
}
