/**
 * Requests to open the selector of the conversation on screen.
 *
 * The chat shortcut (Cmd L on macOS, Ctrl L elsewhere) bumps the counter; the
 * selector that is mounted, the one of the open conversation or the one of a
 * new conversation, toggles itself on each change.
 */
import { writable } from "svelte/store";

export const chatTargetSelectorRequests = writable(0);

/** Ask the mounted selector to open, or to close when it is open. */
export function requestChatTargetSelector(): void {
  chatTargetSelectorRequests.update((n) => n + 1);
}
