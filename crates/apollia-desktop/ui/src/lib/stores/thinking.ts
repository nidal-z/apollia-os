/**
 * Thinking transparency store (US-SP42-037).
 *
 * Holds one `ThinkingState` per `turn_id` so chat views can render the
 * `ThinkingPanel` and `ContradictionBadge` components as events stream in
 * from the runtime.
 *
 * The Tauri/Rust bridge is expected to forward `ThinkingStarted` and
 * `ThinkingEnded` payloads into this store via [`handleThinkingStarted`] /
 * [`handleThinkingEnded`]. Meta LLM summaries arrive later asynchronously
 * and are attached via [`attachThinkingSummary`].
 */
import { writable, derived, type Readable } from "svelte/store";
import type {
  ThinkingEndedEvent,
  ThinkingStartedEvent,
  ThinkingState,
  ThinkingSummary,
} from "$lib/types";

const states = writable<Record<string, ThinkingState>>({});

/** Read-only map of `turn_id` → `ThinkingState`. */
export const thinkingStates: Readable<Record<string, ThinkingState>> = {
  subscribe: states.subscribe,
};

/**
 * Turn id of the most recently started turn (or `null` before the first
 * `ThinkingStarted`). Used by `<InjectedMemorySheet />` to resolve the
 * injection window for the current conversation.
 */
export const latestTurnId = writable<string | null>(null);

/** Derived accessor for a single turn's state, or `null`. */
export function thinkingForTurn(
  turnId: string,
): Readable<ThinkingState | null> {
  return derived(states, ($s) => $s[turnId] ?? null);
}

export function handleThinkingStarted(event: ThinkingStartedEvent): void {
  states.update((current) => ({
    ...current,
    [event.turn_id]: {
      turn_id: event.turn_id,
      started_ms: event.ts_ms,
      ended_ms: null,
      raw_content: "",
      tokens: 0,
      summary: null,
    },
  }));
  latestTurnId.set(event.turn_id);
}

export function handleThinkingEnded(event: ThinkingEndedEvent): void {
  states.update((current) => {
    const prev = current[event.turn_id];
    return {
      ...current,
      [event.turn_id]: {
        turn_id: event.turn_id,
        started_ms: prev?.started_ms ?? event.ts_ms - event.duration_ms,
        ended_ms: event.ts_ms,
        raw_content: event.raw_content,
        tokens: event.tokens,
        summary: prev?.summary ?? null,
      },
    };
  });
}

export function attachThinkingSummary(
  turnId: string,
  summary: ThinkingSummary,
): void {
  states.update((current) => {
    const prev = current[turnId];
    if (!prev) return current;
    return { ...current, [turnId]: { ...prev, summary } };
  });
}

export function clearThinkingState(turnId: string): void {
  states.update((current) => {
    if (!(turnId in current)) return current;
    const copy = { ...current };
    delete copy[turnId];
    return copy;
  });
}
