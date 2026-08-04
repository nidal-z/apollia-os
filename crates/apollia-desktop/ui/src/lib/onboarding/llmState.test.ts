import { describe, test, expect } from "vitest";
import { deriveLlmState } from "./llmState";

describe("deriveLlmState", () => {
  test("shows a neutral checking state before the first hydration", () => {
    // GIVEN the store has not completed a single backend-list fetch
    // WHEN the state is derived with an empty list
    // THEN the step reports "checking", never "no engine available"
    expect(deriveLlmState(false, 0)).toBe("checking");
  });

  test("confirms absence only after a successful hydration", () => {
    // GIVEN one successful fetch returned an empty list
    // THEN the destructive no-engine state is legitimate
    expect(deriveLlmState(true, 0)).toBe("none");
  });

  test("is ready as soon as a backend is present", () => {
    // GIVEN at least one backend, hydrated or not (an SSE push may land
    // before the eager fetch resolves)
    expect(deriveLlmState(true, 1)).toBe("ready");
    expect(deriveLlmState(false, 2)).toBe("ready");
  });
});
