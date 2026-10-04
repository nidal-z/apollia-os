import type { OnboardingPhase } from "$lib/types";

/**
 * The steps of the first-launch modal, in the order the stepper shows them,
 * and the phase of the backend machine each one stands for.
 *
 * The list lives here rather than in the modal so that it can be read without
 * rendering a component: a step added or removed is a change to this file,
 * and its test says which step may never come back.
 */
export type OnboardingStep = "welcome" | "ai-setup" | "chat";

export interface OnboardingStepEntry {
  id: OnboardingStep;
  labelKey: string;
  /** The phase the backend moves to when the step opens; `null` for the first. */
  phase: OnboardingPhase | null;
}

export const ONBOARDING_STEPS: readonly OnboardingStepEntry[] = [
  { id: "welcome", labelKey: "onboarding_modal.step_welcome", phase: null },
  { id: "ai-setup", labelKey: "onboarding_modal.step_models", phase: "ai_setup" },
  { id: "chat", labelKey: "onboarding_modal.step_calibration", phase: "acquaintance" },
];

/** The phase the backend moves to when `step` opens. */
export function phaseOfStep(step: OnboardingStep): OnboardingPhase | null {
  return ONBOARDING_STEPS.find((entry) => entry.id === step)?.phase ?? null;
}

/**
 * The step to reopen for a persisted phase, so the flow resumes across
 * restarts. The phases past the conversation reopen on it; a phase this
 * interface does not know reopens on the first step.
 */
export function stepOfPhase(phase: string): OnboardingStep {
  const direct = ONBOARDING_STEPS.find((entry) => entry.phase === phase);
  if (direct) return direct.id;
  if (phase === "guided_tour" || phase === "graduation" || phase === "done") return "chat";
  return "welcome";
}
