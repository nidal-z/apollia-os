/**
 * The request the regeneration shortcut makes of the conversation shown.
 *
 * The chat route owns the shortcut and the conversation owns its thread: the
 * route names the conversation, and the conversation of that id regenerates
 * its last answer.
 */
import { writable } from "svelte/store";

/** The last request: its rank and the conversation it names. */
export const regenerateLastRequest = writable<{ rank: number; sessionId: string | null }>({
  rank: 0,
  sessionId: null,
});

/** Ask the conversation `sessionId` to regenerate its last answer. */
export function requestRegenerateLast(sessionId: string): void {
  regenerateLastRequest.update(({ rank }) => ({ rank: rank + 1, sessionId }));
}

/**
 * Calls `run` at each request that names `sessionId()` made after the call.
 * Returns what stops listening.
 */
export function onRegenerateLast(sessionId: () => string, run: () => void): () => void {
  let seen: number | null = null;
  return regenerateLastRequest.subscribe((request) => {
    if (seen === null) {
      seen = request.rank;
      return;
    }
    if (request.rank === seen) return;
    seen = request.rank;
    if (request.sessionId === sessionId()) run();
  });
}
