#!/usr/bin/env node
/*
 * Icon-only button audit.
 *
 * A `<button>` whose whole content is an icon (a lucide component, an image,
 * an svg) and nothing readable must expose `aria-label`, `aria-labelledby` or
 * `title`, otherwise assistive technology announces "button" and nothing else.
 *
 * The previous generation of this script matched `<button ([^>]*)>` with a
 * regexp, which cut the opening tag at the first `>` of an inline arrow
 * handler, and blanked `{#if}` branches before deciding: a button whose only
 * child is `{#if busy}<Spinner />{:else}<Check />{/if}` came out with an empty
 * body and was declared "not icon-only", so the whole family went unjudged.
 * It also scanned the raw source, so a `<button>` written inside a comment or
 * a doc block counted as markup.
 *
 * This version walks the markup parsed by `svelte/compiler`: the button's
 * content is icon-only when no descendant carries readable text, and an
 * `{#if}` branch is content like any other. Two shapes carry a name this file
 * cannot see and are not reported: a `<label>` around the button (a button is
 * a labelable element), and a `{...spread}` on it, which can carry the
 * `aria-label` its call site passes.
 *
 * Exit codes: 0 clean, 1 violations, 2 nothing measured (no `.svelte` file
 * read, parser failure).
 *
 * Escape hatch: `data-a11y-ignore="reason"` on the button.
 */
import { readFile, readdir, stat, mkdtemp, mkdir, writeFile, rm } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { tmpdir } from "node:os";
import path from "node:path";
import { parse } from "svelte/compiler";

const SRC_ROOT = path.resolve(fileURLToPath(import.meta.url), "../../src");

const USAGE = `usage: node scripts/audit-a11y.mjs [--selftest] [--help]

Audits every icon-only <button> under crates/apollia-desktop/ui/src for an
accessible name (aria-label, aria-labelledby or title).

  --selftest   run the positive and negative controls on a temporary fixture
               tree, never on the repository, and report their verdicts
  --help       print this and exit 0 without reading the tree

Exit codes: 0 clean, 1 violations found, 2 nothing measured.
`;

/** Walk `dir` recursively, yielding every `.svelte` file. */
async function* walkDir(dir) {
  for (const entry of await readdir(dir)) {
    const full = path.join(dir, entry);
    const info = await stat(full);
    if (info.isDirectory()) yield* walkDir(full);
    else if (entry.endsWith(".svelte")) yield full;
  }
}

/**
 * Depth-first walk of a Svelte 5 markup fragment. `visit` receives each node
 * and the stack of its ancestors, outermost first.
 *
 * @param {unknown} node
 * @param {(node: Record<string, any>, ancestors: Record<string, any>[]) => void} visit
 * @param {Record<string, any>[]} ancestors
 */
function walkMarkup(node, visit, ancestors = []) {
  if (Array.isArray(node)) {
    for (const child of node) walkMarkup(child, visit, ancestors);
    return;
  }
  if (typeof node !== "object" || node === null) return;
  const n = /** @type {Record<string, any>} */ (node);
  if (n.type === "Comment" || n.type === "Script") return;

  visit(n, ancestors);
  const next = [...ancestors, n];

  switch (n.type) {
    case "Fragment":
      walkMarkup(n.nodes ?? [], visit, ancestors);
      return;
    case "IfBlock":
      walkMarkup(n.consequent, visit, next);
      walkMarkup(n.alternate, visit, next);
      return;
    case "EachBlock":
      walkMarkup(n.body, visit, next);
      walkMarkup(n.fallback, visit, next);
      return;
    case "AwaitBlock":
      walkMarkup(n.pending, visit, next);
      walkMarkup(n.then, visit, next);
      walkMarkup(n.catch, visit, next);
      return;
    default:
      if (n.fragment) walkMarkup(n.fragment, visit, next);
      if (n.body) walkMarkup(n.body, visit, next);
      return;
  }
}

function hasAttribute(node, name) {
  return (node.attributes ?? []).some((a) => a.type === "Attribute" && a.name === name);
}

/** A spread can carry `aria-label` from the call site: nothing to decide here. */
function hasSpread(node) {
  return (node.attributes ?? []).some((a) => a.type === "SpreadAttribute");
}

/**
 * A button is icon-only when it renders something, and nothing it renders can
 * be read out: no text node, no interpolation, no rendered snippet.
 *
 * @param {Record<string, any>} button
 */
function isIconOnly(button) {
  let rendered = 0;
  let readable = 0;
  walkMarkup(button.fragment, (node) => {
    switch (node.type) {
      case "Text":
        if ((node.data ?? node.raw ?? "").trim() !== "") readable++;
        return;
      case "ExpressionTag":
      case "HtmlTag":
      case "RenderTag":
        readable++;
        return;
      case "RegularElement":
      case "Component":
      case "SvelteElement":
      case "SvelteComponent":
        rendered++;
        return;
      default:
        return;
    }
  });
  return rendered > 0 && readable === 0;
}

/**
 * Audit one tree of `.svelte` files.
 *
 * @param {string} root
 * @returns {Promise<{violations: string[], files: number}>}
 */
async function auditTree(root) {
  const violations = [];
  let files = 0;

  for await (const file of walkDir(root)) {
    files++;
    const source = await readFile(file, "utf8");
    const rel = path.relative(root, file);
    const ast = parse(source, { modern: true });

    walkMarkup(ast.fragment, (node, ancestors) => {
      if (node.type !== "RegularElement" || node.name !== "button") return;
      if (!isIconOnly(node)) return;
      if (
        hasAttribute(node, "aria-label") ||
        hasAttribute(node, "aria-labelledby") ||
        hasAttribute(node, "title") ||
        hasAttribute(node, "data-a11y-ignore")
      ) {
        return;
      }
      // A `<button>` is a labelable element: a `<label>` around it names it.
      if (ancestors.some((a) => a.type === "RegularElement" && a.name === "label")) return;
      if (hasSpread(node)) return;
      const line = source.slice(0, node.start).split("\n").length;
      violations.push(`${rel}:${line}  icon-only <button> missing aria-label/title`);
    });
  }

  violations.sort();
  return { violations, files };
}

/* ── selftest ─────────────────────────────────────────────────────────── */

const FIXTURE_CASES = [
  { name: "labelled.svelte", body: `<button aria-label="Close"><X /></button>`, red: false },
  { name: "titled.svelte", body: `<button title="Close"><X /></button>`, red: false },
  { name: "with-text.svelte", body: `<button><X />Close</button>`, red: false },
  { name: "with-expression.svelte", body: `<button><X />{$t("common.close")}</button>`, red: false },
  { name: "ignored.svelte", body: `<button data-a11y-ignore="decorative"><X /></button>`, red: false },
  { name: "empty.svelte", body: `<button></button>`, red: false },
  { name: "bare-icon.svelte", body: `<button><X size={16} /></button>`, red: true },
  {
    name: "arrow-handler.svelte",
    body: `<button onclick={(e) => go(e)}><X size={16} /></button>`,
    red: true,
  },
  {
    name: "icon-in-branch.svelte",
    body: `<button>{#if busy}<Spinner />{:else}<Check />{/if}</button>`,
    red: true,
  },
  {
    name: "in-comment.svelte",
    body: `<!-- <button><X /></button> -->\n<button aria-label="Close"><X /></button>`,
    red: false,
  },
  { name: "wrapped-in-label.svelte", body: `<label>Enabled<button><X /></button></label>`, red: false },
  { name: "spread-attrs.svelte", body: `<button {...restProps}><X /></button>`, red: false },
];

/**
 * Replay a positive and a negative control on a temporary fixture tree. Never
 * reads the repository. Returns the process exit code.
 */
async function selftest() {
  const dir = await mkdtemp(path.join(tmpdir(), "a11y-selftest-"));
  const failures = [];
  try {
    for (const kase of FIXTURE_CASES) {
      const caseRoot = path.join(dir, path.basename(kase.name, ".svelte"));
      await mkdir(caseRoot, { recursive: true });
      await writeFile(path.join(caseRoot, kase.name), `${kase.body}\n`, "utf8");
      const { violations } = await auditTree(caseRoot);
      const red = violations.length > 0;
      const ok = red === kase.red;
      console.log(
        `  ${ok ? "ok  " : "FAIL"} ${kase.name}: expected ${kase.red ? "a violation" : "no violation"}, got ${violations.length}`,
      );
      if (!ok) failures.push(kase.name);
    }

    const emptyRoot = path.join(dir, "empty-tree");
    await mkdir(emptyRoot, { recursive: true });
    const { files } = await auditTree(emptyRoot);
    const emptyOk = files === 0;
    console.log(`  ${emptyOk ? "ok  " : "FAIL"} empty tree: read ${files} file(s), expected 0`);
    if (!emptyOk) failures.push("empty tree");
  } finally {
    await rm(dir, { recursive: true, force: true });
  }

  if (failures.length > 0) {
    console.error(`\nselftest: ${failures.length} control(s) wrong: ${failures.join(", ")}`);
    return 1;
  }
  console.log(`\nselftest: ${FIXTURE_CASES.length + 1} controls, all correct. ✓`);
  return 0;
}

/* ── entry point ──────────────────────────────────────────────────────── */

async function main(argv) {
  const known = new Set(["--selftest", "--help", "-h"]);
  for (const arg of argv) {
    if (!known.has(arg)) {
      console.error(`unknown argument: ${arg}\n\n${USAGE}`);
      return 2;
    }
  }
  if (argv.includes("--help") || argv.includes("-h")) {
    console.log(USAGE);
    return 0;
  }
  if (argv.includes("--selftest")) return selftest();

  const { violations, files } = await auditTree(SRC_ROOT);

  if (files === 0) {
    console.error(`A11y audit - no .svelte file read under ${SRC_ROOT}; nothing measured.`);
    return 2;
  }
  if (violations.length > 0) {
    console.error("A11y audit - violations found:\n");
    for (const v of violations) console.error(`  ✗ ${v}`);
    console.error(`\nTotal: ${violations.length} violation(s) over ${files} file(s).`);
    console.error("Fix by adding aria-label={$t('…')} (preferred) or title=\"…\".");
    return 1;
  }

  console.log(`A11y audit - 0 violations over ${files} file(s). ✓`);
  return 0;
}

process.exit(await main(process.argv.slice(2)));
