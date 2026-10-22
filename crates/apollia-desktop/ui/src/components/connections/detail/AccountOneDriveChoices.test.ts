import { describe, expect, test } from "vitest";
import { render as renderToString } from "svelte/server";
import { locale, waitLocale } from "svelte-i18n";
import "$lib/i18n";

import AccountOneDriveChoices from "./AccountOneDriveChoices.svelte";
import fr from "$lib/i18n/fr.json";
import en from "$lib/i18n/en.json";

const ADA = "ada@example.test";

function escaped(text: string): string {
  return text.replaceAll("&", "&amp;").replaceAll("'", "&#39;");
}

/** True when `message` is in `html`, raw or as the renderer escapes it. */
function shows(html: string, message: string): boolean {
  return html.includes(message) || html.includes(escaped(message));
}

async function renderIn(lang: "fr" | "en", chosen: boolean): Promise<string> {
  locale.set(lang);
  await waitLocale(lang);
  const initialChoices = chosen
    ? [{ id: "fold-c", name: "Clients", is_folder: true, path: "/drive/root:/Clients" }]
    : [];
  return renderToString(AccountOneDriveChoices, { props: { accountId: ADA, initialChoices } }).body;
}

describe("AccountOneDriveChoices", () => {
  test("says in each language that agents read only what is chosen, and that nothing is yet", async () => {
    for (const [lang, catalogue] of [["fr", fr], ["en", en]] as const) {
      // GIVEN a Microsoft account with nothing chosen
      // WHEN its OneDrive choices are rendered
      const html = await renderIn(lang, false);

      // THEN the limit, the empty state and the way to choose are shown in that language
      const c = catalogue.connections.onedrive_choices;
      for (const message of [c.title, c.intro, c.none, c.browse]) {
        expect(shows(html, message)).toBe(true);
      }
      expect(html).toContain(`data-testid="onedrive-browse-${ADA}"`);
    }
  });

  test("lists a chosen folder with the way to remove it", async () => {
    // GIVEN a Microsoft account that chose its Clients folder
    // WHEN its OneDrive choices are rendered
    const html = await renderIn("en", true);

    // THEN the folder is listed with its place as a hint and a remove action
    expect(html).toContain('data-testid="onedrive-choice-fold-c"');
    expect(html).toContain('title="/drive/root:/Clients"');
    expect(html).toContain('data-testid="onedrive-forget-fold-c"');
    expect(shows(html, en.connections.onedrive_choices.none)).toBe(false);
  });
});
