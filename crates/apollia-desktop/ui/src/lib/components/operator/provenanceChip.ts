// Pure mapping from a step origin to a provenance chip descriptor.
//
// Extracted from the node so the mapping (origin -> label key + token class)
// can be unit-tested without a DOM. The wire form mirrors
// `apollia_core::plan::StepOrigin`, serialized snake_case with `replan`
// carrying a revision number.

import type { StepOrigin } from "$lib/ipc/plan";
import { PLAN_SESSION_KEYS } from "$lib/i18n/strings/planSession";

/** Visual descriptor for a provenance chip. */
export interface ProvenanceChipDescriptor {
  /** i18n key for the chip label. */
  labelKey: string;
  /** Interpolation values for the label (e.g. the replan revision). */
  labelValues?: Record<string, number>;
  /** Token-based class controlling the chip color. Never a hardcoded value. */
  tokenClass: string;
}

/**
 * Maps a step origin to its chip descriptor.
 *
 * Each origin gets a distinct token class so the four origins stay visually
 * separable in both light and dark themes. `replan` interpolates its revision
 * into the label. An unknown origin string falls back to the neutral initial
 * descriptor so the chip is never blank.
 */
export function originToChip(origin: StepOrigin): ProvenanceChipDescriptor {
  if (typeof origin === "object" && origin !== null && "replan" in origin) {
    return {
      labelKey: PLAN_SESSION_KEYS.originReplan,
      labelValues: { revision: origin.replan },
      tokenClass: "bg-warning/10 text-warning",
    };
  }
  switch (origin) {
    case "user_inject":
      return {
        labelKey: PLAN_SESSION_KEYS.originUserInject,
        tokenClass: "bg-accent/10 text-accent-foreground",
      };
    case "agent_edit":
      return {
        labelKey: PLAN_SESSION_KEYS.originAgentEdit,
        tokenClass: "bg-primary/10 text-primary",
      };
    case "initial":
    default:
      return {
        labelKey: PLAN_SESSION_KEYS.originInitial,
        tokenClass: "bg-muted/30 text-muted-foreground",
      };
  }
}

/** True when the step carries a change/removal reason worth rendering. */
export function hasReason(reason: string | null | undefined): reason is string {
  return typeof reason === "string" && reason.trim().length > 0;
}
