import { describe, test, expect } from "vitest";
import { readFileSync } from "node:fs";
import { join } from "node:path";

/**
 * Source-level contract for the plan review card's scroll containment.
 *
 * The test environment is node (no DOM, components are not rendered, see
 * `vitest.config.ts`), so the fix contract is pinned on the class attributes
 * themselves: the step list must cap its height in rem (a viewport-relative
 * cap exceeds the visible pane on short windows and pushes the approve and
 * reject buttons out of view) and must contain its overscroll (without it,
 * wheel events chain into the conversation scroller and the card reads as
 * broken while the user reviews the steps).
 */

function stepListClasses(file: string): string {
  const source = readFileSync(join(__dirname, file), "utf-8");
  const match = /<ol class="([^"]*)"/.exec(source);
  expect(match, `${file} should have a class-carrying <ol>`).not.toBeNull();
  // SAFETY of the non-null assertion: asserted non-null on the line above.
  return (match as RegExpExecArray)[1];
}

describe("plan review step list scroll containment", () => {
  for (const file of ["ChatPlanReview.svelte", "ChatPlanReviewBuilder.svelte"]) {
    test(`${file} caps in rem and contains overscroll`, () => {
      // GIVEN the card's step list classes
      const classes = stepListClasses(file);

      // WHEN inspecting the scroll contract
      // THEN the list scrolls, contains its overscroll, and caps in rem
      expect(classes).toContain("overflow-y-auto");
      expect(classes).toContain("overscroll-contain");
      expect(classes).toMatch(/max-h-\d+/);
      expect(classes).not.toMatch(/max-h-\[\d+vh\]/);
    });
  }
});
