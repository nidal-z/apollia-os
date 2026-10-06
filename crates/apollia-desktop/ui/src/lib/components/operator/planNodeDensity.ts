// Pure helper deriving which plan-step node fields are visible per UI mode.
//
// Extracted from the node so the density rules can be unit-tested without a
// DOM. Operator stays compact (title + status + provenance chip only). Builder
// exposes every detail field per the observability doctrine. Field presence is
// still gated per-step by the data itself (an absent hint hides its line even
// in Builder).

/** The two densities of a plan step node; "builder" is the developer mode. */
export type PlanNodeDensity = "operator" | "builder";

/** Fields a plan step node may render, gated by UI mode. */
export interface NodeFieldVisibility {
  showDescription: boolean;
  showDependencies: boolean;
  showHints: boolean;
  showRationale: boolean;
  showReason: boolean;
}

/**
 * Returns the field visibility for `mode`.
 *
 * Operator returns all-false (the node keeps only its title, status and
 * provenance chip). Builder returns all-true, exposing dependencies, tool and
 * model hints, the rationale and the change reason.
 */
export function nodeFields(mode: PlanNodeDensity): NodeFieldVisibility {
  const builder = mode === "builder";
  return {
    showDescription: builder,
    showDependencies: builder,
    showHints: builder,
    showRationale: builder,
    showReason: builder,
  };
}
