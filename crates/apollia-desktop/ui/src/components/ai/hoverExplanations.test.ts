import { describe, test, expect } from "vitest";
import { readFileSync, readdirSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import fr from "$lib/i18n/fr.json";
import en from "$lib/i18n/en.json";
import { CAPABILITIES, capabilityExplanationKey, capabilityLabelKey } from "./explanations";
import { SERVER_SETTING_ROWS, SERVER_SETTINGS_TEXTS } from "../developer/serverSettings";

/**
 * Every icon of capability and every badge of the AI screen explains itself on
 * hover, in French and in English, through the house `Tooltip`; none carries a
 * native `title`, which the system would draw in its own chrome.
 *
 * The rule holds by construction: a capability is drawn by `CapabilityIcon`,
 * which reads its text from the capability, and a badge by `ExplainedBadge`,
 * whose `explanation` is required. This suite reads the sources of the screen
 * and refuses what would step around them. A change that adds a surface to the
 * AI screen adds its folder or file to `SCREEN_SOURCES`.
 *
 * The settings of the local server follow the same rule: a row is drawn by
 * `ServerSettingRow`, whose `help` is required, and reads its text from the
 * list of rows. The suite refuses a row without its text in either language.
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

/** The sources of the settings of the local server, relative to `src/`. */
const SERVER_SETTINGS_SOURCES = [
  "components/developer/ServerSettingsPanel.svelte",
  "components/developer/ServerSettingRow.svelte",
];

/** Lines of a row of the server settings drawn without its explanation. */
export function unexplainedSettingRows(src: string): number[] {
  return openingTags(src, "ServerSettingRow")
    .filter((tag) => !/\shelp=(\{[^}]*\S[^}]*\}|"[^"]*[^"\s][^"]*")/.test(tag.attrs))
    .map((tag) => lineOf(src, tag.index));
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

describe("the settings of the local server explain every row on hover", () => {
  const files = SERVER_SETTINGS_SOURCES.map((entry) => path.join(SRC, entry));

  test("a row drawn without its explanation is found", () => {
    // GIVEN a row without help, one with an empty help, and a correct one
    const src = [
      '<ServerSettingRow label="Port" testid="x" for="y">',
      '<ServerSettingRow label="Port" help="" testid="x" for="y">',
      '<ServerSettingRow label="Port" help={$t(item.help)} testid="x" for="y">',
    ].join("\n");
    // WHEN the unexplained rows are read
    // THEN the first two are found
    expect(unexplainedSettingRows(src)).toEqual([1, 2]);
  });

  test.each(files.map((f) => [path.relative(SRC, f), f]))(
    "%s carries no native title and no row without its explanation",
    (_name, file) => {
      // GIVEN one source of the panel
      const src = readFileSync(file, "utf8");
      // WHEN its native titles and its rows are read
      // THEN the system draws no tooltip there, and every row has its text
      expect(nativeTitles(src)).toEqual([]);
      expect(unexplainedSettingRows(src)).toEqual([]);
    },
  );

  test("the row shows its explanation through the house tooltip, from a required text", () => {
    // GIVEN the component every row goes through
    const src = readFileSync(path.join(SRC, "components/developer/ServerSettingRow.svelte"), "utf8");
    // WHEN its props and its markup are read
    // THEN `help` is required, and the house `Tooltip` shows it on its "?"
    expect(src).toMatch(/\n\s+help: string;/);
    expect(src).not.toMatch(/help\?:/);
    expect(src).toMatch(/<Tooltip\s+content=\{help\}/);
    expect(src).toContain("triggerTestid={`${testid}-help`}");
    // AND the panel draws its rows from the list alone
    const panel = readFileSync(path.join(SRC, "components/developer/ServerSettingsPanel.svelte"), "utf8");
    expect(openingTags(panel, "ServerSettingRow")).toHaveLength(1);
    expect(panel).toContain("help={$t(item.help)}");
  });

  test.each([
    ["fr", fr],
    ["en", en],
  ])("every row has its name and its explanation in %s", (_loc, catalog) => {
    // GIVEN the rows of the panel
    // WHEN their texts are read from the catalogue
    const keys = [
      ...SERVER_SETTING_ROWS.flatMap((row) => [row.label, row.help]),
      SERVER_SETTINGS_TEXTS.helpLabel,
      SERVER_SETTINGS_TEXTS.policyLockedHint,
    ];
    const missing = keys.filter((key) => {
      const value = read(catalog, key);
      return typeof value !== "string" || value.trim() === "";
    });
    // THEN none is missing
    expect(SERVER_SETTING_ROWS.length).toBe(10);
    expect(missing).toEqual([]);
  });

  test("the explanations differ between the two languages", () => {
    // GIVEN the explanation of each row
    // WHEN French and English are compared
    const same = SERVER_SETTING_ROWS.filter((row) => read(fr, row.help) === read(en, row.help));
    // THEN none was left untranslated
    expect(same).toEqual([]);
  });
});
