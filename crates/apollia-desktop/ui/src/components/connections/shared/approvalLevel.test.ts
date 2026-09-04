import { describe, test, expect } from "vitest";
import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

/**
 * The approval level offered for an MCP server is `auto` or `ask`, and nothing
 * else. A third value, `readonly`, survived in five type declarations after the
 * selector stopped offering it on 2026-08-20. It was not inert: every consumer
 * derives `requires_approval` as `level === "ask"`, so the most restrictive
 * label produced the least protective setting. This guard keeps the union at
 * two values, in the shared type and in every local copy of it.
 */

const UI_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../../../..");
const QUOTED_READONLY = /"readonly"/;
const APPROVAL_UNION = /"ask"|"auto"/;

function trackedSources(): string[] {
  const listing = execFileSync("git", ["ls-files", "src"], {
    cwd: UI_ROOT,
    encoding: "utf-8",
  });
  return listing
    .split("\n")
    .filter((line) => /\.(ts|svelte)$/.test(line) && !line.endsWith(".test.ts"));
}

describe("approval level union", () => {
  test("no tracked production source declares a readonly approval level", () => {
    // GIVEN every tracked non-test .ts and .svelte source of the UI
    const sources = trackedSources();
    expect(sources.length).toBeGreaterThan(100);

    // WHEN each line is scanned for a quoted `readonly` sitting in an approval union
    const offenders: string[] = [];
    for (const source of sources) {
      const text = readFileSync(path.join(UI_ROOT, source), "utf-8");
      text.split("\n").forEach((line, index) => {
        if (QUOTED_READONLY.test(line) && APPROVAL_UNION.test(line)) {
          offenders.push(`${source}:${index + 1}`);
        }
      });
    }

    // THEN none carries one
    expect(offenders).toEqual([]);
  });

  test("positive control: the pattern itself is detected", () => {
    // GIVEN the declaration this guard exists to forbid
    const line = 'export type ApprovalLevel = "auto" | "ask" | "readonly";';

    // WHEN the same pair of patterns runs over it
    // THEN it matches, so the sweep above is green by absence, not by blindness
    expect(QUOTED_READONLY.test(line) && APPROVAL_UNION.test(line)).toBe(true);
  });
});
