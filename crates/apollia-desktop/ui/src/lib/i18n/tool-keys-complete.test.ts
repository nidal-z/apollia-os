import { describe, expect, it } from "vitest";
import en from "./en.json";
import fr from "./fr.json";
import productionTools from "./production-tools.json";

/**
 * Every production tool must have `tools.labels.<name>` and
 * `tools.descriptions.<name>` in both catalogues.
 *
 * `resolveToolDisplay` falls back to `tools.labels.${tool_name}` /
 * `tools.descriptions.${tool_name}` for any tool without a dedicated
 * resolver, and `OperatorApprovalCard` renders `descriptionKey` with no
 * default, so a missing key surfaces as a raw `tools.descriptions.*` string
 * on the HITL approval card. `call-site-keys.test.ts` cannot catch these:
 * the keys are built from runtime data, not from literals in the source.
 *
 * `production-tools.json` is the production list; the Rust test
 * `crates/apollia-desktop/tests/tool_catalogue.rs` refuses a fixture that
 * drifted from the runtime registration.
 */

type Catalogue = Record<string, unknown>;

function lookup(catalogue: Catalogue, dotted: string): unknown {
  let node: unknown = catalogue;
  for (const part of dotted.split(".")) {
    if (typeof node !== "object" || node === null) return undefined;
    node = (node as Record<string, unknown>)[part];
  }
  return node;
}

describe("tool i18n keys - production coverage", () => {
  // GIVEN the production tool list exported by the runtime
  const tools = productionTools as string[];

  it("reads a non-empty production tool list", () => {
    // WHEN loading the fixture THEN it holds the tool surface
    expect(tools.length).toBeGreaterThan(0);
  });

  it.each([
    ["en", en as Catalogue],
    ["fr", fr as Catalogue],
  ])("defines tools.labels.* and tools.descriptions.* for every production tool in %s", (_lang, catalogue) => {
    // WHEN resolving both keys of every production tool
    const missing: string[] = [];
    for (const name of tools) {
      for (const family of ["labels", "descriptions"]) {
        const key = `tools.${family}.${name}`;
        const value = lookup(catalogue, key);
        if (typeof value !== "string" || value.length === 0) missing.push(key);
      }
    }
    // THEN none is missing or empty
    expect(missing).toEqual([]);
  });
});
