import { describe, test, expect } from "vitest";
import en from "./en.json";
import fr from "./fr.json";
import {
  DECLARED_INTERPOLATIONS,
  GUARD_FIXTURE_KEYS,
  catalogueLeaves,
  deadKeys,
  literalKeys,
  productFiles,
  shellFiles,
  type JsonObject,
} from "./catalogueReach";

/**
 * No catalogue entry without a reader.
 *
 * Keys are added when a screen is born and never removed when it dies, and
 * nothing asked the question in that direction: `call-site-keys.test.ts` checks
 * that the catalogue answers the code, `i18n-catalogue-parity.test.ts` that the
 * two locales answer each other. A quarter of the catalogue was weight nobody
 * could see, which is what made parity, duplicates and FR = EN unreadable at a
 * glance.
 *
 * The reachability model lives in `catalogueReach.ts`; this file is the verdict
 * plus the controls that keep a green from being an empty scan.
 */

const EN_LEAVES = catalogueLeaves(en as unknown as JsonObject);
const FR_LEAVES = catalogueLeaves(fr as unknown as JsonObject);
const ALL_KEYS = [
  ...new Set([...EN_LEAVES.keys(), ...FR_LEAVES.keys()]),
].sort();
const REACHED = literalKeys();

describe("i18n catalogue - the scan reaches both trees", () => {
  test("the product tree and the desktop shell are both read", () => {
    // GIVEN the two source trees the model reads
    // WHEN each is walked
    const product = productFiles();
    const shell = shellFiles();
    // THEN both carry files, so a verdict below cannot come from an empty walk
    expect(product.length).toBeGreaterThan(500);
    expect(shell.length).toBeGreaterThan(20);
  });

  test("a key spelled only by the Rust shell counts as reached", () => {
    // GIVEN `integrations.connectors.figma.auth_help`, emitted by
    // crates/apollia-desktop/src/mcp/enrichments.json and by no TypeScript
    // WHEN the reached set is consulted
    // THEN the shell pass found it, so source 3 of the model is live
    expect(REACHED.has("integrations.connectors.figma.auth_help")).toBe(true);
  });

  test("every declared interpolation still matches a catalogue key", () => {
    // GIVEN the hand-declared key builders
    // WHEN each pattern is run over the catalogue
    const inert = DECLARED_INTERPOLATIONS.filter(
      ({ pattern }) => !ALL_KEYS.some((key) => pattern.test(key)),
    ).map(({ builtBy }) => builtBy);
    // THEN none matches nothing: a pattern that has stopped matching is an
    // exemption still granted to a builder that no longer exists
    expect(
      inert,
      `declared interpolation matching nothing: ${inert.join(", ")}`,
    ).toEqual([]);
  });

  test("the exemption list of another guard is not a call site", () => {
    // GIVEN identicalLocales.ts, which names 226 keys to excuse them from the
    // FR = EN rule
    // WHEN the product walk is asked for it
    const named = productFiles().filter((path) =>
      path.endsWith("lib/i18n/identicalLocales.ts"),
    );
    // THEN it is not there, so one guard's exemption list cannot answer this
    // guard's question and make its 226 keys immortal
    expect(named).toEqual([]);
    expect(REACHED.get("settings.model_hub.filters.lang_de")).not.toContain(
      "lib/i18n/identicalLocales.ts",
    );
  });

  test("the model reports a key that nothing reaches", () => {
    // GIVEN a key name absent from every source tree and every builder
    const invented = "settings.this_key_is_not_called_by_anything";
    // WHEN the model is asked about it
    const verdict = deadKeys([invented], REACHED);
    // THEN it comes back dead, so a green verdict below is a measure and not a
    // model that says yes to everything
    expect(verdict).toEqual([invented]);
  });
});

describe("i18n catalogue - no entry without a reader", () => {
  test("every catalogue key is reached by the product", () => {
    // GIVEN every leaf of the two catalogues
    // WHEN each is matched against literals, declared interpolations, the
    // desktop shell and the named guard fixtures
    const dead = deadKeys(ALL_KEYS, REACHED);
    // THEN none is left without a reader
    expect(
      dead,
      `${dead.length} catalogue key(s) nothing reaches: ${dead.slice(0, 20).join(", ")}`,
    ).toEqual([]);
  });

  test("the named guard fixtures are all still in the catalogue", () => {
    // GIVEN the keys kept for the guards rather than for the product
    // WHEN each is looked up
    const orphan = GUARD_FIXTURE_KEYS.filter(
      (key) => !EN_LEAVES.has(key) || !FR_LEAVES.has(key),
    );
    // THEN the list holds no name the catalogues dropped, so the exemption
    // cannot outlive what it excuses
    expect(
      orphan,
      `guard fixture absent from the catalogues: ${orphan.join(", ")}`,
    ).toEqual([]);
  });
});
