import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, test, expect } from "vitest";
import type { OnboardingState } from "$lib/types";
import { engineArrivalAsksAgain, shouldOpenOnboarding } from "./reopening";

function state(over: Partial<OnboardingState>): OnboardingState {
  return {
    phase: "welcome",
    llm_configured: false,
    stt_configured: false,
    topics_covered: [],
    mandatory_complete: false,
    companion_session_id: null,
    voice_enabled: false,
    skipped: false,
    completed: false,
    started_at: null,
    completed_at: null,
    stats: { total_time_sec: 0, actions_completed: 0, companion_questions: 0 },
    ...over,
  };
}

describe("first launch - when the modal opens", () => {
  test("a first launch never begun opens it", () => {
    // GIVEN the state of an installation nothing was done on
    // WHEN it is read at launch
    // THEN the modal opens
    expect(shouldOpenOnboarding(state({}))).toBe(true);
  });

  test("a first launch done, put off or begun does not", () => {
    // GIVEN a first launch that was finished, one that was put off, and one left midway
    const done = state({ phase: "done", completed: true, started_at: "2026-10-06T08:00:00Z" });
    const skipped = state({ skipped: true });
    const begun = state({ phase: "ai_setup", started_at: "2026-10-06T08:00:00Z" });

    // WHEN each is read at launch
    // THEN none opens the modal
    expect(shouldOpenOnboarding(done)).toBe(false);
    expect(shouldOpenOnboarding(skipped)).toBe(false);
    expect(shouldOpenOnboarding(begun)).toBe(false);
  });

  test("a completion alone is enough, even with no start recorded", () => {
    // GIVEN a state the backend stamped as completed without a start date
    const completed = state({ phase: "done", completed: true });

    // WHEN it is read
    // THEN the modal stays closed
    expect(shouldOpenOnboarding(completed)).toBe(false);
  });
});

describe("first launch - a language model that arrives", () => {
  test("the first model of a start without one asks again", () => {
    // GIVEN an application started with no language model, the modal never closed
    // WHEN a first model is configured
    // THEN the stored state is read again
    expect(engineArrivalAsksAgain({ previous: 0, count: 1, closedThisRun: false })).toBe(true);
  });

  test("once the modal was closed, it does not", () => {
    // GIVEN a first launch closed during this run, before any model was there
    // WHEN a first model is configured afterwards
    // THEN nothing is asked again: the first launch is over
    expect(engineArrivalAsksAgain({ previous: 0, count: 1, closedThisRun: true })).toBe(false);
    expect(engineArrivalAsksAgain({ previous: 0, count: 3, closedThisRun: true })).toBe(false);
  });

  test("a model added to others, or removed, never does", () => {
    // GIVEN an application that already had a language model
    // WHEN another is added, one is removed, or nothing changes
    // THEN nothing is asked again
    expect(engineArrivalAsksAgain({ previous: 1, count: 2, closedThisRun: false })).toBe(false);
    expect(engineArrivalAsksAgain({ previous: 2, count: 1, closedThisRun: false })).toBe(false);
    expect(engineArrivalAsksAgain({ previous: 0, count: 0, closedThisRun: false })).toBe(false);
  });
});

describe("first launch - the application remembers the close", () => {
  const app = readFileSync(fileURLToPath(new URL("../../App.svelte", import.meta.url)), "utf8");

  test("closing the modal marks the first launch as done, and remembers it for this run", () => {
    // GIVEN the source of the application shell
    const close = app.slice(app.indexOf("function handleOnboardingClose"), app.indexOf("</script>"));

    // WHEN what closing the modal does is read
    // THEN the close is remembered before anything is awaited, and recorded
    expect(close).toContain("onboardingClosed = true;");
    expect(close).toContain("onboardingModalOpen.set(false);");
    expect(close).toContain("void markOnboarded()");
    expect(close.indexOf("onboardingClosed = true;")).toBeLessThan(close.indexOf("void markOnboarded()"));
  });

  test("the arrival of a model is judged with that memory, twice", () => {
    // GIVEN the same source
    // WHEN the watch on the language models is read
    // THEN the close is checked before the state is read, and again once it
    // came back, so a close in between is not undone
    expect(app).toContain("closedThisRun: onboardingClosed,");
    expect(app).toContain("if (!onboardingClosed && shouldOpenOnboarding(state)) {");
  });
});
