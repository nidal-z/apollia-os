import { describe, test, expect } from "vitest";
import { readFileSync, readdirSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { formatDuration, formatElapsedClock } from "./utils";

/**
 * A millisecond duration has one rendering in this product, and it lives in
 * `lib/utils.ts`. Every local redefinition that appeared next to it dropped the
 * minute cap, so the same 90 second task read `1m 30s` on one screen and
 * `90.0s` on the next, and a locale fix once had to be applied twice in a
 * single commit.
 *
 * This suite holds the name space rather than the behaviour of any one screen:
 * a declaration of `formatDuration` or `fmtDuration` may exist in exactly one
 * file. It reads the sources, so a component that redeclares the helper is
 * caught whether or not a render test mounts it.
 *
 * The faulty fixtures below are assembled from fragments on purpose. Written
 * out in one piece they would put the forbidden text in this very file, and the
 * repository grep that states the same rule would report this suite as an
 * offender.
 */

const HERE = path.dirname(fileURLToPath(import.meta.url));
const SRC_DIR = path.resolve(HERE, "..");

/** The single file allowed to declare the duration formatter, relative to `src/`. */
const CANONICAL = "lib/utils.ts";

/** Keyword and name kept apart so this file never holds the pattern it forbids. */
const KEYWORDS = ["function", "const"];
const NAMES = ["formatDuration", "fmtDuration"];

/**
 * The declaration shape, deliberately without a trailing word boundary: a
 * helper named `formatDurationLong` or `formatDurationSeconds` is a second
 * answer to "where is the duration formatter" even though its body differs,
 * and this is the grep the acceptance criterion runs.
 */
const DECLARATION = new RegExp(`(${KEYWORDS.join("|")}) (${NAMES.join("|")})`, "g");

interface Declaration {
  /** Path relative to `src/`. */
  file: string;
  /** 1-indexed line of the declaration. */
  line: number;
}

function sourceFiles(dir: string): string[] {
  const out: string[] = [];
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) out.push(...sourceFiles(full));
    else if (entry.name.endsWith(".ts") || entry.name.endsWith(".svelte")) out.push(full);
  }
  return out.sort();
}

/** Every duration formatter declaration in one source. */
function declarations(src: string, file: string): Declaration[] {
  const out: Declaration[] = [];
  DECLARATION.lastIndex = 0;
  let match = DECLARATION.exec(src);
  while (match !== null) {
    out.push({ file, line: src.slice(0, match.index).split("\n").length });
    match = DECLARATION.exec(src);
  }
  return out;
}

describe("duration - one formatter, declared once", () => {
  test("formatDuration and fmtDuration are declared in lib/utils.ts and nowhere else", () => {
    // GIVEN every .ts and .svelte source of the desktop UI
    const files = sourceFiles(SRC_DIR);
    expect(files.length).toBeGreaterThan(0);

    // WHEN every declaration of the duration formatter is collected
    const found: Declaration[] = [];
    for (const file of files) {
      const src = readFileSync(file, "utf8");
      found.push(...declarations(src, path.relative(SRC_DIR, file)));
    }

    // THEN exactly one exists, in the canonical module
    expect(found.map((d) => `${d.file}:${d.line}`)).toHaveLength(1);
    expect(found[0]?.file).toBe(CANONICAL);
  });

  test("a reintroduced local copy is caught", () => {
    // GIVEN a component that redeclares the helper without the minute cap
    const faulty = [
      '<script lang="ts">',
      `  ${KEYWORDS[0]} ${NAMES[0]}(ms: number | null): string {`,
      '    if (ms === null) return "-";',
      "    if (ms < 1000) return `${ms}ms`;",
      "    return `${(ms / 1000).toFixed(1)}s`;",
      "  }",
      "</script>",
    ].join("\n");

    // WHEN the same checker reads it
    const found = declarations(faulty, "Fabricated.svelte");

    // THEN it is reported, with its line
    expect(found).toHaveLength(1);
    expect(found[0]!.line).toBe(2);
  });

  test("the short name is caught as well", () => {
    // GIVEN the trace strip variant, declared under the abbreviated name
    const faulty = `  ${KEYWORDS[0]} ${NAMES[1]}(ms: number): string {`;

    // WHEN the checker reads it
    const found = declarations(faulty, "Fabricated.svelte");

    // THEN it is reported
    expect(found).toHaveLength(1);
  });

  test("a rename that keeps the formatDuration prefix is caught too", () => {
    // GIVEN a second helper whose name still answers a grep for the formatter
    const faulty = `export ${KEYWORDS[1]} ${NAMES[0]}Short = (ms: number) => \`\${ms}ms\`;`;

    // WHEN the checker reads it
    const found = declarations(faulty, "Fabricated.ts");

    // THEN it is reported
    expect(found).toHaveLength(1);
  });

  test("the canonical caps at the minute rather than running the seconds up", () => {
    // GIVEN the durations the audit table, the agent panes and the trace strip render
    // WHEN each is formatted by the canonical
    // THEN the minute form appears past sixty seconds, on every surface at once
    expect(formatDuration(418)).toBe("418ms");
    expect(formatDuration(1800)).toBe("1.8s");
    expect(formatDuration(60_000)).toBe("1m 0s");
    expect(formatDuration(90_000)).toBe("1m 30s");
    expect(formatDuration(null)).toBe("-");
    expect(formatDuration(undefined)).toBe("-");
  });

  test("the elapsed clock keeps the readout the chat header had before the move", () => {
    // GIVEN the second counts a live conversation header shows
    // WHEN each is formatted by the helper moved out of that component
    // THEN the readout is the one the component produced in place
    expect(formatElapsedClock(0)).toBe("0s");
    expect(formatElapsedClock(45)).toBe("45s");
    expect(formatElapsedClock(60)).toBe("1m");
    expect(formatElapsedClock(90)).toBe("1m 30s");
    expect(formatElapsedClock(3600)).toBe("1h");
    expect(formatElapsedClock(3900)).toBe("1h 5m");
    expect(formatElapsedClock(3660)).toBe("1h 1m");
  });
});
