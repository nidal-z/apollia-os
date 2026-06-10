/**
 * Thinking transparency store.
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
  DecisionPoint,
  DecisionPointRecordedEvent,
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

/**
 * Live or frozen thinking text of the most recently started turn, shaped for
 * the plan DAG overlay. `live` is true between `ThinkingStarted` and
 * `ThinkingEnded`; it flips to false once the trace settles. Returns `null`
 * before the first turn so an idle panel renders no overlay.
 *
 * While live, `raw_content` is empty (the runtime only carries the full text on
 * `ThinkingEnded`), so the panel shows the "Thinking" header without a body.
 */
export const latestThinking: Readable<{ content: string; live: boolean } | null> =
  derived([states, latestTurnId], ([$states, $turnId]) => {
    if ($turnId === null) return null;
    const state = $states[$turnId];
    if (!state) return null;
    return { content: state.raw_content, live: state.ended_ms === null };
  });

const decisionPoints = writable<Record<string, DecisionPoint>>({});

/** Read-only map of `turn_id` → recorded `DecisionPoint`. */
export const decisionPointStates: Readable<Record<string, DecisionPoint>> = {
  subscribe: decisionPoints.subscribe,
};

/**
 * Turn id of the most recently recorded decision point (or `null` before the
 * first `DecisionPointRecorded`). Decision points are opt-in, so most turns
 * never set this.
 */
export const latestDecisionTurnId = writable<string | null>(null);

/**
 * The most recently recorded decision point, or `null` when none has landed.
 * Surfaced by the plan DAG panel next to the current step.
 */
export const latestDecisionPoint: Readable<DecisionPoint | null> = derived(
  [decisionPoints, latestDecisionTurnId],
  ([$points, $turnId]) => ($turnId === null ? null : ($points[$turnId] ?? null)),
);

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

export function handleDecisionPointRecorded(
  event: DecisionPointRecordedEvent,
): void {
  const point = event.point;
  if (!point || typeof point.turn_id !== "string") return;
  decisionPoints.update((current) => ({ ...current, [point.turn_id]: point }));
  latestDecisionTurnId.set(point.turn_id);
}

export function clearThinkingState(turnId: string): void {
  states.update((current) => {
    if (!(turnId in current)) return current;
    const copy = { ...current };
    delete copy[turnId];
    return copy;
  });
}

/**
 * Clears every per-turn thinking and decision artifact. Called when a plan
 * panel tears down its session so a later session starts from a clean slate.
 */
export function resetThinking(): void {
  states.set({});
  decisionPoints.set({});
  latestTurnId.set(null);
  latestDecisionTurnId.set(null);
}
