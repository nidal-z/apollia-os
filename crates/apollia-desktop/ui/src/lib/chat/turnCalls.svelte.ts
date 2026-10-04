/**
 * The model calls of the turns of the chat, read once per turn and shared.
 *
 * The detail under an answer and the steps panel show the same calls. Each
 * turn is read on demand through its command and kept here, keyed by its
 * conversation and by the identifier of its answer, which never names another
 * turn: an answer played again gets a new identifier.
 */
import { SvelteMap } from "svelte/reactivity";
import { listTurnLlmCalls } from "$lib/ipc/chat";
import type { TurnLlmCall } from "$lib/chat/generationStats";

/** What is known of the calls of one turn. */
export type TurnCallsState =
  | { status: "loading" }
  | { status: "ready"; calls: TurnLlmCall[] }
  | { status: "error"; message: string };

const cache = new SvelteMap<string, TurnCallsState>();

function keyOf(sessionId: string, turnId: string): string {
  return `${sessionId}\n${turnId}`;
}

/** What is known of the calls of a turn; `undefined` when never asked. */
export function turnCallsOf(sessionId: string, turnId: string): TurnCallsState | undefined {
  return cache.get(keyOf(sessionId, turnId));
}

/**
 * Reads the calls of a turn unless they are already held or being read. A
 * read that failed is tried again.
 */
export async function loadTurnCalls(sessionId: string, turnId: string): Promise<void> {
  const key = keyOf(sessionId, turnId);
  const held = cache.get(key);
  if (held && held.status !== "error") return;
  cache.set(key, { status: "loading" });
  try {
    cache.set(key, { status: "ready", calls: await listTurnLlmCalls(sessionId, turnId) });
  } catch (error) {
    cache.set(key, {
      status: "error",
      message: error instanceof Error ? error.message : String(error),
    });
  }
}

/** Forgets every turn read so far. For tests. */
export function resetTurnCalls(): void {
  cache.clear();
}
