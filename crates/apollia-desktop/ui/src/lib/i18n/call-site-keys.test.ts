import { describe, test, expect } from "vitest";
import { readFileSync, readdirSync, statSync } from "node:fs";
import { dirname, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import en from "./en.json";
import fr from "./fr.json";

/**
 * The catalogues answer to the code, not the other way round.
 *
 * `svelte-i18n` is initialised with `initialLocale: "fr"` and
 * `fallbackLocale: "en"` (`lib/i18n/index.ts`). A key absent from `fr.json`
 * alone shows English to a French speaker; a key absent from both shows its own
 * identifier. Neither raises. A hardcoded `default:` at the call site hides the
 * first case from every guard while still showing English to the user, so this
 * scanner ignores fallbacks entirely: it reads the key, and asks the two
 * catalogues whether they carry it.
 */

type JsonObject = Record<string, unknown>;

const HERE = dirname(fileURLToPath(import.meta.url));
const SOURCE_ROOT = resolve(HERE, "../..");

/**
 * The call forms actually present in this tree: the store form in markup, the
 * plain function in script, the `get`-wrapped store outside a component, and
 * the `labelKey` family carried as component props or route metadata. The key
 * is never spelled out here as a quoted literal inside one of those forms: the
 * scan below reads comments too, and a worked example in prose would come
 * back as a call site nothing renders.
 *
 * The negative lookbehind on the second form is what keeps `api.set("a.b")`
 * and other `…t(` method calls out of the set.
 */
const CALL_FORMS: RegExp[] = [
  /\$t\(\s*["'`]([a-zA-Z0-9_.]+)["'`]/g,
  /(?<![A-Za-z0-9_$.])tt?\(\s*["'`]([a-zA-Z0-9_.]+)["'`]/g,
  /get\(t\)\(\s*["'`]([a-zA-Z0-9_.]+)["'`]/g,
  /(?:labelKey|i18nKey|titleKey|descKey|Key)\s*[:=]\s*["'`]([a-zA-Z0-9_.]+)["'`]/g,
];

/**
 * Usage examples in doc comments spell call forms out with placeholder keys,
 * `$t('...')` among them. They are documentation, not call sites, and counting
 * them would make this guard red on prose. Block comments and HTML comments go
 * whole; line comments only when the `//` opens the line, so that a URL inside
 * a string never truncates the code that follows it.
 */
function withoutComments(text: string): string {
  return text
    .replace(/<!--[\s\S]*?-->/g, "")
    .replace(/\/\*[\s\S]*?\*\//g, "")
    .replace(/^[ \t]*\/\/.*$/gm, "");
}

function sourceFiles(root: string): string[] {
  const found: string[] = [];
  for (const entry of readdirSync(root)) {
    if (entry === "node_modules") continue;
    const full = join(root, entry);
    if (statSync(full).isDirectory()) {
      found.push(...sourceFiles(full));
    } else if (full.endsWith(".svelte") || full.endsWith(".ts")) {
      found.push(full);
    }
  }
  return found;
}

/** Every statically resolvable i18n key the tree asks for, with its call sites. */
function scanCallSites(): Map<string, string[]> {
  const sites = new Map<string, string[]>();
  for (const file of sourceFiles(SOURCE_ROOT)) {
    const text = withoutComments(readFileSync(file, "utf8"));
    for (const form of CALL_FORMS) {
      for (const match of text.matchAll(form)) {
        const name = match[1];
        // A single segment is never a catalogue leaf here, and the noise from
        // `split(".")` and field identifiers lands there.
        if (!name.includes(".")) continue;
        const where = sites.get(name) ?? [];
        where.push(relative(SOURCE_ROOT, file));
        sites.set(name, where);
      }
    }
  }
  return sites;
}

function leafAt(catalogue: JsonObject, name: string): string | undefined {
  let node: unknown = catalogue;
  for (const segment of name.split(".")) {
    if (typeof node !== "object" || node === null) return undefined;
    node = (node as JsonObject)[segment];
  }
  return typeof node === "string" ? node : undefined;
}

const CALL_SITES = scanCallSites();
const REQUESTED = [...CALL_SITES.keys()].sort();

const CATALOGUES: [string, JsonObject][] = [
  ["en.json", en as unknown as JsonObject],
  ["fr.json", fr as unknown as JsonObject],
];

/**
 * The sixteen keys this repair moved out of hardcoded call-site fallbacks and
 * into the two catalogues. They are listed by hand because the point of the
 * list is the second assertion: a French entry that repeats its English
 * counterpart is the defect coming back under a key that now exists.
 *
 * `chat.assistant` is deliberately absent from the divergence list: "Assistant"
 * is the same word in both languages.
 */
const REPAIRED = [
  "chat.ask_user_pending",
  "chat.ask_user_qa_label",
  "chat.assistant",
  "chat.rate_limit.too_fast_countdown",
  "chat.rate_limit.too_many_countdown",
  "chat.reasoning.command_label",
  "chat.reasoning.error_label",
  "chat.reasoning.result_label",
  "chat.reasoning.running_hint",
  "chat.reasoning.target_label",
  "chat.reasoning.toggle_thought",
  "chat.retry.badge_title",
  "chat.retry.timeline_label",
  "chat.you",
  "common.copy",
  "settings.profile.title",
];

const IDENTICAL_ACROSS_LOCALES = new Set(["chat.assistant"]);

describe("i18n call sites - the scanner reaches the tree", () => {
  test("the scan covers the source tree and resolves a known key", () => {
    // GIVEN the scanner walking crates/apollia-desktop/ui/src
    // WHEN it collects every statically resolvable i18n key
    const files = sourceFiles(SOURCE_ROOT);
    // THEN it has read the tree, and a key known to be called is in the set,
    // so a green verdict below cannot come from an empty scan
    expect(files.length).toBeGreaterThan(500);
    expect(REQUESTED.length).toBeGreaterThan(1000);
    expect(CALL_SITES.has("chat.rewrite.button_tooltip")).toBe(true);
  });

  test("every key this repair introduced is still called by the tree", () => {
    // GIVEN the sixteen keys the repair added to both catalogues
    // WHEN each is looked up in the scanned call sites
    const unused = REPAIRED.filter((name) => !CALL_SITES.has(name));
    // THEN each one still has a caller, so none became catalogue weight
    expect(unused, `no call site left for: ${unused.join(", ")}`).toEqual([]);
  });
});

describe.each(CATALOGUES)("i18n call sites - %s answers the code", (file, catalogue) => {
  test("every key a call site asks for exists", () => {
    // GIVEN every i18n key the source tree references literally
    // WHEN each is resolved against the catalogue
    const missing = REQUESTED.filter((name) => leafAt(catalogue, name) === undefined);
    // THEN none is absent: an absent key shows English or its own identifier
    expect(missing, `${file} is missing ${missing.length} key(s): ${missing.join(", ")}`).toEqual(
      [],
    );
  });

  test("every key this repair introduced resolves to a non-empty string", () => {
    // GIVEN the sixteen keys the repair added
    // WHEN each is resolved against the catalogue
    const empty = REPAIRED.filter((name) => (leafAt(catalogue, name) ?? "").trim() === "");
    // THEN each carries text
    expect(empty, `${file} has no text for: ${empty.join(", ")}`).toEqual([]);
  });
});

describe("i18n call sites - the repaired French entries are French", () => {
  test("no repaired key carries the English string on the French side", () => {
    // GIVEN the sixteen repaired keys, minus the one identical in both languages
    // WHEN the French entry is compared to the English one
    const untranslated = REPAIRED.filter(
      (name) =>
        !IDENTICAL_ACROSS_LOCALES.has(name) &&
        leafAt(fr as unknown as JsonObject, name) === leafAt(en as unknown as JsonObject, name),
    );
    // THEN none repeats its English counterpart, which is the defect returning
    // under a key that now exists
    expect(untranslated, `fr.json repeats the English text for: ${untranslated.join(", ")}`).toEqual(
      [],
    );
  });

  test("the interpolation placeholders survive on both sides", () => {
    // GIVEN the four repaired keys whose call site passes values
    const interpolated: [string, string][] = [
      ["chat.rate_limit.too_fast_countdown", "{s}"],
      ["chat.rate_limit.too_many_countdown", "{s}"],
      ["chat.retry.badge_title", "{n}"],
      ["chat.retry.timeline_label", "{n}"],
    ];
    // WHEN each catalogue entry is read
    // THEN both languages still carry the placeholder the call site fills
    for (const [name, placeholder] of interpolated) {
      for (const [file, catalogue] of CATALOGUES) {
        const text = leafAt(catalogue, name) ?? "";
        expect(text, `${file}: ${name} lost ${placeholder}`).toContain(placeholder);
      }
    }
  });
});
