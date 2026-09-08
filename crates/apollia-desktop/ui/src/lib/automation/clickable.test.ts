import { describe, test, expect } from "vitest";
import { clickIsBlocked, type ClickTarget } from "./clickable";

/**
 * A disabled control ignores a click and the browser reports nothing, so a
 * step passes while its gesture never happened. These cover the two ways the
 * tree marks a control unusable, without a DOM: this corpus tests logic.
 */
function target(fields: { disabled?: boolean; ariaDisabled?: string }): ClickTarget {
  return {
    disabled: fields.disabled,
    getAttribute: (name: string) =>
      name === "aria-disabled" ? (fields.ariaDisabled ?? null) : null,
  };
}

describe("a click waits for a control that would ignore it", () => {
  test("a disabled button is blocked", () => {
    // GIVEN a button the component disabled, as the connector wizard does
    // until every acknowledgement is ticked
    const el = target({ disabled: true });

    // WHEN the runner asks whether a click would land
    // THEN it says no
    expect(clickIsBlocked(el)).toBe(true);
  });

  test("aria-disabled blocks a control that is not a button", () => {
    // GIVEN a control marked unusable for assistive technology
    const el = target({ ariaDisabled: "true" });

    // WHEN the runner asks
    // THEN it says no, because a person would not reach it either
    expect(clickIsBlocked(el)).toBe(true);
  });

  test("an enabled control is not blocked", () => {
    // GIVEN an ordinary enabled control
    const el = target({});

    // WHEN the runner asks
    // THEN the click goes straight through
    expect(clickIsBlocked(el)).toBe(false);
  });
});
