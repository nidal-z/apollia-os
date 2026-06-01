/**
 * First-connection guided tour - launched after the user successfully adds
 * their first MCP connection via the wizard.
 *
 * Provides a lightweight 4-step Shepherd-like spotlight sequence. Skippable,
 * persisted via localStorage (`user.onboarding_tours.first_connection_done`).
 *
 * This module is a tour descriptor: it owns the step definitions and the
 * persistence flag. The actual overlay rendering is delegated to the
 * existing `OnboardingGuidedTour` spotlight primitives (`TourSpotlight`,
 * `TourStepCard`) mounted by the caller page (e.g. the Connections view).
 */

/** Storage key for the completion flag. */
export const FIRST_CONNECTION_DONE_KEY =
  "apollia.user.onboarding_tours.first_connection_done";

/** Descriptor for a single step in the first-connection tour. */
export interface FirstConnectionTourStep {
  /** Stable step identifier (`fc-1`, `fc-2`, …). */
  id: string;
  /** CSS selector the spotlight should highlight, or `null` for a centred card. */
  spotlightSelector: string | null;
  /** i18n key for the step title. */
  titleKey: string;
  /** i18n key for the step body. */
  bodyKey: string;
}

/** Ordered list of 4 steps shown on first successful connection. */
export const FIRST_CONNECTION_STEPS: FirstConnectionTourStep[] = [
  {
    id: "fc-1",
    spotlightSelector: '[data-testid="operator-connection-card"]',
    titleKey: "integrations.first_connection_tour.step_1_title",
    bodyKey: "integrations.first_connection_tour.step_1_body",
  },
  {
    id: "fc-2",
    spotlightSelector: '[data-testid="manage-tools-count"]',
    titleKey: "integrations.first_connection_tour.step_2_title",
    bodyKey: "integrations.first_connection_tour.step_2_body",
  },
  {
    id: "fc-3",
    spotlightSelector: '[data-testid="manage-sheet-title"]',
    titleKey: "integrations.first_connection_tour.step_3_title",
    bodyKey: "integrations.first_connection_tour.step_3_body",
  },
  {
    id: "fc-4",
    spotlightSelector: '[data-testid="capability-example-card"]',
    titleKey: "integrations.first_connection_tour.step_4_title",
    bodyKey: "integrations.first_connection_tour.step_4_body",
  },
];

/** True if the first-connection tour has already been completed or skipped. */
export function isFirstConnectionTourDone(): boolean {
  return localStorage.getItem(FIRST_CONNECTION_DONE_KEY) === "true";
}

/** Mark the tour as complete so it does not re-launch. */
export function markFirstConnectionTourDone(): void {
  localStorage.setItem(FIRST_CONNECTION_DONE_KEY, "true");
}

/** Reset the flag - exposed for settings/debug affordances. */
export function resetFirstConnectionTour(): void {
  localStorage.removeItem(FIRST_CONNECTION_DONE_KEY);
}
