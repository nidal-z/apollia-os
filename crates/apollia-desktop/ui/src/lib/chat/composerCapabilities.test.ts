import { describe, test, expect } from "vitest";
import {
  REASONING_LEVELS,
  composerChips,
  hasChips,
  withReasoningEffort,
  withThinking,
} from "./composerCapabilities";
import type { ChatModelCapabilities, SessionSettings } from "$lib/types";

function model(
  reasoning_profile: ChatModelCapabilities["reasoning_profile"],
  vision = false,
): ChatModelCapabilities {
  return {
    model: "local-model",
    local: true,
    reasoning_profile,
    vision,
    image_formats: vision ? ["png", "jpeg"] : [],
  };
}

describe("composerChips - a chip only for a capability the model has", () => {
  test("a model with no reasoning setting gets no reasoning chip", () => {
    // GIVEN a model that reasons or not on its own, without vision
    // WHEN its chips are derived
    const chips = composerChips(model("none"), {});

    // THEN nothing is offered
    expect(chips).toEqual({ thinking: null, effort: null, vision: false });
    expect(hasChips(chips)).toBe(false);
  });

  test("a model that reasons on demand gets the switch, on by default", () => {
    // GIVEN a model whose reasoning is switched on or off
    // WHEN its chips are derived for a conversation that set nothing
    const chips = composerChips(model("toggle"), {});

    // THEN the switch shows, on, and no level is offered
    expect(chips.thinking).toEqual({ on: true });
    expect(chips.effort).toBeNull();
    expect(hasChips(chips)).toBe(true);
  });

  test("the switch is off once the conversation switched reasoning off", () => {
    // GIVEN the same model in a conversation that stored reasoning off
    // WHEN its chips are derived
    const chips = composerChips(model("toggle"), { thinking: false });

    // THEN the switch shows off
    expect(chips.thinking).toEqual({ on: false });
  });

  test("a model that reads a level gets the level chip, on the model default", () => {
    // GIVEN a model that reads a level of reasoning
    // WHEN its chips are derived for a conversation that set nothing, then one
    // that chose the middle level
    const byDefault = composerChips(model("levels"), {});
    const chosen = composerChips(model("levels"), { reasoning_effort: "medium" });

    // THEN the level chip shows and the switch does not
    expect(byDefault.effort).toEqual({ level: null });
    expect(byDefault.thinking).toBeNull();
    expect(chosen.effort).toEqual({ level: "medium" });
  });

  test("a setting the model does not read shows no chip", () => {
    // GIVEN a conversation that stored a level, now on a model with a switch
    // WHEN its chips are derived
    const chips = composerChips(model("toggle"), { reasoning_effort: "high" });

    // THEN only the switch shows
    expect(chips.effort).toBeNull();
    expect(chips.thinking).toEqual({ on: true });
  });

  test("an unknown profile, or capabilities not read yet, offer no reasoning chip", () => {
    // GIVEN a remote model whose profile is unknown, and capabilities not read
    // WHEN the chips of each are derived
    const remote = composerChips(model(null), {});
    const unread = composerChips(null, null);

    // THEN neither shows a reasoning chip
    expect(remote.thinking).toBeNull();
    expect(remote.effort).toBeNull();
    expect(hasChips(unread)).toBe(false);
  });

  test("the vision chip follows the vision of the model", () => {
    // GIVEN one model that reads images and one that does not
    // WHEN their chips are derived
    const seeing = composerChips(model("none", true), {});
    const blind = composerChips(model("none", false), {});

    // THEN only the first has the vision chip
    expect(seeing.vision).toBe(true);
    expect(hasChips(seeing)).toBe(true);
    expect(blind.vision).toBe(false);
  });

  test("the menu lists high, medium, low, in that order", () => {
    // GIVEN the levels of the menu
    // WHEN they are read
    // THEN they go from the most thorough to the fastest
    expect(REASONING_LEVELS).toEqual(["high", "medium", "low"]);
  });
});

describe("a chip changes one field and keeps the others", () => {
  const stored: SessionSettings = {
    temperature: 0.2,
    instructions: "Answer in one line.",
    disabled_accesses: ["web_search"],
  };

  test("switching reasoning off writes thinking beside what was stored", () => {
    // GIVEN a conversation with a temperature, instructions and an access off
    // WHEN reasoning is switched off
    const next = withThinking(stored, false);

    // THEN only `thinking` was added
    expect(next).toEqual({ ...stored, thinking: false });
    expect(stored).not.toHaveProperty("thinking");
  });

  test("choosing a level writes it, and the default removes the field", () => {
    // GIVEN the same conversation
    // WHEN the low level is chosen, then the default of the model
    const low = withReasoningEffort(stored, "low");
    const back = withReasoningEffort(low, null);

    // THEN the level is stored, then gone: the field is not sent at all
    expect(low).toEqual({ ...stored, reasoning_effort: "low" });
    expect(back).toEqual(stored);
    expect(back).not.toHaveProperty("reasoning_effort");
  });
});
