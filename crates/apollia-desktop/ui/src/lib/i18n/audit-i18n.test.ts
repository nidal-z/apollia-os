import { describe, expect, it } from "vitest";
import {
  scanSvelteSource,
  scanTsSource,
} from "../../../scripts/audit-i18n.mjs";

/**
 * The hardcoded-string guard is the test that protects this catalogue.
 * These cases pin the perimeter the rewrite added (script blocks, `.ts`
 * modules, template expressions, symbol-opened and lowercase text, copy
 * stored under a `label:`-like name) plus the shapes the previous scanner
 * already got right (colon, entity, brand whitelist, ignore directives,
 * the length rule around an adjacent expression).
 */

function scan(source: string): string[] {
  return scanSvelteSource(source).map(
    (finding: { snippet: string }) => finding.snippet,
  );
}

function scanTs(source: string): string[] {
  return scanTsSource(source).map(
    (finding: { snippet: string }) => finding.snippet,
  );
}

describe("audit-i18n scanner - markup", () => {
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

  it("reports a plain sentence", () => {
    // GIVEN a nominal French copy string
    const source = "<span>Tout est traité</span>";

    // WHEN the scanner reads it
    const snippets = scan(source);

    // THEN it is reported
    expect(snippets).toEqual(["Tout est traité"]);
  });

  it("reports a lowercase text node, the capital rule being gone", () => {
    // GIVEN the live-conversation badge that hid behind the capital rule
    const source = "<span>en cours</span>";

    // WHEN the scanner reads it
    const snippets = scan(source);

    // THEN it is reported
    expect(snippets).toEqual(["en cours"]);
  });

  it("reports a text node opened by a symbol", () => {
    // GIVEN the official badge that hid behind the initial-capital rule
    const source = "<span>✓ OFFICIEL</span>";

    // WHEN the scanner reads it
    const snippets = scan(source);

    // THEN it is reported
    expect(snippets).toEqual(["✓ OFFICIEL"]);
  });

  it("stays silent on a whitelisted brand", () => {
    // GIVEN a brand name inside a heading, followed by more markup
    const source = ["<h2>Apollia OS</h2>", "<p>{$t('x')}</p>"].join("\n");

    // WHEN the scanner reads it
    const snippets = scan(source);

    // THEN nothing is reported
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
      "<code>APOLLIA CLIENT ID VALUE</code>",
    ].join("\n");

    // WHEN the scanner reads it
    const snippets = scan(source);

    // THEN the directive suppresses the finding
    expect(snippets).toEqual([]);
  });

  it("reports a label whose literal part is short only next to an expression", () => {
    // GIVEN the memory-entry time-to-live label, three characters beside
    // `{expiresRel}` - and TTL is a word the token rules keep
    const source = "<span>Age {expiresRel}</span>";

    // WHEN the scanner reads it
    const snippets = scan(source);

    // THEN the label is reported despite being under the length floor
    expect(snippets).toEqual(["Age"]);
  });

  it("keeps ignoring a short label that no expression sits next to", () => {
    // GIVEN a text node under the length floor with nothing dynamic beside it
    const source = "<span>Age</span>";

    // WHEN the scanner reads it
    const snippets = scan(source);

    // THEN it stays below the threshold
    expect(snippets).toEqual([]);
  });

  it("still reports a string the directive does not cover", () => {
    // GIVEN a directive two lines above the offending markup
    const source = [
      "<!-- i18n-ignore: covers the next line only -->",
      "<code>SOME TECHNICAL TOKEN</code>",
      "<span>Tout est traité</span>",
    ].join("\n");

    // WHEN the scanner reads it
    const snippets = scan(source);

    // THEN only the uncovered string is reported
    expect(snippets).toEqual(["Tout est traité"]);
  });
});

describe("audit-i18n scanner - script and expressions", () => {
  it("reports a literal built inside <script>", () => {
    // GIVEN a status label assembled in the component script
    const source = [
      '<script lang="ts">',
      '  const msg = "Outil exécuté avec succès.";',
      "</script>",
      "<p>{msg}</p>",
    ].join("\n");

    // WHEN the scanner reads it
    const snippets = scan(source);

    // THEN the script literal is reported
    expect(snippets).toEqual(["Outil exécuté avec succès."]);
  });

  it("reports a single word stored under a label property", () => {
    // GIVEN the filter-chip shape that hid behind the `key:` line filter
    const source = [
      '<script lang="ts">',
      '  const FILTERS = [{ key: "all", label: "Toutes" }];',
      "</script>",
    ].join("\n");

    // WHEN the scanner reads it
    const snippets = scan(source);

    // THEN the label value is reported, the key value is not
    expect(snippets).toEqual(["Toutes"]);
  });

  it("reports a template literal inside a markup expression", () => {
    // GIVEN copy interpolated in the markup
    const source = "<span>{isDone ? ` · livré aujourd'hui` : ''}</span>";

    // WHEN the scanner reads it
    const snippets = scan(source);

    // THEN the template copy is reported
    expect(snippets).toEqual(["· livré aujourd'hui"]);
  });

  it("stays silent on catalogue keys and routed calls", () => {
    // GIVEN literals that are keys or live inside $t()
    const source = [
      '<script lang="ts">',
      '  const k = "settings.about.subtitle";',
      '  const v = $t("chat.thinking", { default: "Thinking..." });',
      "</script>",
      '<p>{$t("common.copy")}</p>',
    ].join("\n");

    // WHEN the scanner reads it
    const snippets = scan(source);

    // THEN no finding is produced
    expect(snippets).toEqual([]);
  });

  it("honours the code form of the ignore directive", () => {
    // GIVEN a deliberate confirmation token
    const source = [
      '<script lang="ts">',
      "  // i18n-ignore: confirmation token, deliberately not localized",
      '  const word = "FACTORY RESET TOKEN";',
      "</script>",
    ].join("\n");

    // WHEN the scanner reads it
    const snippets = scan(source);

    // THEN the directive suppresses the finding
    expect(snippets).toEqual([]);
  });
});

describe("audit-i18n scanner - .ts modules", () => {
  it("reports an operator sentence in a .ts module", () => {
    // GIVEN the bashDescriber shape: an English sentence in a lookup map
    const source = 'const VERBS = { pwd: "Checking current directory" };';

    // WHEN the scanner reads it
    const snippets = scanTs(source);

    // THEN the sentence is reported
    expect(snippets).toEqual(["Checking current directory"]);
  });

  it("reports a template holding one word beside an interpolation", () => {
    // GIVEN the `Reading ${f}` shape the previous tokenizer swallowed
    const source = "export function d(f: string) { return `Reading ${f}`; }";

    // WHEN the scanner reads it
    const snippets = scanTs(source);

    // THEN it is reported
    expect(snippets).toEqual(["Reading ${…}"]);
  });

  it("stays silent on comparisons, throws, logs and class lists", () => {
    // GIVEN the non-copy shapes the context rules must keep out
    const source = [
      'if (e.key === "Enter") { console.warn("not copy here"); }',
      'throw new Error("machine diagnostic message");',
      'const cls = "flex items-center gap-3 rounded-md border";',
      'const ipc = invoke("start_agent", { path });',
    ].join("\n");

    // WHEN the scanner reads it
    const snippets = scanTs(source);

    // THEN no finding is produced
    expect(snippets).toEqual([]);
  });

  it("honours an i18n-ignore-start / i18n-ignore-end region", () => {
    // GIVEN a region of language autonyms
    const source = [
      "// i18n-ignore-start: language autonyms",
      'const L = [{ code: "fr", label: "Français" }];',
      "// i18n-ignore-end",
      'const after = { label: "Toutes" };',
    ].join("\n");

    // WHEN the scanner reads it
    const snippets = scanTs(source);

    // THEN only the literal outside the region is reported
    expect(snippets).toEqual(["Toutes"]);
  });
});
