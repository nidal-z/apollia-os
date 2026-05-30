/**
 * Derived HITL (Human-in-the-Loop) stores for Apollia Desktop.
 *
 * Re-exports the base pendingApprovals store from sse.ts and provides
 * a derived count store used by the Sidebar badge.
 *
 * Includes browser notification support for new approvals when the
 * window is not focused.
 */
import { derived, get } from "svelte/store";
import { locale } from "svelte-i18n";
import { pendingApprovals } from "./sse";
import en from "$lib/i18n/en.json";
import fr from "$lib/i18n/fr.json";

export { pendingApprovals } from "./sse";

/** Number of pending HITL approvals. */
export const pendingCount = derived(
  pendingApprovals,
  ($approvals) => $approvals.length,
);

/** Request notification permission if not already granted or denied. */
export function requestNotificationPermission(): void {
  if ("Notification" in globalThis && Notification.permission === "default") {
    Notification.requestPermission();
  }
}

/** Resolve the i18n translations for native notifications. */
function getNotificationStrings(): { title: string; bodyTemplate: string } {
  const currentLocale = get(locale) ?? "en";
  const messages = currentLocale.startsWith("fr") ? fr : en;
  return {
    title: messages.notifications.native.approval_title,
    bodyTemplate: messages.notifications.native.approval_body,
  };
}

/** Send a browser notification for a new approval when the window is not focused. */
export function notifyNewApproval(agentName: string): void {
  if (document.hidden && Notification.permission === "granted") {
    const { title, bodyTemplate } = getNotificationStrings();
    new Notification(title, {
      body: bodyTemplate.replace("{agentName}", agentName),
      icon: "/favicon.png",
    });
  }
}
