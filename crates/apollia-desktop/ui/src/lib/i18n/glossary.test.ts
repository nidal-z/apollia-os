import { describe, test, expect } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import glossary from "./glossary.json";
import frCatalogue from "./fr.json";
import enCatalogue from "./en.json";

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
  "batches",
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

  test("names the same eleven screens in both locales, each with a word", () => {
    // GIVEN the French and English halves
    const { fr, en } = glossary;
    // WHEN their screen keys are compared
    // THEN both carry the eleven screens, and no word is empty
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
    // THEN they match across locales and never overlap
    expect([...fr.banned].sort()).toEqual([...en.banned].sort());
    expect(fr.banned_synonyms).toEqual(en.banned_synonyms);
    expect(fr.banned_synonyms.assistant).toBe("agent");
    expect(fr.permitted_formats).toEqual(["Excel", "CSV", "JSON", "PDF", "GGUF", "MLX"]);
    expect(en.permitted_formats).toEqual(fr.permitted_formats);
    for (const format of fr.permitted_formats) {
      expect(fr.banned).not.toContain(format);
    }
  });

  test("holds no exemption: every text outside developer mode speaks the glossary", () => {
    // GIVEN the exemption lists the vocabulary guard reads as a ratchet
    const { fr, en } = glossary;
    // WHEN each list is read
    // THEN both are empty, so no catalogue key is excused from the banned terms
    expect(fr.exemptions).toEqual([]);
    expect(en.exemptions).toEqual([]);
  });

  test("the steps panel of the chat carries the glossary word, not the name of the Activity log screen", () => {
    // GIVEN the title of the chat panel that lists what the agent does, and the
    // title of the screen that keeps the record of every action
    const catalogues = { fr: frCatalogue, en: enCatalogue };
    for (const lang of ["fr", "en"] as const) {
      // WHEN both titles are read in one locale
      const panel = catalogues[lang].plan_session.tab_journal;
      const screen = catalogues[lang].nav.journal;
      // THEN the panel carries the word of the glossary, and the two no longer share one
      expect(panel).toBe(glossary[lang].chat_steps_panel);
      expect(screen).toBe(glossary[lang].screens.activity_log);
      expect(panel).not.toBe(screen);
      expect(panel).not.toBe(catalogues[lang].journal.title);
    }
  });

  test("the chat mounts the steps panel under that title", () => {
    // GIVEN the source of the chat screen
    const chat = readFileSync(resolve(__dirname, "../../routes/Chat.svelte"), "utf-8");
    // WHEN the tab of its side panel is located by its test id
    const tab = chat.slice(chat.indexOf('data-testid="chat-inspector-journal-tab"'));
    // THEN the first text it renders is the key the catalogues give that word
    expect(tab.slice(0, tab.indexOf("</button>"))).toContain('$t("plan_session.tab_journal")');
  });
});
