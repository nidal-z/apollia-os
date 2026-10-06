import type { OnboardingPhase } from "$lib/types";

/**
 * The steps of the first-launch modal, in the order the progress shows them,
 * and the phase of the backend machine each one stands for.
 *
 * The list lives here rather than in the modal so that it can be read without
 * rendering a component: a step added or removed is a change to this file,
 * and its test says in which order the steps come and which one may never
 * come back. The progress counts the steps of this list and no other, so a
 * step that is not built yet is not announced.
 */
export type OnboardingStep =
  | "welcome"
  | "ai-setup"
  | "embeddings"
  | "chat"
  | "first-agent"
  | "connection";

export interface OnboardingStepEntry {
  id: OnboardingStep;
  labelKey: string;
  /** The phase the backend stands on while the step is open. */
  phase: OnboardingPhase;
}

/**
 * Every phase of the backend machine, in the order it walks them. The machine
 * only moves from one phase to the next, so the place of a step in the flow is
 * the place of its phase here.
 */
export const ONBOARDING_PHASE_ORDER: readonly OnboardingPhase[] = [
  "welcome",
  "ai_setup",
  "embeddings",
  "acquaintance",
  "first_agent",
  "connection",
  "first_task",
  "guided_tour",
  "graduation",
  "done",
];

export const ONBOARDING_STEPS: readonly OnboardingStepEntry[] = [
  { id: "welcome", labelKey: "onboarding_modal.step_language", phase: "welcome" },
  { id: "ai-setup", labelKey: "onboarding_modal.step_ai", phase: "ai_setup" },
  { id: "embeddings", labelKey: "onboarding_modal.step_search", phase: "embeddings" },
  { id: "chat", labelKey: "onboarding_modal.step_about", phase: "acquaintance" },
  { id: "first-agent", labelKey: "onboarding_modal.step_first_agent", phase: "first_agent" },
  { id: "connection", labelKey: "onboarding_modal.step_connection", phase: "connection" },
];

/**
 * The step that follows `step`, `null` after the last one built: the modal
 * then closes, as it does at the end of the first launch. A step added to the
 * list is reached from the one before it with no change to the modal. A step
 * named in `passed` is stepped over: it has nothing to show this time.
 */
export function nextStep(
  step: OnboardingStep,
  passed: readonly OnboardingStep[] = [],
): OnboardingStep | null {
  const index = ONBOARDING_STEPS.findIndex((entry) => entry.id === step);
  const next = ONBOARDING_STEPS.slice(index + 1).find((entry) => !passed.includes(entry.id));
  return next?.id ?? null;
}

/** The phase the backend moves to when `step` opens. */
export function phaseOfStep(step: OnboardingStep): OnboardingPhase {
  return ONBOARDING_STEPS.find((entry) => entry.id === step)?.phase ?? "welcome";
}

/**
 * The step to reopen for a persisted phase, so the flow resumes across
 * restarts on the step it was left on. A phase no step of this list stands
 * for reopens on the last step before it; a phase this interface does not
 * know reopens on the first step.
 */
export function stepOfPhase(phase: string): OnboardingStep {
  const reached = ONBOARDING_PHASE_ORDER.indexOf(phase as OnboardingPhase);
  for (let index = reached; index >= 0; index -= 1) {
    const entry = ONBOARDING_STEPS.find((step) => step.phase === ONBOARDING_PHASE_ORDER[index]);
    if (entry) return entry.id;
  }
  return ONBOARDING_STEPS[0].id;
}

/**
 * The phases to walk, in order, to bring the backend from `from` to `to`.
 * Empty when `to` is the phase it stands on or one already behind it: going
 * back a step in the modal never moves the backend backwards.
 */
export function phasePath(from: string, to: OnboardingPhase): OnboardingPhase[] {
  const start = ONBOARDING_PHASE_ORDER.indexOf(from as OnboardingPhase);
  const end = ONBOARDING_PHASE_ORDER.indexOf(to);
  if (start === -1 || end <= start) return [];
  return ONBOARDING_PHASE_ORDER.slice(start + 1, end + 1);
}
