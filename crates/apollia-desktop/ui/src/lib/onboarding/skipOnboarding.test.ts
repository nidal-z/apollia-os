import { describe, it, expect } from "vitest";
import { skipOnboarding, type SkipOnboardingDeps } from "./skipOnboarding";

interface Recorder {
  deps: SkipOnboardingDeps;
  reported: unknown[];
  closed: number;
}

function recorder(dismiss: () => Promise<void>): Recorder {
  const reported: unknown[] = [];
  const state = { closed: 0 };
  return {
    reported,
    get closed() {
      return state.closed;
    },
    deps: {
      dismiss,
      report: (err) => {
        reported.push(err);
      },
      close: () => {
        state.closed += 1;
      },
    },
  };
}

describe("skipOnboarding", () => {
  it("closes the modal once the backend recorded the skip", async () => {
    // GIVEN a backend that persists `onboarding_skipped`
    const rec = recorder(() => Promise.resolve());

    // WHEN the operator asks to configure later
    const done = await skipOnboarding(rec.deps);

    // THEN the modal closes and nothing is reported
    expect(done).toBe(true);
    expect(rec.closed).toBe(1);
    expect(rec.reported).toEqual([]);
  });

  it("keeps the modal open and reports when the skip was not recorded", async () => {
    // GIVEN a backend that refuses to persist the skip
    const refusal = new Error("dismiss_onboarding failed");
    const rec = recorder(() => Promise.reject(refusal));

    // WHEN the operator asks to configure later
    const done = await skipOnboarding(rec.deps);

    // THEN the failure reaches the operator and the modal stays open, so the
    // state at the next launch matches what was read
    expect(done).toBe(false);
    expect(rec.reported).toEqual([refusal]);
    expect(rec.closed).toBe(0);
  });

  it("hands the raw rejection to the reporter, whatever its shape", async () => {
    // GIVEN a backend that rejects with a bare string, as Tauri commands do
    const rec = recorder(() => Promise.reject("runtime unreachable"));

    // WHEN the operator asks to configure later
    await skipOnboarding(rec.deps);

    // THEN the reporter receives the value untouched, so it can humanize it
    expect(rec.reported).toEqual(["runtime unreachable"]);
  });
});
