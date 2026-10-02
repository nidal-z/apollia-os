import { describe, test, expect } from "vitest";
import en from "$lib/i18n/en.json";
import fr from "$lib/i18n/fr.json";
import { unverifiedOriginKey } from "./origin";

function lookup(catalogue: unknown, key: string): unknown {
  return key
    .split(".")
    .reduce<unknown>(
      (node, part) => (node && typeof node === "object" ? (node as Record<string, unknown>)[part] : undefined),
      catalogue,
    );
}

describe("installed model origin mark", () => {
  test("a model of unverified origin carries the mark, in French and in English", () => {
    // GIVEN a model imported from a disk with an unknown digest
    const model = { origin_verified: false };

    // WHEN its mark is resolved
    const key = unverifiedOriginKey(model);

    // THEN it is the unverified origin label of both catalogues
    expect(key).not.toBeNull();
    const french = lookup(fr, key ?? "");
    expect(lookup(en, key ?? "")).toBe("unverified origin");
    expect(typeof french).toBe("string");
    expect(french).not.toBe("");
    expect(french).not.toBe("unverified origin");
  });

  test("a model of verified origin carries no mark", () => {
    // GIVEN a model whose digest matched its repository
    const model = { origin_verified: true };

    // WHEN its mark is resolved
    const key = unverifiedOriginKey(model);

    // THEN there is none
    expect(key).toBeNull();
  });
});
