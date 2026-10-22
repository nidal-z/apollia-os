/**
 * The request the regeneration shortcut makes of the conversation shown.
 *
 * The chat route owns the shortcut and the conversation owns its thread: the
 * route names the conversation, and the conversation of that id regenerates
 * its last answer. A request may also name the answer it means, so that it
 * regenerates nothing once another turn has followed that answer.
 */
import { writable } from "svelte/store";

/** The last request: its rank, the conversation it names, and the answer it means. */
export const regenerateLastRequest = writable<{
  rank: number;
  sessionId: string | null;
  answerId?: string;
}>({
  rank: 0,
  sessionId: null,
});

/**
 * The answer each conversation shown would regenerate now, by conversation
 * id: its last answer when it follows a message of the user and no turn is
 * generating, `null` otherwise. The conversation keeps its own entry current.
 */
export const regenerableAnswers = writable<Record<string, string | null>>({});

/** Records the answer the conversation `sessionId` would regenerate now. */
export function setRegenerableAnswer(sessionId: string, answerId: string | null): void {
  regenerableAnswers.update((all) => (all[sessionId] === answerId ? all : { ...all, [sessionId]: answerId }));
}

/** Forgets the conversation `sessionId`, once it is no longer shown. */
export function forgetRegenerableAnswer(sessionId: string): void {
  regenerableAnswers.update((all) => {
    if (!(sessionId in all)) return all;
    const rest = { ...all };
    delete rest[sessionId];
    return rest;
  });
}

// Whether a conversation started the request being made: listeners run
// synchronously inside the update, so the request reads it right after.
let started = false;

/**
 * Ask the conversation `sessionId` to regenerate its last answer, or only the
 * answer `answerId` when it is given and still the last one. Returns whether a
 * regeneration started.
 */
export function requestRegenerateLast(sessionId: string, answerId?: string): boolean {
  started = false;
  regenerateLastRequest.update(({ rank }) => ({ rank: rank + 1, sessionId, answerId }));
  const result = started;
  started = false;
  return result;
}

/**
 * Calls `run` at each request that names `sessionId()` made after the call,
 * with the answer the request means, if any. `run` returns whether it started
 * a regeneration. Returns what stops listening.
 */
export function onRegenerateLast(
  sessionId: () => string,
  run: (answerId: string | undefined) => boolean,
): () => void {
  let seen: number | null = null;
  return regenerateLastRequest.subscribe((request) => {
    if (seen === null) {
      seen = request.rank;
      return;
    }
    if (request.rank === seen) return;
    seen = request.rank;
    if (request.sessionId === sessionId() && run(request.answerId)) started = true;
  });
}
