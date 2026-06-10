// Pure mapping from a plan step status to its design-system token classes.
//
// Centralizes the status palette so node rendering never reaches for a raw
// color. Every value is a Tailwind class bound to a design token (no hex, no
// hsl literal). Unit-tested in isolation.

import type { StepStatus } from "$lib/ipc/plan";

export interface StepStatusTokens {
  /** Text token class, e.g. "text-success". */
  text: string;
  /** Border token class, e.g. "border-success/40". */
  border: string;
  /** Surface tint token class, e.g. "bg-success/10". */
  surface: string;
  /** i18n key for the human-readable status label. */
  labelKey: string;
  /** Whether the node should pulse (only the active step). */
  pulse: boolean;
}

/**
 * Maps a {@link StepStatus} to its design-system token classes and label key.
 *
 * Pure: no DOM, no side effect. Never returns a hardcoded color value.
 * `pending` and `skipped` share the muted palette (the default branch).
 */
export function stepStatusToken(status: StepStatus): StepStatusTokens {
  switch (status) {
    case "in_progress":
      return {
        text: "text-primary",
        border: "border-primary/50",
        surface: "bg-primary/10",
        labelKey: "plan_session.status_in_progress",
        pulse: true,
      };
    case "completed":
      return {
        text: "text-success",
        border: "border-success/40",
        surface: "bg-success/10",
        labelKey: "plan_session.status_completed",
        pulse: false,
      };
    case "failed":
      return {
        text: "text-destructive",
        border: "border-destructive/40",
        surface: "bg-destructive/10",
        labelKey: "plan_session.status_failed",
        pulse: false,
      };
    case "skipped":
      return {
        text: "text-muted-foreground",
        border: "border-border",
        surface: "bg-muted/10",
        labelKey: "plan_session.status_skipped",
        pulse: false,
      };
    case "pending":
    default:
      return {
        text: "text-muted-foreground",
        border: "border-border",
        surface: "bg-muted/10",
        labelKey: "plan_session.status_pending",
        pulse: false,
      };
  }
}
