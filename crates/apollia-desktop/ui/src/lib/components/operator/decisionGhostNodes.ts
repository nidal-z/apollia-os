// Pure mapping from a recorded decision point to ghost-node descriptors, one
// per rejected alternative.
//
// Extracted so the mapping can be unit-tested without xyflow or a DOM. The
// input types reuse the runtime event contract from `$lib/types`
// (`DecisionPoint` / `ConsideredAlternative`), so there is a single source of
// truth for the decision-point shape.

import type { DecisionPoint } from "$lib/types";

/** Descriptor for a ghost node rendered next to the chosen step. */
export interface GhostNodeDescriptor {
  /** Stable id derived from the turn and the alternative index. */
  id: string;
  /** Id of the turn whose chosen step this ghost node hangs off. */
  anchorTurnId: string;
  label: string;
  rejectedReason: string;
  confidenceDelta: number;
  /** Token-based class for the attenuated ghost styling. */
  tokenClass: string;
}

const GHOST_TOKEN_CLASS =
  "border-dashed border-border bg-muted/40 text-muted-foreground";

/**
 * Builds one ghost-node descriptor per rejected alternative of `point`.
 *
 * Returns an empty array when `point` is null or has no alternatives, so a turn
 * without rejected options renders nothing extra.
 */
export function toGhostNodes(point: DecisionPoint | null): GhostNodeDescriptor[] {
  if (!point || point.alternatives.length === 0) return [];
  return point.alternatives.map((alt, index) => ({
    id: `${point.turn_id}-ghost-${index}`,
    anchorTurnId: point.turn_id,
    label: alt.label,
    rejectedReason: alt.rejected_reason,
    confidenceDelta: alt.confidence_delta,
    tokenClass: GHOST_TOKEN_CLASS,
  }));
}
