import { describe, test, expect } from "vitest";
import { render as renderToString } from "svelte/server";
import { locale, waitLocale } from "svelte-i18n";
import "$lib/i18n";

import JournalNoticeBannerView from "./JournalNoticeBannerView.svelte";
import type { JournalNotice } from "$lib/ipc/governance";
import fr from "$lib/i18n/fr.json";
import en from "$lib/i18n/en.json";

const UNSIGNED: JournalNotice = { notice: "unsigned" };
const EXPORT_FAILED: JournalNotice = {
  notice: "daily_export_failed",
  directory: "/var/lib/apollia/journal-exports",
  detail: "permission denied",
};

/** Renders the banners through Svelte's server renderer, in `lang`. */
async function renderIn(lang: "fr" | "en", notices: JournalNotice[]): Promise<string> {
  locale.set(lang);
  await waitLocale(lang);
  return renderToString(JournalNoticeBannerView, { props: { notices } }).body;
}

/** The server renderer escapes the apostrophe of a text node. */
function contains(html: string, text: string): boolean {
  return html.includes(text) || html.includes(text.replaceAll("'", "&#39;"));
}

describe("JournalNoticeBannerView", () => {
  test("shows nothing while the journal is in order", async () => {
    // GIVEN a signed journal whose daily export is written
    // WHEN the banners are rendered
    const html = await renderIn("en", []);

    // THEN there is none
    expect(html).not.toContain("data-notice=");
  });

  test("says in both languages that the journal is not signed and that nothing is blocked", async () => {
    for (const [lang, words] of [["fr", fr], ["en", en]] as const) {
      // GIVEN a journal opened without its signing key
      // WHEN the banner is rendered
      const html = await renderIn(lang, [UNSIGNED]);

      // THEN one warning says it, in that language
      expect(html).toContain('data-notice="unsigned"');
      expect(html).toContain('role="alert"');
      expect(contains(html, words.governance.journal_notice.unsigned)).toBe(true);
    }
  });

  test("names the folder the daily export could not be written to", async () => {
    for (const [lang, words] of [["fr", fr], ["en", en]] as const) {
      // GIVEN a daily export that failed, on an unsigned journal
      // WHEN the banners are rendered
      const html = await renderIn(lang, [UNSIGNED, EXPORT_FAILED]);

      // THEN each notice has its line, and the export one names the folder
      expect(html.split("data-notice=").length - 1).toBe(2);
      expect(html).toContain('data-notice="daily_export_failed"');
      expect(
        contains(
          html,
          words.governance.journal_notice.daily_export_failed.replace(
            "{directory}",
            EXPORT_FAILED.directory,
          ),
        ),
      ).toBe(true);
    }
  });
});
