import { describe, it, expect } from "vitest";
import en from "$lib/i18n/en.json";
import fr from "$lib/i18n/fr.json";
import {
  guardrailRefusalText,
  isGuardrailRefusal,
  type GuardrailRefusedBody,
  type GuardrailTranslate,
} from "./guardrail";

type Catalogue = Record<string, unknown>;

/** A translator over one flat catalogue, with `{name}` interpolation. */
function translator(catalogue: Catalogue): GuardrailTranslate {
  return (key, { values }) => {
    const found = key
      .split(".")
      .reduce<unknown>((node, part) => (node as Catalogue | undefined)?.[part], catalogue);
    if (typeof found !== "string") throw new Error(`missing key ${key}`);
    return found.replace(/\{(\w+)\}/g, (_, name: string) => values[name] ?? `{${name}}`);
  };
}

const REFUSAL: GuardrailRefusedBody = {
  error: "loading qwen.gguf was refused by the balanced load guardrail",
  code: "guardrail_refused",
  model: "qwen.gguf",
  level: "balanced",
  estimatedGb: 18.25,
  budgetGb: 16,
};

describe("guardrail refusal", () => {
  it("is recognised by its code and its figures", () => {
    // GIVEN a refusal body, another error body, and nothing
    // WHEN each is tested
    // THEN only the refusal is recognised
    expect(isGuardrailRefusal(REFUSAL)).toBe(true);
    expect(isGuardrailRefusal({ error: "boom" })).toBe(false);
    expect(isGuardrailRefusal(null)).toBe(false);
  });

  it("reads in English with the estimate and the limit", () => {
    // GIVEN the English catalogue
    // WHEN the refusal is worded
    const text = guardrailRefusalText(REFUSAL, translator(en as Catalogue), "en");
    // THEN the figures are in the sentence, to one decimal
    expect(text.message).toContain("18.3 GB");
    expect(text.message).toContain("16.0 GB");
    expect(text.title.length).toBeGreaterThan(0);
    expect(text.suggested_action.length).toBeGreaterThan(0);
  });

  it("reads in French with the estimate and the limit", () => {
    // GIVEN the French catalogue
    // WHEN the refusal is worded
    const text = guardrailRefusalText(REFUSAL, translator(fr as Catalogue), "fr");
    // THEN the figures are in the sentence, with a decimal comma
    expect(text.message).toContain("18,3 Go");
    expect(text.message).toContain("16,0 Go");
    expect(text.title).not.toBe(
      guardrailRefusalText(REFUSAL, translator(en as Catalogue), "en").title,
    );
  });
});
