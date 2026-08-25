import { describe, test, expect } from "vitest";
import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

/**
 * `as unknown as T` launders any value into any type: it silences the
 * compiler exactly where the value's shape is least certain. Eleven of them
 * lived in production code at once, three passing a `KeyboardEvent` off as a
 * `MouseEvent`. The rule of `AGENTS.md` section 1 ("Use `unknown`, then
 * narrow") had no guard; this test is it. Test files stay exempt: a fixture
 * cast in a test is the test author's business.
 */

const UI_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const DOUBLE_CAST = /\bas unknown as\b/;

function trackedSources(): string[] {
  const listing = execFileSync("git", ["ls-files", "src"], {
    cwd: UI_ROOT,
    encoding: "utf-8",
  });
  return listing
    .split("\n")
    .filter((line) => /\.(ts|svelte)$/.test(line) && !line.endsWith(".test.ts"));
}

describe("no double cast outside tests", () => {
  test("no tracked production source contains `as unknown as`", () => {
    // GIVEN every tracked non-test .ts and .svelte source of the UI
    const sources = trackedSources();
    expect(sources.length).toBeGreaterThan(100);

    // WHEN each one is scanned for the double-cast pattern
    const offenders: string[] = [];
    for (const source of sources) {
      const text = readFileSync(path.join(UI_ROOT, source), "utf-8");
      text.split("\n").forEach((line, index) => {
        if (DOUBLE_CAST.test(line)) {
          offenders.push(`${source}:${index + 1}`);
        }
      });
    }

    // THEN none carries one
    expect(offenders).toEqual([]);
  });

  test("positive control: the pattern itself is detected", () => {
    // GIVEN a line carrying the laundering cast
    const line = "const x = value as unknown as Target;";

    // WHEN the same pattern runs over it
    // THEN it matches, so the sweep above is green by absence, not by blindness
    expect(DOUBLE_CAST.test(line)).toBe(true);
  });
});
