import { describe, test, expect } from "vitest";
import { readFileSync } from "node:fs";
import { relative } from "node:path";
import en from "./en.json";
import fr from "./fr.json";
import {
  SOURCE_ROOT,
  catalogueLeaves,
  productFiles,
  withoutComments,
  type JsonObject,
} from "./catalogueReach";

/**
 * A `$t(key, { default: "English" })` is English shown to a French reader.
 *
 * `svelte-i18n` resolves the fallback before it resolves `fallbackLocale`, so
 * the hardcoded string wins over both catalogues: the French entry is never
 * read, and no guard sees it happen. Twenty-eight call sites were in that
 * state, every one of them with the key present in `en.json` and `fr.json`,
 * because the site was written before the key existed and the fallback stayed
 * behind after it was added.
 *
 * `call-site-keys.test.ts` ignores fallbacks by design, so that a key deleted
 * from the catalogues is still reported; this file is the other half, and asks
 * that no fallback exist at all when the key does.
 */

const EN_LEAVES = catalogueLeaves(en as unknown as JsonObject);
const FR_LEAVES = catalogueLeaves(fr as unknown as JsonObject);

/**
 * An i18n call with an options object: `$t("a.b", {`, `t("a.b", {`,
 * `get(t)("a.b", {`. The options object is then read to the matching brace, so
 * a `default:` on a later line is found and a `default:` in an unrelated object
 * is not.
 */
const CALL_WITH_OPTIONS =
  /(?:\$t|(?<![A-Za-z0-9_$.])tt?|get\(t\))\(\s*["'`]([a-zA-Z0-9_.]+)["'`]\s*,\s*\{/g;
const DEFAULT_ENTRY = /(?:^|[{,\s])default\s*:/;

export type Fallback = { file: string; key: string };

/** Read from the opening brace to its match, so nesting does not truncate. */
function objectLiteral(text: string, openBrace: number): string {
  let depth = 0;
  for (let i = openBrace; i < text.length; i += 1) {
    if (text[i] === "{") depth += 1;
    else if (text[i] === "}") {
      depth -= 1;
      if (depth === 0) return text.slice(openBrace, i + 1);
    }
  }
  return text.slice(openBrace);
}

/** Every `$t(key, { default: ... })` the product still carries. */
function fallbackCallSites(): Fallback[] {
  const found: Fallback[] = [];
  for (const file of productFiles()) {
    const text = withoutComments(readFileSync(file, "utf8"));
    for (const match of text.matchAll(CALL_WITH_OPTIONS)) {
      const brace = match.index + match[0].length - 1;
      if (!DEFAULT_ENTRY.test(objectLiteral(text, brace))) continue;
      found.push({ file: relative(SOURCE_ROOT, file), key: match[1] });
    }
  }
  return found;
}

const FALLBACKS = fallbackCallSites();

describe("i18n fallbacks - the scanner sees a fallback", () => {
  test("a call with a fallback is recognised, one without is not", () => {
    // GIVEN two call forms written out here rather than read from the tree
    const withFallback =
      'x = $t("chat.thinking", {\n  default: "Thinking...",\n});';
    const withValues = 'y = $t("chat.thinking", { values: { n: 1 } });';
    // WHEN each is read the way the scanner reads a file
    const seen = (source: string) =>
      [...source.matchAll(CALL_WITH_OPTIONS)].some((m) =>
        DEFAULT_ENTRY.test(objectLiteral(source, m.index + m[0].length - 1)),
      );
    // THEN the fallback is found and the plain options object is not, so a
    // green verdict below is a measure rather than a regex that never matches
    expect(seen(withFallback)).toBe(true);
    expect(seen(withValues)).toBe(false);
  });
});

describe("i18n fallbacks - no hardcoded English behind a live key", () => {
  test("no call site carries a fallback for a key the catalogues hold", () => {
    // GIVEN every `$t(key, { default: ... })` in the product tree
    // WHEN each key is looked up in both catalogues
    const shadowed = FALLBACKS.filter(
      ({ key }) => EN_LEAVES.has(key) && FR_LEAVES.has(key),
    ).map(({ file, key }) => `${file}: ${key}`);
    // THEN none is left: the fallback would win over the French entry
    expect(
      shadowed,
      `${shadowed.length} fallback(s) shadowing a live key: ${shadowed.slice(0, 10).join(", ")}`,
    ).toEqual([]);
  });

  test("no call site carries a fallback for a key the catalogues lack either", () => {
    // GIVEN the same call sites, keyed on entries no catalogue holds
    // WHEN each is listed
    const orphan = FALLBACKS.filter(
      ({ key }) => !EN_LEAVES.has(key) || !FR_LEAVES.has(key),
    ).map(({ file, key }) => `${file}: ${key}`);
    // THEN none is left either: a fallback is how a missing key stops being
    // visible, and the repair is the catalogue entry, not the English string
    expect(
      orphan,
      `${orphan.length} fallback(s) standing in for a missing key: ${orphan.join(", ")}`,
    ).toEqual([]);
  });
});
