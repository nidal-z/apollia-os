/**
 * Browser-side state of the dismissable reminders.
 *
 * A reminder that remembers its dismissal in the browser keeps it here, under
 * one key derived from its registry id, rather than under a key of its own.
 * The registry (`./registry.ts`) is then the one place that knows every
 * reminder and how to clear it. A reminder whose state lives in the runtime
 * (the onboarding conversation) does not use this module.
 */

const PREFIX = "apollia.reminders.";

/** The `localStorage` key that holds the state of reminder `id`. */
export function reminderStorageKey(id: string): string {
  return `${PREFIX}${id}`;
}

/** The stored state of reminder `id`, or `null` when none is stored or readable. */
export function readReminderState<T>(id: string): T | null {
  if (typeof localStorage === "undefined") return null;
  try {
    const raw = localStorage.getItem(reminderStorageKey(id));
    return raw ? (JSON.parse(raw) as T) : null;
  } catch {
    return null;
  }
}

export function writeReminderState(id: string, value: unknown): void {
  if (typeof localStorage === "undefined") return;
  try {
    localStorage.setItem(reminderStorageKey(id), JSON.stringify(value));
  } catch {
    // Storage refused or full: the reminder simply comes back next time.
  }
}

export function clearReminderState(id: string): void {
  if (typeof localStorage === "undefined") return;
  try {
    localStorage.removeItem(reminderStorageKey(id));
  } catch {
    // Storage refused: nothing was stored either.
  }
}
