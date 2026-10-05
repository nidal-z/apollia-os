import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, test, expect } from "vitest";
import en from "$lib/i18n/en.json";
import fr from "$lib/i18n/fr.json";
import {
  ONBOARDING_PHASE_ORDER,
  ONBOARDING_STEPS,
  phaseOfStep,
  phasePath,
  stepOfPhase,
} from "./onboardingSteps";

// The list is not frozen here: three steps are still to be added after the
// conversation. What is held is the order of the steps that exist, that each
// one stands for its own phase of the backend, that the usage-profile question
// is not one of them, and that the modal draws this list and no other.

function source(file: string): string {
  return readFileSync(fileURLToPath(new URL(file, import.meta.url)), "utf8");
}

describe("onboarding steps - the order of the first launch", () => {
  test("the steps come in the order language, AI, embeddings, who I am", () => {
    // GIVEN the steps of the first-launch modal
    const ids = ONBOARDING_STEPS.map((step) => step.id);

    // WHEN their order is read
    // THEN it is the order of the first launch, the embeddings model third
    // and the conversation fourth
    expect(ids).toEqual(["welcome", "ai-setup", "embeddings", "chat"]);
    expect(ONBOARDING_STEPS.map((step) => step.labelKey)).toEqual([
      "onboarding_modal.step_language",
      "onboarding_modal.step_ai",
      "onboarding_modal.step_search",
      "onboarding_modal.step_about",
    ]);
  });

  test("the steps follow the order of the phases, one phase each", () => {
    // GIVEN the place of the phase of each step in the chain the backend walks
    const places = ONBOARDING_STEPS.map((step) => ONBOARDING_PHASE_ORDER.indexOf(step.phase));

    // WHEN those places are compared with the order of the steps
    // THEN every phase is known, no two steps share one, and they only go up:
    // a step added later takes its place by its phase
    expect(places).not.toContain(-1);
    expect(new Set(places).size).toBe(places.length);
    expect(places).toEqual([...places].sort((a, b) => a - b));
  });

  test("the seven steps of the first launch are the first seven phases", () => {
    // GIVEN the chain of phases the backend walks
    // WHEN its first seven are read
    // THEN they are the seven steps, in their order, before the closing phases
    expect(ONBOARDING_PHASE_ORDER.slice(0, 7)).toEqual([
      "welcome",
      "ai_setup",
      "embeddings",
      "acquaintance",
      "first_agent",
      "connection",
      "first_task",
    ]);
  });

  test("no step is announced before it is built", () => {
    // GIVEN the source of the modal
    const modal = source("./OnboardingModal.svelte");

    // WHEN the progress and the step count are read
    // THEN both count this list, so the three steps still to come add
    // themselves by an entry here and leave no empty step meanwhile
    expect(modal).toContain("{#each STEPS as step, i (step.id)}");
    expect(modal).toContain("total: STEPS.length");
    for (const step of ONBOARDING_STEPS) {
      expect(modal, step.id).toContain(`currentStep === "${step.id}"`);
    }
  });

  test("every step has its name in both languages", () => {
    // GIVEN the two catalogues
    for (const catalogue of [fr, en]) {
      const names = catalogue.onboarding_modal as Record<string, string>;

      // WHEN the label of each step is looked up
      // THEN it is there and it says something
      for (const step of ONBOARDING_STEPS) {
        const key = step.labelKey.replace("onboarding_modal.", "");
        expect(names[key]?.trim(), step.labelKey).toBeTruthy();
      }
    }
  });
});

describe("onboarding steps - no usage profile", () => {
  test("no step asks for a usage profile", () => {
    // GIVEN the steps of the first-launch modal
    const ids = ONBOARDING_STEPS.map((step) => step.id as string);
    const labels = ONBOARDING_STEPS.map((step) => step.labelKey);

    // WHEN they are read
    // THEN none is the profile step, by identifier, label or phase
    expect(ids).not.toContain("profile");
    expect(labels).not.toContain("onboarding_modal.step_profile");
    expect(ONBOARDING_STEPS.map((step) => step.phase as string)).not.toContain("profile_choice");
  });

  test("the modal draws this list and no profile selector", () => {
    // GIVEN the source of the modal
    const modal = source("./OnboardingModal.svelte");

    // WHEN its imports and step references are read
    // THEN it takes its steps from this module and mounts no profile step
    expect(modal).toContain("ONBOARDING_STEPS");
    expect(modal).not.toContain("OnboardingProfileSelector");
    expect(modal).not.toMatch(/goTo\("profile"\)/);
  });
});

describe("onboarding steps - the flow resumes where it was left", () => {
  test("each step reopens on itself after a restart", () => {
    // GIVEN the phase the backend stores while each step is open
    for (const step of ONBOARDING_STEPS) {
      // WHEN that phase is read back at the next launch
      // THEN the modal reopens on the same step, the embeddings step included
      expect(stepOfPhase(phaseOfStep(step.id)), step.id).toBe(step.id);
    }
    expect(stepOfPhase("embeddings")).toBe("embeddings");
  });

  test("a phase with no step yet reopens on the last step before it", () => {
    // GIVEN the phases of the steps still to be built, and those that close the flow
    // WHEN each is turned into a step
    // THEN each lands on the conversation, the last step that exists
    for (const phase of ["first_agent", "connection", "first_task", "guided_tour", "graduation", "done"]) {
      expect(stepOfPhase(phase), phase).toBe("chat");
    }
  });

  test("a phase stored by an earlier version reopens on a step that exists", () => {
    // GIVEN a phase name this interface does not know, the removed one included
    // WHEN each is turned into a step
    // THEN each lands on the first step
    expect(stepOfPhase("profile_choice")).toBe("welcome");
    expect(stepOfPhase("nowhere")).toBe("welcome");
  });

  test("a restart of the onboarding replays it from the first step", () => {
    // GIVEN the phase a reset onboarding answers, the one of a first launch
    // WHEN the modal reopens on it
    // THEN it opens on the language, the first step of the list
    expect(stepOfPhase("welcome")).toBe(ONBOARDING_STEPS[0].id);
    expect(ONBOARDING_STEPS[0].labelKey).toBe("onboarding_modal.step_language");
  });
});

describe("onboarding steps - the phases walked to reach a step", () => {
  test("the next step is one move", () => {
    // GIVEN a backend standing on the welcome
    // WHEN the AI step opens
    // THEN the backend is asked for that one phase
    expect(phasePath("welcome", "ai_setup")).toEqual(["ai_setup"]);
  });

  test("a step further away walks every phase between", () => {
    // GIVEN a backend standing on the AI setup, which only moves to the next phase
    // WHEN the conversation opens two steps later
    // THEN both phases are asked for, in order
    expect(phasePath("ai_setup", "acquaintance")).toEqual(["embeddings", "acquaintance"]);
  });

  test("going back, or staying, moves nothing", () => {
    // GIVEN a backend standing on the conversation
    // WHEN the modal goes back to the AI step, or reopens the same step
    // THEN the backend is not asked to move backwards
    expect(phasePath("acquaintance", "ai_setup")).toEqual([]);
    expect(phasePath("acquaintance", "acquaintance")).toEqual([]);
  });

  test("a phase this interface does not know moves nothing", () => {
    // GIVEN a stored phase outside the chain
    // WHEN a step opens
    // THEN no move is guessed
    expect(phasePath("profile_choice", "ai_setup")).toEqual([]);
  });
});
