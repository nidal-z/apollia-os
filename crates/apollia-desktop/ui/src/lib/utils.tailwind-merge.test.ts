import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { cn, CUSTOM_FONT_SIZE_TIERS } from "./utils";

/**
 * The scale of `tailwind.config.ts` only reaches an element if `cn()` keeps the
 * class that carries it. tailwind-merge reads `text-*` as a colour for every
 * key it does not know as a font size, so an unregistered tier is removed by
 * any colour class written after it. That shipped: `Badge` composed
 * `text-overline` ahead of `text-info` and rendered at the inherited 16px.
 */
function declaredTiers(): string[] {
  const src = readFileSync(
    new URL("../../tailwind.config.ts", import.meta.url),
    "utf8",
  );
  const start = src.indexOf("fontSize: {");
  let depth = 0;
  let end = start;
  for (let i = src.indexOf("{", start); i < src.length; i++) {
    if (src[i] === "{") depth++;
    else if (src[i] === "}" && --depth === 0) {
      end = i;
      break;
    }
  }
  const block = src.slice(start, end);
  const keys = [...block.matchAll(/^ {8}"?([a-z0-9-]+)"?:/gm)].map((m) => m[1]);
  // `xs`, `sm` and `base` are Tailwind's own keys, which tailwind-merge knows.
  return keys.filter((k) => !["xs", "sm", "base"].includes(k));
}

describe("cn and the font-size scale", () => {
  it("registers every tier the Tailwind config declares", () => {
    // GIVEN the font-size keys the config adds on top of Tailwind's own
    const declared = declaredTiers();

    // WHEN they are compared with the list cn() hands to tailwind-merge
    const missing = declared.filter(
      (k) => !CUSTOM_FONT_SIZE_TIERS.includes(k as never),
    );

    // THEN none is absent, or its size is dropped wherever a colour follows it
    expect(declared.length).toBeGreaterThan(15);
    expect(missing).toEqual([]);
  });

  it("keeps a tier that a colour class follows in the same call", () => {
    // GIVEN a size class and a text colour, the shape every Badge composes
    const declared = declaredTiers();

    // WHEN cn merges them in that order
    const dropped = declared.filter(
      (tier) => !cn(`text-${tier}`, "text-info").includes(`text-${tier}`),
    );

    // THEN the size survives, rather than being read as a second colour
    expect(dropped).toEqual([]);
  });

  it("still resolves a genuine conflict between two tiers", () => {
    // GIVEN two size classes of the scale in one call
    // WHEN cn merges them
    const merged = cn("text-body-sm", "text-overline");

    // THEN the later one wins and the earlier is dropped
    expect(merged).toBe("text-overline");
  });
});
