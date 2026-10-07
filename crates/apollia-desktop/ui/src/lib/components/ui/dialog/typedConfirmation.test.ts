import { describe, expect, test } from "vitest";
import { typedConfirmationMatches as matches } from "./ConfirmDialog.svelte";

// Vitest runs in `node` and mounts no component: the rule the dialog applies
// to its confirm button is exported from its module block and asserted here.

describe("a confirmation that asks for a name to be typed", () => {
  test("a dialog that asks for nothing is always confirmable", () => {
    // GIVEN a confirmation with no text to type
    // WHEN anything, or nothing, is typed
    // THEN it can go through
    expect(matches(undefined, "")).toBe(true);
    expect(matches(undefined, "anything")).toBe(true);
  });

  test("the confirmation waits for the exact name", () => {
    // GIVEN the removal of the server `ollama-bureau`
    const name = "ollama-bureau";

    // WHEN nothing, a part of the name, another case, then the name are typed
    // THEN only the name, spaces around it aside, lets the removal through
    expect(matches(name, "")).toBe(false);
    expect(matches(name, "ollama")).toBe(false);
    expect(matches(name, "Ollama-Bureau")).toBe(false);
    expect(matches(name, "ollama-bureau")).toBe(true);
    expect(matches(name, "  ollama-bureau ")).toBe(true);
  });
});
