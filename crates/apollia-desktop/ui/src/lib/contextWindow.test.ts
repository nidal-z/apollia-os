import { describe, expect, it } from "vitest";
import {
  CONTEXT_WINDOW_CHOICES,
  contextWindowChoices,
  formatContextWindow,
} from "./contextWindow";

describe("contextWindowChoices", () => {
  it("lists the presets when nothing is stored", () => {
    // GIVEN no stored window
    // WHEN the choices are listed
    const choices = contextWindowChoices(null);
    // THEN they are the presets
    expect(choices).toEqual([...CONTEXT_WINDOW_CHOICES]);
  });

  it("keeps a stored value that is not a preset, in order", () => {
    // GIVEN a window set from the CLI
    // WHEN the choices are listed
    const choices = contextWindowChoices(24_000);
    // THEN it is offered between its neighbours, so opening the dialog keeps it
    expect(choices).toEqual([8_192, 16_384, 24_000, 32_768, 65_536, 131_072]);
  });
});

describe("formatContextWindow", () => {
  it("abbreviates round windows and leaves others exact", () => {
    // GIVEN a preset and an odd value
    // WHEN each is formatted
    // THEN the preset reads in k and the odd one stays exact
    expect(formatContextWindow(32_768)).toBe("32k");
    expect(formatContextWindow(24_000)).toBe("24000");
  });
});
