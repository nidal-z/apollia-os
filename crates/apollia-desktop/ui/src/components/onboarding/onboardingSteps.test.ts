import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, test, expect } from "vitest";
import { ONBOARDING_STEPS, phaseOfStep, stepOfPhase } from "./onboardingSteps";

// The list is not frozen here: a later step extends it. What is held is that
// the usage-profile question is not one of the steps, and that the modal draws
// this list and no other.

function modalSource(): string {
  return readFileSync(
    fileURLToPath(new URL("./OnboardingModal.svelte", import.meta.url)),
    "utf8",
  );
}

describe("onboarding steps", () => {
  test("no step asks for a usage profile", () => {
    // GIVEN the steps of the first-launch modal
    const ids = ONBOARDING_STEPS.map((step) => step.id as string);
    const labels = ONBOARDING_STEPS.map((step) => step.labelKey);

    // WHEN they are read
    // THEN none is the profile step, by identifier, label or phase
    expect(ids).not.toContain("profile");
    expect(labels).not.toContain("onboarding_modal.step_profile");
    expect(ONBOARDING_STEPS.map((step) => step.phase)).not.toContain("profile_choice");
  });

  test("the welcome leads straight to the language model", () => {
    // GIVEN the order of the steps
    const ids = ONBOARDING_STEPS.map((step) => step.id);

    // WHEN the step after the welcome is read
    const next = ids[ids.indexOf("welcome") + 1];

    // THEN it is the AI setup, and it moves the backend to its phase
    expect(next).toBe("ai-setup");
    expect(phaseOfStep("ai-setup")).toBe("ai_setup");
  });

  test("a phase stored by an earlier version reopens on a step that exists", () => {
    // GIVEN the phase names the backend may answer, the removed one included
    // WHEN each is turned into a step
    // THEN each lands on a step of the list
    const ids = ONBOARDING_STEPS.map((step) => step.id);
    for (const phase of ["welcome", "profile_choice", "ai_setup", "acquaintance", "done"]) {
      expect(ids).toContain(stepOfPhase(phase));
    }
    expect(stepOfPhase("acquaintance")).toBe("chat");
  });

  test("the modal draws this list and no profile selector", () => {
    // GIVEN the source of the modal
    const source = modalSource();

    // WHEN its imports and step references are read
    // THEN it takes its steps from this module and mounts no profile step
    expect(source).toContain("ONBOARDING_STEPS");
    expect(source).not.toContain("OnboardingProfileSelector");
    expect(source).not.toMatch(/goTo\("profile"\)/);
  });
});
