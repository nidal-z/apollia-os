import { describe, test, expect } from "vitest";
import { readFileSync, readdirSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import fr from "$lib/i18n/fr.json";
import en from "$lib/i18n/en.json";
import { CAPABILITIES, capabilityExplanationKey, capabilityLabelKey } from "./explanations";

/**
 * Every icon of capability and every badge of the AI screen explains itself on
 * hover, in French and in English, through the house `Tooltip`; none carries a
 * native `title`, which the system would draw in its own chrome.
 *
 * The rule holds by construction: a capability is drawn by `CapabilityIcon`,
 * which reads its text from the capability, and a badge by `ExplainedBadge`,
 * whose `explanation` is required. This suite reads the sources of the screen
 * and refuses what would step around them. A story that adds a surface to the
 * AI screen adds its folder or file to `SCREEN_SOURCES`.
 */

const HERE = path.dirname(fileURLToPath(import.meta.url));
const SRC = path.resolve(HERE, "../..");

/** The sources of the AI screen, relative to `src/`. */
const SCREEN_SOURCES = ["components/ai", "routes/Ai.svelte"];

/** The lucide icons that stand for a capability; only `CapabilityIcon` draws them. */
const CAPABILITY_ICONS = ["Eye", "Wrench", "Lightbulb", "Brain"];

/** The components that hold the rule, exempt from the checks they implement. */
const HOLDERS = ["CapabilityIcon.svelte", "ExplainedBadge.svelte"];

function svelteFiles(entry: string): string[] {
  const full = path.join(SRC, entry);
  if (entry.endsWith(".svelte")) return [full];
  return readdirSync(full, { withFileTypes: true })
    .filter((e) => e.isFile() && e.name.endsWith(".svelte"))
    .map((e) => path.join(full, e.name))
    .sort();
}

function lineOf(src: string, index: number): number {
  return src.slice(0, index).split("\n").length;
}

/** Opening tags of an element, with their attributes, by component or element name. */
function openingTags(src: string, name: string): { index: number; attrs: string }[] {
  const out: { index: number; attrs: string }[] = [];
  const re = new RegExp(`<${name}(?=[\\s/>])([^>]*)>`, "g");
  let m = re.exec(src);
  while (m !== null) {
    out.push({ index: m.index, attrs: m[1] ?? "" });
    m = re.exec(src);
  }
  return out;
}

/** Lines of an HTML element carrying a native `title`, literal or bound. */
export function nativeTitles(src: string): number[] {
  const out: number[] = [];
  const re = /<([a-z][a-z0-9-]*)\b([^>]*)>/g;
  let m = re.exec(src);
  while (m !== null) {
    if (/\stitle=/.test(m[2] ?? "")) out.push(lineOf(src, m.index));
    m = re.exec(src);
  }
  return out;
}

/** Lines of a badge drawn without its explanation. */
export function unexplainedBadges(src: string, file: string): number[] {
  const out: number[] = [];
  if (!HOLDERS.includes(file)) {
    for (const tag of openingTags(src, "Badge")) out.push(lineOf(src, tag.index));
  }
  for (const tag of openingTags(src, "ExplainedBadge")) {
    const explanation = /\sexplanation=(\{[^}]*\S[^}]*\}|"[^"]*\S[^"]*")/.test(tag.attrs);
    if (!explanation) out.push(lineOf(src, tag.index));
  }
  return out;
}

/** Lines of a capability icon drawn outside `CapabilityIcon`, or without its capability. */
export function unexplainedCapabilityIcons(src: string, file: string): number[] {
  const out: number[] = [];
  if (!HOLDERS.includes(file)) {
    const imports = /import\s*\{([^}]*)\}\s*from\s*"lucide-svelte"/g;
    let m = imports.exec(src);
    while (m !== null) {
      const names = (m[1] ?? "").split(",").map((n) => n.trim().split(/\s+as\s+/)[0]);
      if (names.some((n) => CAPABILITY_ICONS.includes(n ?? ""))) out.push(lineOf(src, m.index));
      m = imports.exec(src);
    }
  }
  for (const tag of openingTags(src, "CapabilityIcon")) {
    if (!/(\scapability=|\{capability\})/.test(tag.attrs)) out.push(lineOf(src, tag.index));
  }
  return out;
}

function read(catalog: unknown, key: string): unknown {
  return key.split(".").reduce<unknown>(
    (node, part) => (node && typeof node === "object" ? (node as Record<string, unknown>)[part] : undefined),
    catalog,
  );
}

describe("the checks refuse what they exist for", () => {
  test("a native title on an element is found, a component prop is not", () => {
    // GIVEN a span with a native title and a dialog component taking a title prop
    const src = '<span title="Vision">x</span>\n<ConfirmDialog title={x} />\n<div\n  title={y}></div>';
    // WHEN the native titles are read
    // THEN both HTML elements are found and the component is left alone
    expect(nativeTitles(src)).toEqual([1, 3]);
  });

  test("a bare badge and a badge without explanation are found", () => {
    // GIVEN a bare Badge, an ExplainedBadge with an empty explanation, and a correct one
    const src = [
      '<Badge size="sm">MLX</Badge>',
      '<ExplainedBadge label="MLX" explanation="" />',
      '<ExplainedBadge label="MLX" explanation={$t("ai.format_explanation")} />',
    ].join("\n");
    // WHEN the unexplained badges are read
    // THEN the first two are found
    expect(unexplainedBadges(src, "Row.svelte")).toEqual([1, 2]);
  });

  test("a capability icon outside its component is found", () => {
    // GIVEN a row importing the vision icon itself, and a CapabilityIcon without its capability
    const src = 'import { Eye, Trash2 } from "lucide-svelte";\n<CapabilityIcon />';
    // WHEN the capability icons are read
    // THEN both are found
    expect(unexplainedCapabilityIcons(src, "Row.svelte")).toEqual([1, 2]);
  });
});

describe("the AI screen explains every icon and badge on hover", () => {
  const files = SCREEN_SOURCES.flatMap(svelteFiles);

  test("the screen is read", () => {
    // GIVEN the sources of the AI screen
    // WHEN they are listed
    // THEN the route and its components are among them, so a move blinds nothing
    const names = files.map((f) => path.basename(f));
    expect(names).toContain("Ai.svelte");
    expect(names).toContain("CapabilityIcon.svelte");
    expect(names).toContain("InstalledModelsSection.svelte");
    expect(names).toContain("ModelPanel.svelte");
    expect(names).toContain("ModelInfoTab.svelte");
    expect(names).toContain("ModelDevSections.svelte");
  });

  test.each(files.map((f) => [path.relative(SRC, f), f]))("%s carries no native title", (_name, file) => {
    // GIVEN one source of the screen
    const src = readFileSync(file, "utf8");
    // WHEN its native titles are read
    // THEN there are none
    expect(nativeTitles(src)).toEqual([]);
  });

  test.each(files.map((f) => [path.relative(SRC, f), f]))(
    "%s draws every badge and capability with its explanation",
    (_name, file) => {
      // GIVEN one source of the screen
      const src = readFileSync(file, "utf8");
      const base = path.basename(file);
      // WHEN its badges and capability icons are read
      // THEN each goes through the component that requires its text
      expect(unexplainedBadges(src, base)).toEqual([]);
      expect(unexplainedCapabilityIcons(src, base)).toEqual([]);
    },
  );

  test.each([
    ["fr", fr],
    ["en", en],
  ])("every capability has its name and explanation in %s", (_loc, catalog) => {
    // GIVEN the capabilities the screen draws
    // WHEN their texts are read from the catalogue
    const missing = CAPABILITIES.flatMap((c) => [capabilityLabelKey(c), capabilityExplanationKey(c)]).filter(
      (key) => {
        const value = read(catalog, key);
        return typeof value !== "string" || value.trim() === "";
      },
    );
    // THEN none is missing
    expect(missing).toEqual([]);
  });
});
