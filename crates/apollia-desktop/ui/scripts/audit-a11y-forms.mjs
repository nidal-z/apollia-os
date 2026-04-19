#!/usr/bin/env node
/*
 * Form-field label audit (US-SP42-007 — F.76 / E.50).
 *
 * Every `<Input …/>` (or Select/Textarea) in a .svelte file must
 * either:
 *   – carry `aria-label` / `aria-labelledby`, OR
 *   – declare `id="…"` paired with a `<label for="…">` elsewhere
 *     in the same file.
 *
 * Scanner walks the source string and balances `{…}` expressions
 * so arrow-function `>` inside `on:foo={(e) => …}` handlers don't
 * split the tag prematurely.
 *
 * Escape hatch: add `data-a11y-ignore="reason"` on the primitive.
 */
import { readFile, readdir, stat } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import path from "node:path";

const ROOT = path.resolve(fileURLToPath(import.meta.url), "../../src");
const PRIMITIVES = new Set(["Input", "Select", "Textarea"]);

async function* walk(dir) {
  for (const entry of await readdir(dir)) {
    const full = path.join(dir, entry);
    const info = await stat(full);
    if (info.isDirectory()) yield* walk(full);
    else if (entry.endsWith(".svelte")) yield full;
  }
}

/**
 * Return each `<Input|Select|Textarea …>` opening-tag attribute
 * block in `src`, balanced against Svelte `{…}` expressions.
 */
function* primitiveTags(src) {
  const nameRe = /<([A-Z][A-Za-z0-9]*)\b/g;
  let match;
  while ((match = nameRe.exec(src)) !== null) {
    if (!PRIMITIVES.has(match[1])) continue;
    // Walk forward from after the name; balance braces; stop at
    // top-level `>` or `/>`.
    let i = nameRe.lastIndex;
    let depth = 0;
    let inString = null; // track quote char so a stray `{` in an
                         // attribute value doesn't break balance.
    while (i < src.length) {
      const ch = src[i];
      if (inString) {
        if (ch === inString) inString = null;
        i++;
        continue;
      }
      if (ch === '"' || ch === "'") { inString = ch; i++; continue; }
      if (ch === "{") { depth++; i++; continue; }
      if (ch === "}") { depth--; i++; continue; }
      if (depth === 0 && ch === ">") {
        break;
      }
      i++;
    }
    yield {
      name: match[1],
      attrs: src.slice(nameRe.lastIndex, i),
      index: match.index,
    };
  }
}

function extractAttr(attrs, name) {
  const re = new RegExp(`\\b${name}=(?:"([^"]+)"|'([^']+)'|\\{([^}]+)\\})`);
  const m = attrs.match(re);
  if (!m) return null;
  return (m[1] ?? m[2] ?? m[3] ?? "").replace(/[{}]/g, "").trim();
}

const violations = [];

for await (const file of walk(ROOT)) {
  const source = await readFile(file, "utf8");
  const rel = path.relative(ROOT, file);

  // Harvest every <label for="…"> target in this file.
  const forIds = new Set();
  for (const match of source.matchAll(
    /<label\b[^>]*\bfor=(?:"([^"]+)"|'([^']+)'|\{([^}]+)\})/g,
  )) {
    forIds.add((match[1] ?? match[2] ?? match[3] ?? "").replace(/[{}]/g, "").trim());
  }

  for (const { name, attrs, index } of primitiveTags(source)) {
    if (/\bdata-a11y-ignore\b/.test(attrs)) continue;
    if (/\baria-label(?:ledby)?\b/.test(attrs)) continue;
    const id = extractAttr(attrs, "id");
    const line = source.slice(0, index).split("\n").length;
    if (!id) {
      violations.push(`${rel}:${line}  <${name}> without id/aria-label`);
      continue;
    }
    if (!forIds.has(id)) {
      violations.push(
        `${rel}:${line}  <${name} id="${id}"> has no matching <label for="${id}">`,
      );
    }
  }
}

if (violations.length > 0) {
  console.error("A11y forms audit — violations found:\n");
  for (const v of violations) console.error(`  ✗ ${v}`);
  console.error(`\nTotal: ${violations.length} violation(s).`);
  process.exit(1);
}

console.log("A11y forms audit — 0 violations. ✓");
