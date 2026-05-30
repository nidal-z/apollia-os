#!/usr/bin/env node
/*
 * Hardcoded UI string audit (US-SP42-008).
 *
 * Greps every `.svelte` file under `src/` for user-visible strings that
 * look like copy but aren't routed through `$t(...)`. Exits 1 when any
 * match remains outside the whitelist — wire this into CI / a pre-commit
 * hook once the baseline hits 0.
 *
 * Whitelist rules:
 *   - design-system showcase routes (`src/routes/Design*.svelte`)
 *     are dev-only and not part of the end-user product,
 *   - strings inside `$t("...")` calls are keys, not copy,
 *   - HTML/Svelte comment blocks are ignored,
 *   - attribute values that interpolate (`aria-label={...}`,
 *     `placeholder={...}`) are already dynamic.
 *
 * A violation can be silenced with a `<!-- i18n-ignore: reason -->`
 * comment on the line above the offending markup when the string
 * genuinely doesn't need translation (e.g. brand name, technical id).
 */
import { readFile, readdir, stat } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import path from "node:path";

const ROOT = path.resolve(fileURLToPath(import.meta.url), "../../src");

const WHITELIST_FILES = new Set([
  "routes/Design.svelte",
  "routes/DesignMotion.svelte",
  "routes/DesignEmptyStates.svelte",
  "routes/DesignDarkMode.svelte",
]);

// Literal brand / technical tokens that don't translate.
const BRAND_WHITELIST = new Set([
  "Apollia",
  "Apollia OS",
  "MCP",
  "GPU",
  "OpenAI",
  "Mistral",
  "Anthropic",
  "Ollama",
  "Metal",
  "CUDA",
  // placeholder example values for technical config fields
  "local-code",
  "qwen3-0.6b-q8_0",
]);

async function* walk(dir) {
  for (const entry of await readdir(dir)) {
    const full = path.join(dir, entry);
    const info = await stat(full);
    if (info.isDirectory()) yield* walk(full);
    else if (entry.endsWith(".svelte")) yield full;
  }
}

/** Strip `<!-- ... -->` blocks and `<style>…</style>` / `<script>…</script>`. */
function stripNonMarkup(src) {
  return src
    .replaceAll(/<!--[\s\S]*?-->/g, "")
    .replaceAll(/<style[\s\S]*?<\/style>/gi, "")
    .replaceAll(/<script[\s\S]*?<\/script>/gi, "");
}

/** Return the list of hardcoded-string findings in a file body. */
function scanMarkup(markup) {
  const findings = [];

  // 1) Static text between tags: `>Some Text<`
  //    Capitalized word of 4+ letters (matches Workspace, Thinking, etc.)
  const textRe = />\s*([A-ZÀ-Ÿ][a-zA-ZÀ-ÿ ,'.?!—–-]{3,})\s*</g;
  // 2) Attribute values that contain copy: title / aria-label / placeholder /
  //    alt / subtitle / confirmLabel / cancelLabel / message
  const attrRe =
    /\b(title|aria-label|placeholder|alt|subtitle|confirmLabel|cancelLabel|message)\s*=\s*"([A-Za-zÀ-ÿ][^"{}\n]{3,})"/g;

  let match;
  while ((match = textRe.exec(markup)) !== null) {
    const text = match[1].trim();
    if (BRAND_WHITELIST.has(text)) continue;
    findings.push({ kind: "text", snippet: text });
  }
  while ((match = attrRe.exec(markup)) !== null) {
    const value = match[2].trim();
    if (BRAND_WHITELIST.has(value)) continue;
    findings.push({ kind: `attr:${match[1]}`, snippet: value });
  }
  return findings;
}

async function main() {
  let totalFindings = 0;
  const offenders = [];

  for await (const file of walk(ROOT)) {
    const rel = path.relative(ROOT, file);
    if (WHITELIST_FILES.has(rel)) continue;

    const raw = await readFile(file, "utf8");
    const markup = stripNonMarkup(raw);
    const findings = scanMarkup(markup);
    if (findings.length === 0) continue;

    offenders.push({ file: rel, findings });
    totalFindings += findings.length;
  }

  if (totalFindings === 0) {
    console.log("✓ No hardcoded UI strings found in src/**/*.svelte");
    process.exit(0);
  }

  console.error(`✗ Found ${totalFindings} hardcoded UI string(s):\n`);
  for (const { file, findings } of offenders) {
    console.error(`  ${file}`);
    for (const f of findings) {
      console.error(`    [${f.kind}] ${f.snippet}`);
    }
  }
  process.exit(1);
}

try {
  await main();
} catch (err) {
  console.error(err);
  process.exit(2);
}
