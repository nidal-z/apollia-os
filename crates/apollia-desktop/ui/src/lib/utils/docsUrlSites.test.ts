import { describe, it, expect } from "vitest";

/**
 * Guard for the documentation deep links.
 *
 * The documentation site serves both locales on the same routes, English at
 * the root and French under `/fr`. A link written as a bare literal carries no
 * locale, so a French operator who clicks it lands on the English page. Six
 * such sites had accumulated across five screens before `lib/utils/docsUrl.ts`
 * existed, and nothing in the tree could tell.
 *
 * The rule this guard holds is stated here in full: the host of the
 * documentation site appears in exactly one production module, the one that
 * builds the URLs. Test files are exempt, because asserting the resolved URL
 * is how the mapping above is measured at all.
 */

const DOCS_HOST = "docs.apollia.fr";

/** The module that owns the URLs is the one place the host is expected. */
const DOCS_MODULE = "lib/utils/docsUrl.ts";

/**
 * The catalogue names the host as prose the operator reads ("guides on
 * docs.apollia.fr"), which is copy and not a link.
 */
const CATALOGUES = ["lib/i18n/en.json", "lib/i18n/fr.json"];

const svelteSources = import.meta.glob("/src/**/*.svelte", {
  query: "?raw",
  import: "default",
  eager: true,
}) as Record<string, string>;

const tsSources = import.meta.glob("/src/**/*.ts", {
  query: "?raw",
  import: "default",
  eager: true,
}) as Record<string, string>;

const jsonSources = import.meta.glob("/src/**/*.json", {
  query: "?raw",
  import: "default",
  eager: true,
}) as Record<string, string>;

/** `/src/routes/settings/About.svelte` -> `routes/settings/About.svelte`. */
function normalise(globKey: string): string {
  return globKey.replace(/^\/src\//, "");
}

function scannedSources(): [string, string][] {
  return Object.entries({ ...svelteSources, ...tsSources, ...jsonSources })
    .map(([key, source]): [string, string] => [normalise(key), source])
    .sort((a, b) => a[0].localeCompare(b[0]));
}

function isExempt(file: string): boolean {
  return (
    file === DOCS_MODULE ||
    file.endsWith(".test.ts") ||
    CATALOGUES.includes(file)
  );
}

describe("documentation links all go through the locale-aware builder", () => {
  it("scans every source file under src/ and finds them", () => {
    // GIVEN the glob over the desktop UI sources
    const sources = scannedSources();

    // WHEN the set is counted
    // THEN it is not empty, so an empty scan cannot pass as a clean scan
    expect(sources.length).toBeGreaterThan(500);
    expect(sources.map(([file]) => file)).toContain(DOCS_MODULE);
  });

  it("leaves no production file naming the documentation host", () => {
    // GIVEN every source file except the builder, the tests and the catalogues
    const candidates = scannedSources().filter(([file]) => !isExempt(file));

    // WHEN each is checked for the host of the documentation site
    const offenders = candidates
      .filter(([, source]) => source.includes(DOCS_HOST))
      .map(([file]) => file);

    // THEN none is left: a literal there is a link with no locale
    expect(offenders).toEqual([]);
  });

  it("still reports a site that is reintroduced", () => {
    // GIVEN a file that hard-codes the host outside the builder
    const reintroduced: [string, string][] = [
      [
        "components/demo/Reintroduced.svelte",
        'const HELP = "https://docs.apollia.fr/operator-help";',
      ],
    ];

    // WHEN the same rule is applied to it
    const offenders = reintroduced
      .filter(([file]) => !isExempt(file))
      .filter(([, source]) => source.includes(DOCS_HOST))
      .map(([file]) => file);

    // THEN the rule flags it, so a clean run above is a measurement and not a blind spot
    expect(offenders).toEqual(["components/demo/Reintroduced.svelte"]);
  });
});
