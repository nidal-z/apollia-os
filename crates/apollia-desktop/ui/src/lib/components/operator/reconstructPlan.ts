// Pure reconstruction of a plan state at a given revision from the ordered
// mutation list. Extracted so it can be unit-tested without a DOM or IPC.
//
// The mutation log is the inviolable trace of how a plan was built. Each entry
// is one recorded step delta in insertion order. Replaying the first `n` entries
// rebuilds the plan as it stood after the n-th change, which is what the history
// scrubber renders.
//
// Wire shape note: the runtime records `propose` and `reorder` mutations with no
// per-step payload (`after` is null and `step_id` is null). They therefore carry
// no step state to replay and are treated as structural markers, not corruption.
// A plan built incrementally through `add_step` replays faithfully; a single
// multi-step `propose` is a reset marker whose steps land through the deltas that
// follow it.

import type { PlanStep } from "$lib/ipc/plan";
import type { PlanMutation } from "$lib/ipc/plan";

/** A mutation flagged as corrupt and skipped during reconstruction. */
export interface SkippedMutation {
  /** Revision index (1-based) at which the corrupt entry was found. */
  revision: number;
  /** Short machine-readable cause, surfaced as a count in the UI. */
  cause: string;
}

/** Result of reconstructing the plan at a revision. */
export interface ReconstructResult {
  steps: PlanStep[];
  skipped: SkippedMutation[];
}

/** Kinds that carry a per-step payload we replay onto the reconstructed plan. */
const STEP_SETTING_KINDS = new Set(["add_step", "modify_step", "status_change"]);

/**
 * Reports whether a mutation is malformed for its kind.
 *
 * A step-setting mutation (`add_step` / `modify_step` / `status_change`) must
 * carry an `after` image; a `remove_step` must carry a `step_id`. Structural
 * markers (`propose`, `reorder`, `submit`, `approve`, `reject`) are never
 * corrupt: they carry no step payload by design.
 */
function isCorrupt(m: PlanMutation): boolean {
  if (STEP_SETTING_KINDS.has(m.kind)) return m.after === null;
  if (m.kind === "remove_step") return m.step_id === null;
  return false;
}

/**
 * Reconstructs the plan state after applying the first `revision` mutations.
 *
 * Applies step-setting mutations through their `after` image and `remove_step`
 * by dropping the step id, in insertion order. Corrupt entries are skipped and
 * reported in `skipped`, never thrown. A `revision` of 0 yields the empty plan;
 * values beyond the list apply every mutation.
 */
export function reconstructPlanAt(
  mutations: PlanMutation[],
  revision: number,
): ReconstructResult {
  const byId = new Map<string, PlanStep>();
  const skipped: SkippedMutation[] = [];
  const upTo = Math.max(0, Math.min(revision, mutations.length));

  for (let i = 0; i < upTo; i += 1) {
    const m = mutations[i];
    if (isCorrupt(m)) {
      skipped.push({ revision: i + 1, cause: `corrupt_${m.kind}` });
      continue;
    }
    if (STEP_SETTING_KINDS.has(m.kind) && m.after) {
      byId.set(m.after.step_id, m.after);
    } else if (m.kind === "remove_step" && m.step_id) {
      byId.delete(m.step_id);
    }
  }

  return { steps: [...byId.values()], skipped };
}

/** Total number of revisions available (one per recorded mutation). */
export function revisionCount(mutations: PlanMutation[]): number {
  return mutations.length;
}
