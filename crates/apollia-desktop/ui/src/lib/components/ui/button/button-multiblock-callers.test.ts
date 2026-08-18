import { describe, test, expect } from "vitest";
import { readFileSync, readdirSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

/**
 * `Button` writes `inline-flex items-center justify-center whitespace-nowrap`
 * in its base class, and every `size` except `auto` adds a fixed height. None
 * of the sizes touches the base class, so a caller that puts block-level
 * children inside a `Button` gets them laid out side by side on one clipped
 * line unless it overrides those defaults itself.
 *
 * This suite reads the component sources rather than rendering them, so it
 * covers every caller in the tree and not only the ones a render test happens
 * to mount. The recipe it enforces is the one `RecentActivityStrip` uses:
 * `size="auto"` plus a class that makes the button a full-width stacked or
 * top-aligned box.
 */

const HERE = path.dirname(fileURLToPath(import.meta.url));
const SRC_DIR = path.resolve(HERE, "../../../..");

/** Block-level children that cannot share a line inside a `whitespace-nowrap` row. */
const BLOCK_CHILD = /<(div|p|ul|ol|section|article|header|footer|h[1-6]|table|dl)\b/;

/** Alignment overrides that neutralise the base `items-center` row layout. */
const STACKING_CLASS = /\b(flex-col|items-start|items-stretch)\b/;

interface ButtonCall {
  /** Path relative to `src/`. */
  file: string;
  /** 1-indexed line of the `<Button` opening angle bracket. */
  line: number;
  /** The whole opening tag, whitespace collapsed. */
  openTag: string;
  /** Everything between the opening tag and the matching `</Button>`. */
  body: string;
}

function svelteFiles(dir: string): string[] {
  const out: string[] = [];
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) out.push(...svelteFiles(full));
    else if (entry.name.endsWith(".svelte")) out.push(full);
  }
  return out.sort();
}

/**
 * Index just past the `>` that closes the opening tag starting at `start`.
 *
 * Attribute values carry both quotes and Svelte `{...}` expressions, and an
 * expression may contain a bare `>`, so a plain `indexOf(">")` cuts the tag in
 * the wrong place. Returns -1 when the tag never closes.
 */
function endOfOpenTag(src: string, start: number): number {
  let quote: string | null = null;
  let depth = 0;
  for (let i = start; i < src.length; i++) {
    const c = src[i];
    if (quote !== null) {
      if (c === quote) quote = null;
    } else if (c === '"' || c === "'") {
      quote = c;
    } else if (c === "{") {
      depth++;
    } else if (c === "}") {
      depth--;
    } else if (c === ">" && depth === 0) {
      return i + 1;
    }
  }
  return -1;
}

/** Every `<Button>...</Button>` call in one source, self-closing tags excluded. */
function buttonCalls(src: string, file: string): ButtonCall[] {
  const out: ButtonCall[] = [];
  let idx = src.indexOf("<Button");
  while (idx !== -1) {
    const after = src[idx + "<Button".length];
    if (after !== undefined && /[A-Za-z0-9_-]/.test(after)) {
      idx = src.indexOf("<Button", idx + 1);
      continue;
    }
    const openEnd = endOfOpenTag(src, idx);
    if (openEnd === -1) break;
    const openTag = src.slice(idx, openEnd);
    if (openTag.endsWith("/>")) {
      idx = src.indexOf("<Button", openEnd);
      continue;
    }
    let depth = 1;
    let cursor = openEnd;
    while (depth > 0) {
      const nextOpen = src.indexOf("<Button", cursor);
      const nextClose = src.indexOf("</Button>", cursor);
      if (nextClose === -1) break;
      if (nextOpen !== -1 && nextOpen < nextClose) {
        const nestedEnd = endOfOpenTag(src, nextOpen);
        if (nestedEnd === -1) break;
        if (!src.slice(nextOpen, nestedEnd).endsWith("/>")) depth++;
        cursor = nestedEnd;
      } else {
        depth--;
        cursor = nextClose + "</Button>".length;
      }
    }
    out.push({
      file,
      line: src.slice(0, idx).split("\n").length,
      openTag: openTag.replace(/\s+/g, " "),
      body: src.slice(openEnd, Math.max(openEnd, cursor - "</Button>".length)),
    });
    idx = src.indexOf("<Button", openEnd);
  }
  return out;
}

/** The `class` attribute text of an opening tag, or the empty string. */
function classAttribute(openTag: string): string {
  const match = /\sclass="([^"]*)"/.exec(openTag);
  return match?.[1] ?? "";
}

/**
 * True when a multi-block caller carries the full recipe: no fixed height, a
 * width that fills its cell, and an alignment that stops the base row layout.
 */
function followsTileRecipe(openTag: string): boolean {
  if (!/\ssize="auto"/.test(openTag)) return false;
  const classes = classAttribute(openTag);
  return /\bw-full\b/.test(classes) && STACKING_CLASS.test(classes);
}

/** Multi-block `Button` callers of one source that miss the recipe. */
function offenders(src: string, file: string): ButtonCall[] {
  return buttonCalls(src, file)
    .filter((call) => BLOCK_CHILD.test(call.body))
    .filter((call) => !followsTileRecipe(call.openTag));
}

describe("Button - a multi-block caller carries the whole tile recipe", () => {
  test("every Button with block-level children uses size=auto plus a full-width stacked class", () => {
    // GIVEN every .svelte source of the desktop UI
    const files = svelteFiles(SRC_DIR);
    expect(files.length).toBeGreaterThan(0);

    // WHEN each Button call is parsed and the multi-block ones are isolated
    const multiBlock: ButtonCall[] = [];
    const missing: string[] = [];
    for (const file of files) {
      const src = readFileSync(file, "utf8");
      const relative = path.relative(SRC_DIR, file);
      for (const call of buttonCalls(src, relative)) {
        if (!BLOCK_CHILD.test(call.body)) continue;
        multiBlock.push(call);
        if (!followsTileRecipe(call.openTag)) {
          missing.push(`${call.file}:${call.line}\n    ${call.openTag}`);
        }
      }
    }

    // THEN the scan found the real population, and none of it misses the recipe
    expect(multiBlock.length).toBeGreaterThanOrEqual(4);
    expect(missing).toEqual([]);
  });

  test("RecentActivityStrip stays the worked example the recipe is read from", () => {
    // GIVEN the caller the recipe was extracted from
    const file = path.join(SRC_DIR, "components/dashboard/RecentActivityStrip.svelte");
    const src = readFileSync(file, "utf8");

    // WHEN its Button calls are parsed
    const multiBlock = buttonCalls(src, "RecentActivityStrip.svelte").filter((call) =>
      BLOCK_CHILD.test(call.body)
    );

    // THEN exactly one multi-block caller exists and it carries the recipe
    expect(multiBlock).toHaveLength(1);
    expect(followsTileRecipe(multiBlock[0]!.openTag)).toBe(true);
  });

  test("a reintroduced fixed-height card is caught", () => {
    // GIVEN a source that puts a three-block card inside a sized Button
    const faulty = [
      '<Button variant="ghost" size="sm"',
      '  class="px-3.5 py-3 rounded-[10px] text-left"',
      ">",
      '  <div class="flex"><span>Title</span></div>',
      "  <div>Description</div>",
      '  <div class="flex gap-1"><span>agent</span></div>',
      "</Button>",
    ].join("\n");

    // WHEN the same checker reads it
    const found = offenders(faulty, "Fabricated.svelte");

    // THEN it is reported, with its line
    expect(found).toHaveLength(1);
    expect(found[0]!.line).toBe(1);
  });

  test("size=auto alone does not satisfy the checker", () => {
    // GIVEN the remedy that removes the height but leaves the row layout
    const halfFixed =
      '<Button size="auto" class="px-3.5 py-3">\n  <div>a</div>\n  <div>b</div>\n</Button>';

    // WHEN the checker reads it
    const found = offenders(halfFixed, "HalfFixed.svelte");

    // THEN it is still reported, because the base row classes survive
    expect(found).toHaveLength(1);
  });
});
