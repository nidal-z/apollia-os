import { describe, expect, test } from "vitest";
import type { RegistryServerView } from "$lib/types";
import {
  argumentDescription,
  entryDescription,
  entryLabel,
  pickLocale,
} from "$lib/connections/catalogueText";

/** A selection entry as `fetch_mcp_curated` sends it: its texts in the enrichment only. */
const FILES = {
  name: "@modelcontextprotocol/server-filesystem",
  title: null,
  description: null,
  enrichment: {
    operator_label: { en: "Local Files", fr: "Fichiers locaux" },
    description: { en: "description-en", fr: "description-fr" },
    argument_descriptions: [{ en: "argument-en", fr: "argument-fr" }],
  },
} as unknown as RegistryServerView;

/** A public registry entry: one language, no enrichment. */
const REGISTRY = {
  name: "io.github.someone/weather",
  title: "Weather",
  description: "Weather forecasts",
  enrichment: null,
} as unknown as RegistryServerView;

describe("the texts of a catalogue entry", () => {
  test("a selection entry reads in the interface language", () => {
    // GIVEN a selection entry written in English and French
    // WHEN its label, description and argument are read in each language
    // THEN each language reads its own text
    expect(entryLabel(FILES, "fr")).toBe("Fichiers locaux");
    expect(entryLabel(FILES, "en")).toBe("Local Files");
    expect(entryDescription(FILES, "fr")).toBe("description-fr");
    expect(argumentDescription(FILES.enrichment, 0, "fr")).toBe("argument-fr");
  });

  test("a regional locale reads its language, an unknown one reads English", () => {
    // GIVEN a text in English and French
    const text = { en: "Local Files", fr: "Fichiers locaux" };

    // WHEN read for a regional French locale and for German
    // THEN French is read for the first, English for the second
    expect(pickLocale(text, "fr-FR")).toBe("Fichiers locaux");
    expect(pickLocale(text, "de")).toBe("Local Files");
  });

  test("a missing translation falls back, never to an empty text", () => {
    // GIVEN a text written in French only, and one with a blank French
    // WHEN read in English and in French
    // THEN the text given is read
    expect(pickLocale({ fr: "Fichiers" }, "en")).toBe("Fichiers");
    expect(pickLocale({ en: "Files", fr: "  " }, "fr")).toBe("Files");
    expect(pickLocale(null, "fr")).toBeNull();
  });

  test("a public registry entry is shown in the language it was published in", () => {
    // GIVEN a public registry entry
    // WHEN read in French
    // THEN its title and description are shown as they came
    expect(entryLabel(REGISTRY, "fr")).toBe("Weather");
    expect(entryDescription(REGISTRY, "fr")).toBe("Weather forecasts");
  });
});
