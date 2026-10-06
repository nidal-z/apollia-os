/**
 * Human-readable labels and descriptions for the notification event types
 * surfaced in the UI: the seven events a user can switch on or off, in the
 * order the settings list them.
 *
 * The runtime engine maps more RuntimeEvents than these; the others
 * (`pipeline.*`) stay out of the interface.
 */
export const NOTIFICATION_EVENT_TYPES: readonly string[] = [
  "task.completed",
  "task.failed",
  "task.input_required",
  "chat.user_input_required",
  "agent.degraded",
  "trigger.error",
  "llm.backend_down",
];

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
