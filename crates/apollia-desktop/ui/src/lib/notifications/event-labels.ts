/**
 * Human-readable labels and descriptions for the notification event types
 * surfaced in the UI (the 6 globally-toggleable events).
 *
 * The runtime engine maps more than 6 RuntimeEvents, but the global picker
 * intentionally narrows the surface — the others (`pipeline.*`, `chat.*`)
 * are reserved for v3.
 *
 * Keep this list in sync with `GlobalEventsEditor.svelte#ALL_EVENT_TYPES`.
 */
export const NOTIFICATION_EVENT_TYPES = [
  "task.completed",
  "task.failed",
  "task.input_required",
  "agent.degraded",
  "trigger.error",
  "llm.backend_down",
] as const;

export type NotificationEventType = (typeof NOTIFICATION_EVENT_TYPES)[number];

/**
 * i18n key for the short human label of an event type.
 * Convention: `notifications.events.<event_type>` (dotted as-is).
 */
export function eventLabelKey(event: string): string {
  return `notifications.events.${event}`;
}

/**
 * i18n key for the 1-sentence description of an event type.
 * Convention: `notifications.events_desc.<event_type>` (dotted as-is).
 */
export function eventDescriptionKey(event: string): string {
  return `notifications.events_desc.${event}`;
}
