import { describe, test, expect } from "vitest";
import { readFileSync, readdirSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

/**
 * The product draws its own tooltips, either through `ui/tooltip/Tooltip.svelte`
 * or through a `class="tooltip"` element revealed by a `group-hover` rule. A
 * native `title` attribute on the same hover group adds a second label, drawn by
 * the operating system after its own delay, carrying the same text in the system
 * chrome instead of the product theme.
 *
 * The scope that matters is the hover group, because that is what makes the
 * house tooltip appear: a `title` anywhere between the element carrying the
 * `group` class and the `class="tooltip"` element it reveals doubles that
 * tooltip. This suite reads the sources, so it covers every group in the tree.
 */

const HERE = path.dirname(fileURLToPath(import.meta.url));
const SRC_DIR = path.resolve(HERE, "../../../..");

/** An opening tag that carries a `class` attribute. */
const TAG_WITH_CLASS = /<[a-zA-Z][a-zA-Z0-9-]*\s[^<]*?class="([^"]*)"/g;

/** The bare Tailwind `group` token, not `group-hover:` and not `group/name`. */
const GROUP_TOKEN = /(?:^|\s)group(?:\s|$)/;

/** The opening tag of a house tooltip element. */
const HOUSE_TOOLTIP = /class="tooltip(?:\s|")/g;

/** A native `title` attribute, literal or bound. */
const NATIVE_TITLE = /\stitle=/;

interface Doubling {
  /** Path relative to `src/`. */
  file: string;
  /** 1-indexed line of the house tooltip element. */
  line: number;
  /** 1-indexed line of the element that opens the hover group. */
  groupLine: number;
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

function lineOf(src: string, index: number): number {
  return src.slice(0, index).split("\n").length;
}

/** Byte offsets of every opening tag whose class carries the bare `group` token. */
function hoverGroupStarts(src: string): number[] {
  const out: number[] = [];
  TAG_WITH_CLASS.lastIndex = 0;
  let match = TAG_WITH_CLASS.exec(src);
  while (match !== null) {
    if (GROUP_TOKEN.test(match[1] ?? "")) out.push(match.index);
    match = TAG_WITH_CLASS.exec(src);
  }
  return out;
}

/** House tooltips of one source whose hover group also carries a native `title`. */
function doublings(src: string, file: string): Doubling[] {
  const groups = hoverGroupStarts(src);
  const out: Doubling[] = [];
  HOUSE_TOOLTIP.lastIndex = 0;
  let match = HOUSE_TOOLTIP.exec(src);
  while (match !== null) {
    const at = match.index;
    let groupStart = -1;
    for (const candidate of groups) {
      if (candidate < at) groupStart = candidate;
      else break;
    }
    if (groupStart !== -1 && NATIVE_TITLE.test(src.slice(groupStart, at))) {
      out.push({ file, line: lineOf(src, at), groupLine: lineOf(src, groupStart) });
    }
    match = HOUSE_TOOLTIP.exec(src);
  }
  return out;
}

/** Every house tooltip in one source, doubled or not. */
function houseTooltipCount(src: string): number {
  HOUSE_TOOLTIP.lastIndex = 0;
  let count = 0;
  while (HOUSE_TOOLTIP.exec(src) !== null) count++;
  return count;
}

describe("tooltip - no hover group carries both a house tooltip and a native title", () => {
  test("no .svelte source doubles a house tooltip with a native title attribute", () => {
    // GIVEN every .svelte source of the desktop UI
    const files = svelteFiles(SRC_DIR);
    expect(files.length).toBeGreaterThan(0);

    // WHEN each hover group holding a house tooltip is checked for a native title
    let tooltips = 0;
    const doubled: string[] = [];
    for (const file of files) {
      const src = readFileSync(file, "utf8");
      const relative = path.relative(SRC_DIR, file);
      tooltips += houseTooltipCount(src);
      for (const hit of doublings(src, relative)) {
        doubled.push(`${hit.file}:${hit.line} (hover group opened at line ${hit.groupLine})`);
      }
    }

    // THEN the scan saw real tooltips, and none of them is doubled
    expect(tooltips).toBeGreaterThanOrEqual(3);
    expect(doubled).toEqual([]);
  });

  test("a reintroduced native title on a rail entry is caught", () => {
    // GIVEN a hover group whose button carries both markers
    const faulty = [
      '<div class="rail-item group relative mb-1">',
      "  <button",
      '    type="button"',
      "    aria-label={label}",
      "    title={label}",
      "  >",
      "    <item.Icon size={17} />",
      "  </button>",
      '  <span class="tooltip pointer-events-none absolute opacity-0 group-hover:opacity-100">',
      "    {label}",
      "  </span>",
      "</div>",
    ].join("\n");

    // WHEN the same checker reads it
    const found = doublings(faulty, "Fabricated.svelte");

    // THEN it is reported, and it names the hover group it belongs to
    expect(found).toHaveLength(1);
    expect(found[0]!.line).toBe(9);
    expect(found[0]!.groupLine).toBe(1);
  });

  test("a native title outside any hover group is left alone", () => {
    // GIVEN the avatar shape: a title on an element that reveals no house tooltip
    const clean = [
      '<div class="rail-item group relative mb-1">',
      "  <button aria-label={label}></button>",
      '  <span class="tooltip opacity-0 group-hover:opacity-100">{label}</span>',
      "</div>",
      '<div class="mt-1">',
      '  <span class="avatar-warm" title={$t("settings.profile.title")}></span>',
      "</div>",
    ].join("\n");

    // WHEN the checker reads it
    const found = doublings(clean, "Clean.svelte");

    // THEN nothing is reported, the title having no house tooltip to double
    expect(found).toEqual([]);
  });
});
