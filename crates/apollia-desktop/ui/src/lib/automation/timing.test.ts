import { describe, test, expect } from "vitest";
import { elapsedVerdict } from "./timing";

/**
 * A recipe that promises a duration fails past its budget and says what it
 * measured either way: the number is the point of the step, not only the
 * verdict.
 */
describe("a duration is held to its budget", () => {
  test("a duration under the budget passes and is reported", () => {
    // GIVEN a first launch that took a little over three minutes
    const elapsedMs = 183_421.4;

    // WHEN it is held to a budget of five minutes
    const verdict = elapsedVerdict(elapsedMs, 300_000);

    // THEN the step passes and the report carries the measure
    expect(verdict.ok).toBe(true);
    expect(verdict.elapsedMs).toBe(183_421);
    expect(verdict.detail).toBe("elapsed 183.4s since launch (budget 300.0s)");
  });

  test("a duration over the budget fails and still says what it measured", () => {
    // GIVEN a first launch that took one millisecond more than five minutes
    const elapsedMs = 300_001;

    // WHEN it is held to a budget of five minutes
    const verdict = elapsedVerdict(elapsedMs, 300_000);

    // THEN the step fails, and the measure is kept for the report
    expect(verdict.ok).toBe(false);
    expect(verdict.elapsedMs).toBe(300_001);
    expect(verdict.detail).toBe("elapsed 300.0s since launch, over the budget of 300.0s");
  });

  test("the budget itself is within the budget", () => {
    // GIVEN a duration equal to the budget
    // WHEN it is held to it
    const verdict = elapsedVerdict(300_000, 300_000);

    // THEN the step passes: the promise is "at most", not "less than"
    expect(verdict.ok).toBe(true);
  });

  test("a budget that is not a positive number is a recipe error", () => {
    // GIVEN a step written without a usable budget
    // WHEN the duration is held to it
    const zero = elapsedVerdict(10, 0);
    const missing = elapsedVerdict(10, Number.NaN);

    // THEN the step fails naming the budget, instead of passing on nothing
    expect(zero.ok).toBe(false);
    expect(zero.detail).toContain("positive maxMs");
    expect(missing.ok).toBe(false);
  });
});
