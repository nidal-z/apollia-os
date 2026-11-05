import { beforeAll, describe, expect, test, vi } from "vitest";
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { get } from "svelte/store";
import { locale, t, waitLocale } from "svelte-i18n";
import "$lib/i18n";
import {
  importLanguageData,
  isUnknownLanguageData,
  languageDataToast,
} from "./languageDataImport";

// The import of a reading language from the AI screen, with the window and the
// command answered here: what is held is the command it reaches, and the toast
// each outcome ends on, in both languages.

const UNKNOWN =
  "/tmp/foo.traineddata is not language data this build reads: its SHA-256 matches none the registry pins";

beforeAll(async () => {
  locale.set("en");
  await waitLocale();
});

function say(lang: "en" | "fr", key: string, values?: Record<string, string>): string {
  locale.set(lang);
  return get(t)(key, { values });
}

describe("importing a reading language", () => {
  test("the picker is filtered on language data, and the command takes that extension alone", async () => {
    // GIVEN a picker that returns a language file, and a command that brings it in
    const pick = vi.fn(async () => "/Users/ada/Desktop/fra.traineddata");
    const importModelFile = vi.fn(async () => "/models/tessdata/fra.traineddata");

    // WHEN the import runs
    const outcome = await importLanguageData({ pick, importModelFile });

    // THEN both see `traineddata` only, and the file is named by its basename
    expect(pick).toHaveBeenCalledWith(["traineddata"]);
    expect(importModelFile).toHaveBeenCalledWith("/Users/ada/Desktop/fra.traineddata", ["traineddata"]);
    expect(outcome).toEqual({ kind: "imported", file: "fra.traineddata" });
  });

  test("a cancelled pick reaches no command and shows no toast", async () => {
    // GIVEN a picker the person closes
    const importModelFile = vi.fn(async () => "");

    // WHEN the import runs
    const outcome = await importLanguageData({ pick: async () => null, importModelFile });

    // THEN nothing is imported and nothing is said
    expect(importModelFile).not.toHaveBeenCalled();
    expect(languageDataToast(outcome)).toBeNull();
  });

  test("a success says what the welcome step says", async () => {
    // GIVEN an imported file
    const toast = languageDataToast({ kind: "imported", file: "fra.traineddata" });

    // WHEN its toast is read
    // THEN it is the message of the welcome step, a success, in both languages
    expect(toast?.key).toBe("onboarding.ai_setup.language_data_imported");
    expect(toast?.tone).toBe("success");
    expect(say("en", toast!.key, toast!.values)).toBe(
      "fra.traineddata is in place: scanned pages and pictures are read with it.",
    );
    const fr = say("fr", toast!.key, toast!.values);
    expect(fr).toContain("fra.traineddata");
    expect(fr).not.toBe(say("en", toast!.key, toast!.values));
  });

  test("a file the registry does not pin is refused with an error toast, without jargon", async () => {
    // GIVEN a command that refuses the digest of the file
    const importModelFile = vi.fn(async () => {
      throw UNKNOWN;
    });

    // WHEN the import runs
    const outcome = await importLanguageData({ pick: async () => "/tmp/foo.traineddata", importModelFile });
    const toast = languageDataToast(outcome);

    // THEN an error toast says the file is no reading language, with no digest in it
    expect(outcome).toEqual({ kind: "unknown" });
    expect(toast?.tone).toBe("error");
    for (const lang of ["en", "fr"] as const) {
      const text = say(lang, toast!.key);
      expect(text).not.toBe(toast!.key);
      expect(text).not.toMatch(/SHA|digest|registry|traineddata/i);
    }
    expect(say("en", toast!.key)).toContain("not a reading language");
    expect(say("fr", toast!.key)).not.toBe(say("en", toast!.key));
  });

  test("another failure is left to the error report", async () => {
    // GIVEN a command that cannot write into the models folder
    const error = new Error("cannot read or write /models/tessdata");

    // WHEN the import runs
    const outcome = await importLanguageData({
      pick: async () => "/tmp/fra.traineddata",
      importModelFile: async () => {
        throw error;
      },
    });

    // THEN the failure carries its error and no toast of its own
    expect(outcome).toEqual({ kind: "failed", error });
    expect(languageDataToast(outcome)).toBeNull();
    expect(isUnknownLanguageData(error)).toBe(false);
  });

  test("the AI screen offers the gesture, apart from the import of a model", () => {
    // GIVEN the sources of the AI screen and of its section of installed models
    const here = dirname(fileURLToPath(import.meta.url));
    const section = readFileSync(resolve(here, "InstalledModelsSection.svelte"), "utf8");
    const screen = readFileSync(resolve(here, "../../routes/Ai.svelte"), "utf8");

    // WHEN their wiring is read
    // THEN the button has its anchor, the screen hands it the language import,
    // and the import of a model still takes GGUF alone
    expect(section).toContain('data-testid="ai-import-language-data"');
    expect(screen).toContain("onimportlanguage={importLanguage}");
    expect(screen).toContain('importModelFile(filePath, ["gguf"])');
  });
});
