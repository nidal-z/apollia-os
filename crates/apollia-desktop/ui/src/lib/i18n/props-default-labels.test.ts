import { describe, test, expect } from "vitest";
import { readFileSync, readdirSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import en from "./en.json";
import fr from "./fr.json";

/**
 * A shared component that writes a user-visible label as a `$props()` default
 * publishes that literal on every call site which omits the property, in one
 * fixed language, whatever the interface locale is. This suite is the guard for
 * that mechanism: it reads the component sources rather than rendering them,
 * so it covers every component of `lib/components/` and not only the ones a
 * render test happens to mount.
 */

const HERE = path.dirname(fileURLToPath(import.meta.url));
const COMPONENTS_DIR = path.resolve(HERE, "../components");

/** Props whose value reaches the screen or a screen reader. */
const USER_FACING_PROP = /(\w*[Ll]abel|\w*[Tt]ext|\w*[Tt]itle|placeholder)\s*=\s*"([^"]+)"/g;
const PROPS_BLOCK = /let\s*\{([\s\S]*?)\}\s*:\s*Props\s*=\s*\$props\(\)/;

function svelteFiles(dir: string): string[] {
  const out: string[] = [];
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) out.push(...svelteFiles(full));
    else if (entry.name.endsWith(".svelte")) out.push(full);
  }
  return out.sort();
}

function relative(file: string): string {
  return path.relative(COMPONENTS_DIR, file);
}

/** The seven components this lot repaired, with the catalogue key each falls back to. */
const REPAIRED: Array<[string, string[]]> = [
  ["operator/ErrorBanner.svelte", ["common.retry", "a11y.dismiss"]],
  ["operator/FilterChipBar.svelte", ["a11y.filter_bar"]],
  ["ui/action-menu/ActionMenu.svelte", ["a11y.actions_menu"]],
  ["ui/data-table/DataTable.svelte", ["common.no_data"]],
  ["ui/dialog/ConfirmDialog.svelte", ["common.confirm", "common.cancel"]],
  ["ui/form-field/FormField.svelte", ["common.optional_label"]],
  ["ui/sheet/SheetHeader.svelte", ["a11y.close"]],
];

function lookup(catalogue: unknown, key: string): unknown {
  let node: unknown = catalogue;
  for (const part of key.split(".")) {
    if (typeof node !== "object" || node === null) return undefined;
    node = (node as Record<string, unknown>)[part];
  }
  return node;
}

describe("shared components - no user-facing string is written as a $props() default", () => {
  test("no component of lib/components declares a literal label, text, title or placeholder default", () => {
    // GIVEN every .svelte file under lib/components that destructures $props()
    const offenders: string[] = [];
    for (const file of svelteFiles(COMPONENTS_DIR)) {
      const source = readFileSync(file, "utf8");
      const block = PROPS_BLOCK.exec(source);
      if (!block) continue;
      // WHEN scanning its $props() block for a string literal default
      USER_FACING_PROP.lastIndex = 0;
      let match: RegExpExecArray | null;
      while ((match = USER_FACING_PROP.exec(block[1])) !== null) {
        offenders.push(`${relative(file)} ${match[1]} = ${JSON.stringify(match[2])}`);
      }
    }
    // THEN none is found: every default now resolves through svelte-i18n
    expect(offenders, `defaults bypassing the catalogue:\n${offenders.join("\n")}`).toEqual([]);
  });

  test("the seven repaired components import svelte-i18n and cite their fallback key", () => {
    // GIVEN the seven components whose defaults were literals
    for (const [relPath, keys] of REPAIRED) {
      const source = readFileSync(path.join(COMPONENTS_DIR, relPath), "utf8");
      // WHEN reading the source
      // THEN it pulls `t` from the catalogue and names every key it falls back to
      expect(source, `${relPath} does not import svelte-i18n`).toContain('from "svelte-i18n"');
      for (const key of keys) {
        expect(source, `${relPath} does not reference ${key}`).toContain(`$t("${key}")`);
      }
    }
  });

  test("no repaired component leaves a literal aria-label in its markup", () => {
    // GIVEN the same seven components
    const offenders: string[] = [];
    for (const [relPath] of REPAIRED) {
      const source = readFileSync(path.join(COMPONENTS_DIR, relPath), "utf8");
      // WHEN looking for an aria-label assigned a bare string
      for (const match of source.matchAll(/aria-label="([^"{}]+)"/g)) {
        offenders.push(`${relPath} aria-label="${match[1]}"`);
      }
    }
    // THEN none remains: a screen reader announces the interface locale
    expect(offenders, `hardcoded aria-labels:\n${offenders.join("\n")}`).toEqual([]);
  });

  test("every fallback key exists in both catalogues with a non-empty string", () => {
    // GIVEN the keys the seven components fall back to
    for (const [relPath, keys] of REPAIRED) {
      for (const key of keys) {
        // WHEN resolving each key against fr.json and en.json
        const frValue = lookup(fr, key);
        const enValue = lookup(en, key);
        // THEN both carry a non-empty string, so neither locale renders the raw key
        expect(typeof frValue, `${relPath}: fr.json is missing ${key}`).toBe("string");
        expect(typeof enValue, `${relPath}: en.json is missing ${key}`).toBe("string");
        expect((frValue as string).length).toBeGreaterThan(0);
        expect((enValue as string).length).toBeGreaterThan(0);
      }
    }
  });
});
