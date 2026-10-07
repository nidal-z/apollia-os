import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, test, expect } from "vitest";
import en from "$lib/i18n/en.json";
import fr from "$lib/i18n/fr.json";
import {
  CLOSED_PHASE,
  CLOSING_STEP,
  ONBOARDING_PHASE_ORDER,
  ONBOARDING_STEPS,
  closesModal,
  nextStep,
  phaseOfStep,
  phasePath,
  stepOfPhase,
} from "./onboardingSteps";

// What is held is the order of the seven steps, that each one stands for its
// own phase of the backend, that the usage-profile question is not one of
// them, that the modal draws this list and no other, and that the last one,
// the first task, has no screen: reaching it closes the modal.

function source(file: string): string {
  return readFileSync(fileURLToPath(new URL(file, import.meta.url)), "utf8");
}

describe("onboarding steps - the order of the first launch", () => {
  test("the steps come in the order language, AI, embeddings, who I am, first agent, connection, first task", () => {
    // GIVEN the steps of the first-launch modal
    const ids = ONBOARDING_STEPS.map((step) => step.id);

    // WHEN their order is read
    // THEN it is the order of the first launch, the embeddings model third,
    // the conversation fourth, the first agent fifth, the connection sixth
    // and the first task seventh
    expect(ids).toEqual([
      "welcome",
      "ai-setup",
      "embeddings",
      "chat",
      "first-agent",
      "connection",
      "first-task",
    ]);
    expect(ONBOARDING_STEPS.map((step) => step.labelKey)).toEqual([
      "onboarding_modal.step_language",
      "onboarding_modal.step_ai",
      "onboarding_modal.step_search",
      "onboarding_modal.step_about",
      "onboarding_modal.step_first_agent",
      "onboarding_modal.step_connection",
      "onboarding_modal.step_first_task",
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

  test("every step with a screen is drawn by the modal, and the progress counts the list", () => {
    // GIVEN the source of the modal
    const modal = source("./OnboardingModal.svelte");

    // WHEN the progress and the step count are read
    // THEN both count this list, and every step but the one the modal
    // closes on has its screen
    expect(modal).toContain("{#each STEPS as step, i (step.id)}");
    expect(modal).toContain("total: STEPS.length");
    for (const step of ONBOARDING_STEPS) {
      if (step.id === CLOSING_STEP) continue;
      expect(modal, step.id).toContain(`currentStep === "${step.id}"`);
    }
    expect(modal).not.toContain(`currentStep === "${CLOSING_STEP}"`);
    expect(modal).not.toContain(`goTo("${CLOSING_STEP}")`);
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
      if (step.id === CLOSING_STEP) continue;
      // WHEN that phase is read back at the next launch
      // THEN the modal reopens on the same step, the embeddings step included
      expect(stepOfPhase(phaseOfStep(step.id)), step.id).toBe(step.id);
    }
    expect(stepOfPhase("embeddings")).toBe("embeddings");
  });

  test("a phase past the last screen reopens on the last step that has one", () => {
    // GIVEN the phase of the first task, which has no screen, and those that close the flow
    // WHEN each is turned into a step
    // THEN each lands on the connection, the last step with a screen
    for (const phase of ["first_task", "guided_tour", "graduation", "done"]) {
      expect(stepOfPhase(phase), phase).toBe("connection");
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

describe("onboarding steps - what follows a step", () => {
  test("the first agent follows the conversation", () => {
    // GIVEN the conversation, once its profile was saved or left unsaved
    // WHEN the step after it is asked for
    // THEN it is the first agent, on its own phase of the backend
    expect(nextStep("chat")).toBe("first-agent");
    expect(phaseOfStep("first-agent")).toBe("first_agent");
  });

  test("the first task is the last step, and reaching it closes the modal", () => {
    // GIVEN the last step of the list
    const last = ONBOARDING_STEPS[ONBOARDING_STEPS.length - 1].id;

    // WHEN it is read, and the step after it asked for
    // THEN it is the first task, on its own phase, nothing follows it, and
    // it is the one step that closes the modal
    expect(last).toBe("first-task");
    expect(CLOSING_STEP).toBe("first-task");
    expect(phaseOfStep("first-task")).toBe("first_task");
    expect(nextStep(last)).toBeNull();
    expect(nextStep("connection")).toBe("first-task");
    expect(closesModal("first-task")).toBe(true);
    expect(closesModal(null)).toBe(true);
    for (const step of ONBOARDING_STEPS.slice(0, -1)) {
      expect(closesModal(step.id), step.id).toBe(false);
    }
  });

  test("the modal moves on through this function, from the conversation and from the first agent", () => {
    // GIVEN the source of the modal
    const modal = source("./OnboardingModal.svelte");

    // WHEN the two mounts are read
    // THEN each hands over to the step after it, and a completed profile
    // asked from the settings still closes the modal
    expect(modal).toContain('onclose={enriching ? onclose : () => advanceFrom("chat")}');
    expect(modal).toContain('onnext={() => advanceFrom("first-agent")}');
    expect(modal).toContain("const next = nextStep(step, passed);");
  });

  test("the connection follows the first agent, unless it is passed over", () => {
    // GIVEN the first agent, once an agent is kept or the step skipped
    // WHEN the step after it is asked for
    // THEN it is the connection, on its own phase of the backend
    expect(nextStep("first-agent")).toBe("connection");
    expect(phaseOfStep("connection")).toBe("connection");
    // AND a connection step with nothing to show is stepped over: the first
    // task follows, which closes the modal
    expect(nextStep("first-agent", ["connection"])).toBe("first-task");
    expect(nextStep("chat", ["connection"])).toBe("first-agent");
  });

  test("a first launch resumed on a connection step with nothing left returns to the first agent", () => {
    // GIVEN the source of the modal
    const modal = source("./OnboardingModal.svelte");

    // WHEN the restore of the stored phase is read
    // THEN the connection step reopens only with something to propose
    expect(modal).toContain(
      'currentStep = step === "connection" && (await connectionIsPassed()) ? "first-agent" : step;',
    );
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

describe("onboarding steps - the first launch is marked done as it closes", () => {
  test("closing walks the backend to its last phase, whatever step it stood on", () => {
    // GIVEN the phase of each step the modal can be closed from
    for (const from of ["first_agent", "connection"] as const) {
      // WHEN the phases to the closed one are asked for
      const walked = phasePath(from, CLOSED_PHASE);

      // THEN the first task is walked, and the walk ends on the phase that
      // stamps the completion
      expect(walked, from).toContain("first_task");
      expect(walked[walked.length - 1], from).toBe("done");
    }
    expect(CLOSED_PHASE).toBe("done");
  });

  test("the modal ends on the first task: the phase closed, the task asked, then the close", () => {
    // GIVEN the source of the modal
    const modal = source("./OnboardingModal.svelte");
    const finish = modal.slice(modal.indexOf("function finish(): void {"), modal.indexOf("async function advanceFrom"));

    // WHEN what ends the first launch is read
    // THEN the backend is walked to the closed phase, the first task of the
    // agent kept is asked for, and the modal closes
    expect(finish).toContain("syncBackendPhase(CLOSED_PHASE)");
    expect(finish).toContain("firstTaskRequest.set(firstTaskPlan(get(onboardingFirstAgent)));");
    expect(finish).toContain("onclose();");
    expect(modal).toContain("if (next === null || closesModal(next)) finish();");
  });

  test("putting the first launch off asks for no first task", () => {
    // GIVEN the source of the modal
    const modal = source("./OnboardingModal.svelte");
    const skip = modal.slice(modal.indexOf("async function handleSkip"), modal.indexOf("async function restoreStepFromBackend"));

    // WHEN "configure later" is read
    // THEN it records the skip and opens nothing behind it
    expect(skip).toContain("dismiss: dismissOnboarding");
    expect(skip).not.toContain("firstTaskRequest");
  });
});
