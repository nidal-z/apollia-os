import { describe, test, expect } from "vitest";
import { nextDisclaimerChecks } from "./disclaimerChecks";

describe("the accepted-version lookup never overwrites what someone ticked", () => {
  test("keeps the boxes when the person has already touched them", () => {
    // GIVEN four boxes ticked by hand while the version hash was still
    // computing, which is the race that cost a whole Windows run
    const ticked = {
      code_on_machine: true,
      external_data: true,
      revocable: true,
      read_capabilities: true,
    };

    // WHEN the lookup comes back, saying the version was never accepted
    const next = nextDisclaimerChecks(true, false, ticked);

    // THEN what was ticked stays ticked, and "next" stays reachable
    expect(next).toEqual(ticked);
  });

  test("pre-ticks every box when the version is already accepted", () => {
    // GIVEN a wizard nobody has touched yet, on a version already accepted
    // WHEN the lookup comes back
    const next = nextDisclaimerChecks(false, true, {});

    // THEN the four acknowledgements are pre-ticked
    expect(next).toEqual({
      code_on_machine: true,
      external_data: true,
      revocable: true,
      read_capabilities: true,
    });
  });

  test("leaves an untouched wizard empty when the version is not accepted", () => {
    // GIVEN a wizard nobody has touched, on a version never accepted
    // WHEN the lookup comes back
    const next = nextDisclaimerChecks(false, false, {});

    // THEN nothing is ticked: the person has to acknowledge each one
    expect(next).toEqual({});
  });
});
