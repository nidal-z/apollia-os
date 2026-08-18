import { describe, it, expect } from "vitest";

/**
 * Guard for the outbound-link opener.
 *
 * The packaged webview ignores `<a target="_blank">` and disables
 * `window.open`, so an outbound link that does not go through
 * `lib/utils/externalLink.ts` is dead on the desktop app: the click produces
 * nothing at all. Thirteen such sites had accumulated across eight screens
 * before this guard existed, and nothing in the tree could tell.
 *
 * The rule mirrors the acceptance criterion of the batch that closed them:
 * every file under `src/` carrying an outbound-open marker must also name
 * one of the two opener entry points. No exception, including the injected
 * markup of `lib/utils/markdown.ts`, whose click delegation puts
 * `openExternalUrl` in that file too.
 */

const MARKERS = ['target="_blank"', "window.open(", 'href="http'];

const OPENER_ENTRY_POINTS = ["handleExternalLinkClick", "openExternalUrl"];

/** The module that owns the opener is the one place a marker is expected. */
const OPENER_MODULE = "lib/utils/externalLink.ts";

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

/** `/src/components/chat/X.svelte` -> `components/chat/X.svelte`. */
function normalise(globKey: string): string {
  return globKey.replace(/^\/src\//, "");
}

function scannedSources(): [string, string][] {
  return Object.entries({ ...svelteSources, ...tsSources })
    .map(([key, source]): [string, string] => [normalise(key), source])
    .sort((a, b) => a[0].localeCompare(b[0]));
}

function markersIn(source: string): string[] {
  return MARKERS.filter((marker) => source.includes(marker));
}

function namesOpener(source: string): boolean {
  return OPENER_ENTRY_POINTS.some((entry) => source.includes(entry));
}

describe("outbound links all go through the opener module", () => {
  it("scans every source file under src/ and finds them", () => {
    // GIVEN the glob over the desktop UI sources
    const sources = scannedSources();

    // WHEN the set is counted
    // THEN it is not empty, so an empty scan cannot pass as a clean scan
    expect(sources.length).toBeGreaterThan(500);
    expect(sources.map(([file]) => file)).toContain(OPENER_MODULE);
  });

  it("leaves no file carrying an outbound-open marker without the opener", () => {
    // GIVEN every source file except the opener module itself
    const candidates = scannedSources().filter(([file]) => file !== OPENER_MODULE);

    // WHEN each file carrying a marker is checked for an opener entry point
    const offenders = candidates
      .filter(([, source]) => markersIn(source).length > 0)
      .filter(([, source]) => !namesOpener(source))
      .map(([file, source]) => `${file} (${markersIn(source).join(", ")})`);

    // THEN none is left: an outbound link outside the opener is a dead click
    expect(offenders).toEqual([]);
  });

  it("still reports a site that is reintroduced", () => {
    // GIVEN a file that carries a marker and never names the opener
    const reintroduced: [string, string][] = [
      ["components/demo/Reintroduced.svelte", '<a href="https://example.com" target="_blank">x</a>'],
    ];

    // WHEN the same rule is applied to it
    const offenders = reintroduced
      .filter(([, source]) => markersIn(source).length > 0)
      .filter(([, source]) => !namesOpener(source))
      .map(([file]) => file);

    // THEN the rule flags it, so a clean run above is a measurement and not a blind spot
    expect(offenders).toEqual(["components/demo/Reintroduced.svelte"]);
  });
});
