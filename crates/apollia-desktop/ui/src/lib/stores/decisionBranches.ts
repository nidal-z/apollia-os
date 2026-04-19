/**
 * Decision branches store (US-SP42-041 — Pattern P5).
 *
 * Holds one `DecisionPoint` per `turn_id` so chat views can render the
 * `<DecisionBranchesPanel />` as `DecisionPointRecorded` runtime events
 * arrive via the Tauri bridge.
 *
 * The bridge is expected to forward each payload into this store via
 * [`handleDecisionPointRecorded`]. Opt-in: when the backend routine
 * `GenerateAlternativeBranches` is disabled, no events arrive and the
 * store stays empty.
 */
import { writable, derived, type Readable } from "svelte/store";
import type { DecisionPoint } from "$lib/types";

const points = writable<Record<string, DecisionPoint>>({});

/** Read-only map of `turn_id` → `DecisionPoint`. */
export const decisionPoints: Readable<Record<string, DecisionPoint>> = {
  subscribe: points.subscribe,
};

/** Derived accessor for a single turn's decision point, or `null`. */
export function decisionPointForTurn(
  turnId: string,
): Readable<DecisionPoint | null> {
  return derived(points, ($p) => $p[turnId] ?? null);
}

export function handleDecisionPointRecorded(point: DecisionPoint): void {
  points.update((current) => ({ ...current, [point.turn_id]: point }));
}

export function clearDecisionPoint(turnId: string): void {
  points.update((current) => {
    if (!(turnId in current)) return current;
    const copy = { ...current };
    delete copy[turnId];
    return copy;
  });
}
