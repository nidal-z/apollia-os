import { describe, expect, it } from "vitest";
import {
  ignoredLines,
  scanMarkup,
  stripNonMarkup,
} from "../../../scripts/audit-i18n.mjs";

/**
 * The hardcoded-string guard is the test that protects this catalogue.
 * These cases pin the four shapes its character class used to get wrong:
 * a colon, an HTML entity next to a digit, a plain sentence, and a
 * whitelisted brand immediately followed by its closing tag.
 */

function scan(source: string): string[] {
  const markup = stripNonMarkup(source);
  return scanMarkup(markup, ignoredLines(source)).map(
    (finding: { snippet: string }) => finding.snippet,
  );
}

describe("audit-i18n scanner", () => {
  it("reports a text node that carries a colon", () => {
    // GIVEN the cron help line, whose only unusual character is a colon
    const source = "<p>Standard cron syntax: min hour day month weekday</p>";

    // WHEN the scanner reads it
    const snippets = scan(source);

    // THEN the whole sentence is reported
    expect(snippets).toEqual([
      "Standard cron syntax: min hour day month weekday",
    ]);
  });

  it("reports a text node that mixes an HTML entity and a digit", () => {
    // GIVEN the injected-memory footnote
    const source = "<p>Principe&nbsp;6 - la mémoire</p>";

    // WHEN the scanner reads it
    const snippets = scan(source);

    // THEN it is reported verbatim
    expect(snippets).toEqual(["Principe&nbsp;6 - la mémoire"]);
  });

  it("reports a plain sentence", () => {
    // GIVEN a nominal French copy string
    const source = "<span>Tout est traité</span>";

    // WHEN the scanner reads it
    const snippets = scan(source);

    // THEN it is reported
    expect(snippets).toEqual(["Tout est traité"]);
  });

  it("stays silent on a whitelisted brand followed by its closing tag", () => {
    // GIVEN a brand name from BRAND_WHITELIST inside a heading, as
    // AboutHero.svelte:41 writes it, followed by more markup
    const source = ["<h2>Apollia OS</h2>", "<p>{$t('x')}</p>"].join("\n");

    // WHEN the scanner reads it
    const snippets = scan(source);

    // THEN nothing is reported, the match having stopped at the tag
    expect(snippets).toEqual([]);
  });

  it("never lets a match cross the closing tag", () => {
    // GIVEN markup whose text node is followed by a Svelte block, as
    // OnboardingAiSetup.svelte:841 writes it
    const source = ["<span>GPU</span>{/if}", "</div>"].join("\n");

    // WHEN the scanner reads it
    const snippets = scan(source);

    // THEN the tag is not swallowed, so the brand whitelist still applies
    expect(snippets).toEqual([]);
  });

  it("does not mistake a Svelte expression for copy", () => {
    // GIVEN a block tag holding a comparison
    const source =
      "{#if collapsible && items.length > COLLAPSE_ITEM_THRESHOLD}<i></i>{/if}";

    // WHEN the scanner reads it
    const snippets = scan(source);

    // THEN no finding is produced
    expect(snippets).toEqual([]);
  });

  it("honours an i18n-ignore directive on the line above", () => {
    // GIVEN a technical token silenced by the documented directive
    const source = [
      "<!-- i18n-ignore: environment variable name -->",
      "<code>APOLLIA_*_CLIENT_ID</code>",
    ].join("\n");

    // WHEN the scanner reads it
    const snippets = scan(source);

    // THEN the directive suppresses the finding
    expect(snippets).toEqual([]);
  });

  it("still reports a string the directive does not cover", () => {
    // GIVEN a directive two lines above the offending markup
    const source = [
      "<!-- i18n-ignore: covers the next line only -->",
      "<code>APOLLIA_*_CLIENT_ID</code>",
      "<span>Tout est traité</span>",
    ].join("\n");

    // WHEN the scanner reads it
    const snippets = scan(source);

    // THEN only the uncovered string is reported
    expect(snippets).toEqual(["Tout est traité"]);
  });
});
