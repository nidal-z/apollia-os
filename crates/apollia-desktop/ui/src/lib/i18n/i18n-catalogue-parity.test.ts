import { describe, test, expect } from "vitest";
import en from "./en.json";
import fr from "./fr.json";

/**
 * Parity of the two desktop catalogues, over their whole surface.
 *
 * `index.ts` initialises svelte-i18n with `fallbackLocale: "en"` on a default
 * locale of `fr`, so a key that only the French catalogue is missing renders
 * English text: no console warning, no thrown error, no failing render. Nothing
 * else in the tree looks at both files at once. `i18n-tools.test.ts` compares
 * them on the `tools.*` subtree only, and `scripts/audit-i18n.mjs` looks for
 * hardcoded strings in components without ever opening a catalogue.
 */

type JsonNode = Record<string, unknown>;

/**
 * Every leaf path of a catalogue, dotted.
 *
 * A leaf is anything that is not a plain object, arrays and numbers included,
 * so a value whose shape diverges between the two files still compares as one
 * path instead of dropping out of the comparison.
 */
function collectLeaves(node: JsonNode, prefix = ""): string[] {
  const paths: string[] = [];
  for (const [key, value] of Object.entries(node)) {
    const full = prefix ? `${prefix}.${key}` : key;
    if (typeof value === "object" && value !== null && !Array.isArray(value)) {
      paths.push(...collectLeaves(value as JsonNode, full));
    } else {
      paths.push(full);
    }
  }
  return paths;
}

/**
 * The paths of `source` that `target` does not carry, as a report the failure
 * output prints in full. An empty string means parity holds in that direction.
 */
function missingFrom(
  source: string[],
  sourceName: string,
  target: string[],
  targetName: string,
): string {
  const known = new Set(target);
  const missing = source.filter((path) => !known.has(path)).sort();
  if (missing.length === 0) return "";
  const header = `${missing.length} key(s) present in ${sourceName} and absent from ${targetName}:`;
  return [header, ...missing.map((path) => `  ${path}`)].join("\n");
}

const EN_LEAVES = collectLeaves(en as unknown as JsonNode);
const FR_LEAVES = collectLeaves(fr as unknown as JsonNode);

describe("i18n catalogue parity", () => {
  test("every key of en.json exists in fr.json", () => {
    // GIVEN the two catalogues as the application loads them
    // WHEN listing the leaf paths English carries and French does not
    // THEN the list is empty, so no French reader silently gets English text
    expect(missingFrom(EN_LEAVES, "en.json", FR_LEAVES, "fr.json")).toBe("");
  });

  test("every key of fr.json exists in en.json", () => {
    // GIVEN the two catalogues as the application loads them
    // WHEN listing the leaf paths French carries and English does not
    // THEN the list is empty, so no English reader gets a raw key identifier
    expect(missingFrom(FR_LEAVES, "fr.json", EN_LEAVES, "en.json")).toBe("");
  });

  test("both catalogues carry a non-trivial number of keys", () => {
    // GIVEN the two catalogues as the application loads them
    // WHEN counting their leaf paths
    // THEN neither is near-empty, so a parity that holds is a measured parity
    //      and not the vacuous parity of two files that failed to load
    expect(EN_LEAVES.length).toBeGreaterThan(1000);
    expect(FR_LEAVES.length).toBeGreaterThan(1000);
  });
});
