/**
 * Type contract for the guided-tour system.
 *
 * The step catalogue lives in the frontend (see `catalog.ts`) rather than in the
 * Rust backend. `TourStep.anchor` therefore resolves against the same
 * `data-testid` corpus the automation scripts use, and tour routes are typed as
 * `Route`, so a slug that no longer exists fails at type-check instead of
 * silently unmounting the page content at runtime.
 */
import type { Route } from "$lib/stores/navigation";

/** Identifier of a tour in the catalogue. */
export type TourId =
  | "first-result"
  | "landmarks"
  | "frame"
  | "automate"
  | "connect"
  | "follow";

/** Identifier of a getting-started milestone. */
export type MilestoneId = "chat" | "frame" | "automate" | "connect" | "follow";

/**
 * How a tour presents itself.
 *
 * `modal` dims the application behind a spotlight cutout and traps focus inside
 * the step card. `annotated` renders the card beside a fully usable interface
 * and traps nothing: the first-result tour points at a live approval card, and
 * the user must be able to answer it while the annotation is on screen.
 */
export type TourPresentation = "modal" | "annotated";

/**
 * How a step's anchor is located in the DOM.
 *
 * `testidPrefix` matches `[data-testid^="..."]`, which is required for anchors
 * whose testid is interpolated at render time (approval cards carry the tool
 * name, for instance).
 */
export type StepAnchor =
  | { readonly kind: "testid"; readonly value: string }
  | {
      readonly kind: "testidPrefix";
      readonly value: string;
      /**
       * Which match to take, negative counting from the end. Mirrors the
       * automation scripts' convention, where `-1` is the last match. Defaults
       * to the first match.
       */
      readonly nth?: number;
    };

/** A single step of a tour. */
export interface TourStep {
  /** Stable identifier, used for persistence and telemetry. */
  readonly id: string;
  /** Element the step points at, or `null` for a step with no visual anchor. */
  readonly anchor: StepAnchor | null;
  /** i18n key of the step title. */
  readonly titleKey: string;
  /** i18n key of the step body. */
  readonly bodyKey: string;
  /**
   * Keep the step even when its anchor cannot be resolved during pre-flight.
   *
   * Such a step renders in the anchorless state instead of being dropped. Use it
   * for anchors that only appear after an action taken during the tour itself.
   */
  readonly optional?: boolean;
  /**
   * Wait for the anchor to appear instead of resolving it immediately.
   *
   * Used by the annotated presentation: the approval-card annotation must fire
   * when the card actually shows up, which may be several seconds after the step
   * becomes current, or never.
   */
  readonly awaitAnchor?: boolean;
}

/** A complete tour. */
export interface TourDefinition {
  readonly id: TourId;
  readonly presentation: TourPresentation;
  /** Route the tour lives on, or `null` to run wherever the user already is. */
  readonly route: Route | null;
  /** i18n key of the tour name, shown when resuming. */
  readonly titleKey: string;
  /**
   * Side effect run once, after navigating and before pre-flight.
   *
   * Lets a tour put the surface into the state its first step describes, such
   * as opening the new-conversation picker. Kept declarative here rather than
   * special-cased in the engine.
   */
  readonly onStart?: () => void;
  readonly steps: readonly TourStep[];
}

/** Lifecycle of a tour for a given user. */
export type TourStatus = "not_started" | "in_progress" | "done" | "skipped";

/** Persisted progress of a single tour. */
export interface TourRunState {
  readonly status: TourStatus;
  /** Index of the step the user stopped on, within the pre-flighted list. */
  readonly index: number;
}

/** Everything the host component needs to render the current tour. */
export interface ActiveTour {
  readonly definition: TourDefinition;
  /** Steps retained after pre-flight. Never empty: an empty tour never starts. */
  readonly steps: readonly TourStep[];
  readonly index: number;
  readonly rect: DOMRect | null;
  /** True while the exit confirmation is showing. */
  readonly confirmingExit: boolean;
}
