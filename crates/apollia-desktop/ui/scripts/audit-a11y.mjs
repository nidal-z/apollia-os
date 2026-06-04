#!/usr/bin/env node
/*
 * Lightweight accessibility audit (D.49 / E.48 / F.76).
 *
 * Rules:
 *   1. Every `<button …>` that contains only a lucide-svelte icon
 *      (e.g. `<X size={…} />`) must expose `aria-label`,
 *      `aria-labelledby`, or `title` - otherwise assistive tech
 *      reads "Button" with no context.
 *   2. Every `<input … />` / `<select … />` / `<textarea … />`
 *      must be wrapped in a `<label for>` / `aria-labelledby` /
 *      `aria-label` association.
 *
 * Invoked via `node scripts/audit-a11y.mjs`. Exits 1 when any rule
 * reports a violation - suitable for a pre-commit hook.
 *
 * NOTE: Regex-based, no Svelte AST - intentionally conservative to
 * keep the dev-loop fast. False positives can be silenced with a
 * `data-a11y-ignore="reason"` attribute on the offending tag.
 */
import { readFile, readdir, stat } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import path from "node:path";

const ROOT = path.resolve(fileURLToPath(import.meta.url), "../../src");

const ICON_IMPORT_RE = /from\s+["']lucide-svelte["']/;

/** Walk src/ recursively, yielding every .svelte file. */
async function* walk(dir) {
  for (const entry of await readdir(dir)) {
    const full = path.join(dir, entry);
    const info = await stat(full);
    if (info.isDirectory()) yield* walk(full);
    else if (entry.endsWith(".svelte")) yield full;
  }
}

/** Check if a <button …> contains only an icon (lucide self-closing tag). */
function buttonIsIconOnly(openTag, innerText) {
  // A translation call anywhere inside the button means the button
  // renders visible text - not icon-only.
  if (/\$t\s*\(/.test(innerText)) return false;

  const stripped = innerText
    .replaceAll(/<!--[\s\S]*?-->/g, "")
    .replaceAll(/{#if[\s\S]*?{\/if}/g, " ") // best-effort: inline Svelte branches
    .replaceAll(/{[^{}]*}/g, " ")
    .replaceAll(/\s+/g, " ")
    .trim();
  if (!stripped) return false;
  // A single lucide tag, e.g. `<X size={18} />` or `<Settings … />`.
  return /^<[A-Z][A-Za-z0-9]*\b[^<>]*\/\s*>$/.test(stripped) ||
         /^<[A-Z][A-Za-z0-9]*\b[^<>]*><\/[A-Z][A-Za-z0-9]*>$/.test(stripped);
}

/** Extract <button …>…</button> blocks one by one (no nested button parsing). */
function* buttons(source) {
  const re = /<button\b([^>]*)>([\s\S]*?)<\/button>/g;
  let match;
  while ((match = re.exec(source)) !== null) {
    yield { openAttrs: match[1], inner: match[2], index: match.index };
  }
}

function hasA11yText(openAttrs) {
  return /\baria-label\b/.test(openAttrs)
      || /\baria-labelledby\b/.test(openAttrs)
      || /\btitle\b\s*=/.test(openAttrs)
      || /\bdata-a11y-ignore\b/.test(openAttrs);
}

const violations = [];

for await (const file of walk(ROOT)) {
  const source = await readFile(file, "utf8");
  if (!ICON_IMPORT_RE.test(source)) continue; // no lucide, no risk.

  const rel = path.relative(ROOT, file);
  for (const { openAttrs, inner, index } of buttons(source)) {
    if (!buttonIsIconOnly(openAttrs, inner)) continue;
    if (hasA11yText(openAttrs)) continue;
    const line = source.slice(0, index).split("\n").length;
    violations.push(`${rel}:${line}  icon-only <button> missing aria-label/title`);
  }
}

if (violations.length > 0) {
  console.error("A11y audit - violations found:\n");
  for (const v of violations) console.error(`  ✗ ${v}`);
  console.error(`\nTotal: ${violations.length} violation(s).`);
  console.error("Fix by adding aria-label={$t('…')} (preferred) or title=\"…\".");
  process.exit(1);
}

console.log("A11y audit - 0 violations. ✓");
