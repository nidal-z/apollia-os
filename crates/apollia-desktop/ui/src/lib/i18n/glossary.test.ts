import { describe, test, expect } from "vitest";
import glossary from "./glossary.json";

/**
 * The official interface vocabulary, read as data.
 *
 * `glossary.json` names, per locale, the words every screen uses for the six
 * objects and the ten screens, the terms that stay out of the interface outside
 * developer mode, and the file formats that remain allowed. A vocabulary guard
 * reads it, so its shape is checked here: a locale missing an object or a
 * screen would let the guard pass on a word nobody defined.
 */

type Locale = (typeof glossary)["fr"];

const LOCALES: Record<string, Locale> = glossary;

const OBJECTS = ["agent", "workflow", "tool", "connection", "package", "source"];

const SCREENS = [
  "chat",
  "agents",
  "connections",
  "ai",
  "review",
  "library",
  "automations",
  "permissions",
  "activity_log",
  "settings",
];

describe("glossary.json", () => {
  test("carries exactly the two interface locales", () => {
    // GIVEN the glossary as the bundler reads it
    // WHEN its locales are listed
    const locales = Object.keys(LOCALES).sort();
    // THEN they are French and English, and nothing else
    expect(locales).toEqual(["en", "fr"]);
  });

  test("names the same six objects in both locales, each with a word", () => {
    // GIVEN the French and English halves
    const { fr, en } = glossary;
    // WHEN their object keys are compared
    // THEN both carry the six objects, and no word is empty
    expect(Object.keys(fr.objects).sort()).toEqual([...OBJECTS].sort());
    expect(Object.keys(en.objects).sort()).toEqual([...OBJECTS].sort());
    for (const word of [...Object.values(fr.objects), ...Object.values(en.objects)]) {
      expect(word.trim()).not.toBe("");
    }
  });

  test("names the same ten screens in both locales, each with a word", () => {
    // GIVEN the French and English halves
    const { fr, en } = glossary;
    // WHEN their screen keys are compared
    // THEN both carry the ten screens, and no word is empty
    expect(Object.keys(fr.screens).sort()).toEqual([...SCREENS].sort());
    expect(Object.keys(en.screens).sort()).toEqual([...SCREENS].sort());
    for (const word of [...Object.values(fr.screens), ...Object.values(en.screens)]) {
      expect(word.trim()).not.toBe("");
    }
  });

  test("bans the same terms in both locales and keeps the file formats allowed", () => {
    // GIVEN the French and English halves
    const { fr, en } = glossary;
    // WHEN the banned and permitted lists are read
    // THEN they match across locales, never overlap, and the exemptions start empty
    expect([...fr.banned].sort()).toEqual([...en.banned].sort());
    expect(fr.banned_synonyms).toEqual(en.banned_synonyms);
    expect(fr.banned_synonyms.assistant).toBe("agent");
    expect(fr.permitted_formats).toEqual(["Excel", "CSV", "JSON", "PDF"]);
    expect(en.permitted_formats).toEqual(fr.permitted_formats);
    for (const format of fr.permitted_formats) {
      expect(fr.banned).not.toContain(format);
    }
    expect(fr.exemptions).toEqual([]);
    expect(en.exemptions).toEqual([]);
  });
});
