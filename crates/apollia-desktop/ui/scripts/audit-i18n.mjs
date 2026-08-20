#!/usr/bin/env node
/*
 * Hardcoded UI string audit.
 *
 * Greps every `.svelte` file under `src/` for user-visible strings that
 * look like copy but aren't routed through `$t(...)`. Exits 1 when any
 * match remains outside the whitelist - wire this into CI / a pre-commit
 * hook once the baseline hits 0.
 *
 * Every file under `src/` is read and scanned, including the excused ones:
 * the verdict says how many files it enforced, how many it excused, and how
 * many findings the excused files carry. A guard that concluded on a file it
 * never opened would be reporting on a subject it had not read.
 *
 * Whitelist rules:
 *   - design-system showcase routes (`src/routes/Design*.svelte`)
 *     are dev-only and not part of the end-user product, so their findings
 *     are counted and printed but never change the exit code,
 *   - strings inside `$t("...")` calls are keys, not copy,
 *   - HTML/Svelte comment blocks are ignored,
 *   - attribute values that interpolate (`aria-label={...}`,
 *     `placeholder={...}`) are already dynamic.
 *
 * A violation can be silenced with a `<!-- i18n-ignore: reason -->`
 * comment on the line above the offending markup when the string
 * genuinely doesn't need translation (e.g. brand name, technical id).
 * The directive covers its own line, the line below it, and the rest of
 * the element that opens there, so it also reaches an attribute buried
 * in a multi-line tag.
 *
 * Both scanners read markup whose `{...}` expressions have been blanked,
 * and neither match crosses a `<` or a `>`. That is deliberate: the
 * previous character class spanned U+0021 to U+2013, so it swallowed the
 * closing tag, reported fragments of code as copy, and dragged the tag
 * into the snippet, which made the brand whitelist unmatchable.
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
  "Apollia Chat",
  "MCP",
  "GPU",
  "OpenAI",
  "Mistral",
  "Anthropic",
  "Ollama",
  "Metal",
  "CUDA",
  // third-party search engines, named the same in every locale
  "Brave Search",
  "DuckDuckGo",
  // placeholder example values for technical config fields
  "local-code",
  "qwen3-0.6b-q8_0",
  "gpt-4o-mini",
  "Qwen/Qwen3-30B-A3B-GGUF",
]);

const IGNORE_DIRECTIVE = /<!--\s*i18n-ignore\b/;

/** Shortest copy string the scanner reports, in characters. */
const MIN_COPY_LENGTH = 4;

/**
 * @param {string} dir
 * @returns {AsyncGenerator<string>}
 */
async function* walk(dir) {
  for (const entry of await readdir(dir)) {
    const full = path.join(dir, entry);
    const info = await stat(full);
    if (info.isDirectory()) yield* walk(full);
    else if (entry.endsWith(".svelte")) yield full;
  }
}

/**
 * Replace every matched span with spaces, keeping newlines so line numbers hold.
 *
 * @param {string} src
 * @param {RegExp} re
 * @returns {string}
 */
function blankMatches(src, re) {
  return src.replaceAll(re, (chunk) => chunk.replaceAll(/[^\n]/g, " "));
}

/**
 * Replace the contents of every `{...}` expression with spaces, braces
 * included. Depth-aware, so nested braces and `${...}` inside a template
 * literal are consumed by the same pass. Newlines survive.
 *
 * @param {string} src
 * @returns {string}
 */
function blankMustaches(src) {
  const chars = [...src];
  let depth = 0;
  for (let i = 0; i < chars.length; i += 1) {
    const c = chars[i];
    if (c === "{") {
      depth += 1;
      chars[i] = " ";
    } else if (c === "}" && depth > 0) {
      depth -= 1;
      chars[i] = " ";
    } else if (depth > 0 && c !== "\n") {
      chars[i] = " ";
    }
  }
  return chars.join("");
}

/**
 * Blank `<style>`, `<script>`, comments and Svelte expressions, so what is
 * left is markup whose text nodes and attribute values are literal.
 * Every replacement keeps the newlines it covered, so a match index still
 * maps to its source line.
 *
 * @param {string} src
 * @returns {string}
 */
function stripNonMarkup(src) {
  let out = blankMatches(src, /<style[\s\S]*?<\/style>/gi);
  out = blankMatches(out, /<script[\s\S]*?<\/script>/gi);
  out = blankMatches(out, /<!--[\s\S]*?-->/g);
  return blankMustaches(out);
}

/**
 * Lines silenced by a `<!-- i18n-ignore: reason -->` comment, 1-based.
 *
 * A directive covers its own line, the line below it, and the rest of the
 * element that opens there. The last part matters because an attribute
 * cannot carry a comment of its own: a directive aimed at a `placeholder`
 * sits above the tag, and the tag may spread over a dozen lines.
 *
 * @param {string} src
 * @returns {Set<number>}
 */
function ignoredLines(src) {
  /** @type {Set<number>} */
  const silenced = new Set();
  const lines = src.split("\n");
  const lineStarts = [0];
  for (const line of lines) {
    lineStarts.push((lineStarts.at(-1) ?? 0) + line.length + 1);
  }

  lines.forEach((line, index) => {
    if (!IGNORE_DIRECTIVE.test(line)) return;
    silenced.add(index + 1);
    silenced.add(index + 2);

    const open = src.indexOf("<", lineStarts[index + 1] ?? src.length);
    if (open === -1) return;
    const close = endOfOpeningTag(src, open);
    if (close === -1) return;
    const first = index + 2;
    const last = lineOf(src, close);
    for (let n = first; n <= last; n += 1) silenced.add(n);
  });
  return silenced;
}

/**
 * Index of the `>` that closes the tag opening at `start`, quotes skipped.
 *
 * @param {string} src
 * @param {number} start
 * @returns {number}
 */
function endOfOpeningTag(src, start) {
  /** @type {string | null} */
  let quote = null;
  for (let i = start + 1; i < src.length; i += 1) {
    const c = src[i];
    if (quote !== null) {
      if (c === quote) quote = null;
      continue;
    }
    if (c === '"' || c === "'") quote = c;
    else if (c === ">") return i;
    else if (c === "<") return -1;
  }
  return -1;
}

/**
 * Collapse the whitespace a blanked expression left behind.
 *
 * @param {string} raw
 * @returns {string}
 */
function normalize(raw) {
  return raw.replaceAll(/\s+/g, " ").trim();
}

/**
 * Number of leading whitespace characters in a captured text node.
 *
 * @param {string} raw
 * @returns {number}
 */
function leadingBlanks(raw) {
  return raw.length - raw.trimStart().length;
}

/**
 * Length of the span `[start, start + length)` of `source`, whitespace
 * collapsed, before any blanking pass touched it.
 *
 * The blanking passes are index-preserving, so the caller can hand over the
 * match offsets it computed on the blanked markup.
 *
 * @param {string} source
 * @param {number} start
 * @param {number} length
 * @returns {number}
 */
function unblankedLength(source, start, length) {
  return normalize(source.slice(start, start + length)).length;
}

/**
 * 1-based line holding `index`.
 *
 * @param {string} markup
 * @param {number} index
 * @returns {number}
 */
function lineOf(markup, index) {
  let line = 1;
  for (let i = 0; i < index; i += 1) if (markup[i] === "\n") line += 1;
  return line;
}

/**
 * Return the list of hardcoded-string findings in a file body.
 *
 * `markup` is expected to have gone through `stripNonMarkup`. `silenced`
 * is the set of 1-based lines covered by an `i18n-ignore` directive.
 * `source` is the body `markup` was derived from. Every blanking pass keeps
 * its indices, so a match into `markup` slices the same span out of
 * `source`, and MIN_COPY_LENGTH is measured on that unblanked span.
 *
 * Measuring after blanking is what made `TTL {expiresRel}` invisible: the
 * expression left three literal characters, the threshold dropped the node,
 * and no directive or whitelist entry recorded the decision. The reported
 * snippet is still the blanked literal part, so messages keep their shape.
 *
 * @param {string} markup
 * @param {Set<number>} [silenced]
 * @param {string} [source]
 * @returns {{ kind: string, snippet: string }[]}
 */
function scanMarkup(markup, silenced = new Set(), source = markup) {
  /** @type {{ kind: string, snippet: string }[]} */
  const findings = [];

  // 1) Static text between tags: `>Some Text<`.
  //    The class excludes `<` and `>` so a match never crosses the closing
  //    tag, and `{}` are already blanked, so an interpolation cannot end it
  //    either. Capitalized, and at least MIN_COPY_LENGTH characters once
  //    trimmed.
  const textRe = />([^<>]+)</g;
  // 2) Attribute values that contain copy: title / aria-label / placeholder /
  //    alt / subtitle / confirmLabel / cancelLabel / message
  const attrRe =
    /\b(title|aria-label|placeholder|alt|subtitle|confirmLabel|cancelLabel|message)\s*=\s*"([^"\n]+)"/g;

  let match;
  while ((match = textRe.exec(markup)) !== null) {
    const start = match.index + 1;
    const text = normalize(match[1]);
    if (unblankedLength(source, start, match[1].length) < MIN_COPY_LENGTH)
      continue;
    if (!/^[A-ZÀ-Ÿ]/.test(text)) continue;
    if (BRAND_WHITELIST.has(text)) continue;
    // Anchor on the first visible character, not on the `>` that opened the
    // text node: that `>` can belong to a self-closing icon on the line
    // above, and a directive is written above the words it silences.
    const anchor = start + leadingBlanks(match[1]);
    if (silenced.has(lineOf(markup, anchor))) continue;
    findings.push({ kind: "text", snippet: text });
  }
  while ((match = attrRe.exec(markup)) !== null) {
    const value = normalize(match[2]);
    const valueStart = match.index + match[0].lastIndexOf(match[2]);
    if (unblankedLength(source, valueStart, match[2].length) < MIN_COPY_LENGTH)
      continue;
    if (!/^[A-Za-zÀ-ÿ]/.test(value)) continue;
    if (BRAND_WHITELIST.has(value)) continue;
    if (silenced.has(lineOf(markup, match.index))) continue;
    findings.push({ kind: `attr:${match[1]}`, snippet: value });
  }
  return findings;
}

/**
 * Print what the run read, what it enforced, and what it excused.
 *
 * `enforced + excused.length` is the number of files walked, so a reader can
 * check the coverage against `find src -name '*.svelte' | wc -l` without
 * trusting the guard's own arithmetic.
 *
 * @param {number} enforced
 * @param {{ file: string, count: number }[]} excused
 * @returns {void}
 */
function reportCoverage(enforced, excused) {
  const excusedFindings = excused.reduce((sum, e) => sum + e.count, 0);
  console.log(
    `audit-i18n: ${enforced + excused.length} files, ${enforced} enforced, ` +
      `${excused.length} excused (design-system showcase routes, dev-only)`,
  );
  for (const { file, count } of excused) {
    console.log(`  excused  ${file}  ${count} finding(s), not enforced`);
  }
  if (excused.length > 0) {
    console.log(`  excused findings, total: ${excusedFindings}`);
  }
}

async function main() {
  let totalFindings = 0;
  let enforced = 0;
  const offenders = [];
  /** @type {{ file: string, count: number }[]} */
  const excused = [];

  for await (const file of walk(ROOT)) {
    const rel = path.relative(ROOT, file);
    const raw = await readFile(file, "utf8");
    const markup = stripNonMarkup(raw);
    const findings = scanMarkup(markup, ignoredLines(raw), raw);

    if (WHITELIST_FILES.has(rel)) {
      excused.push({ file: rel, count: findings.length });
      continue;
    }

    enforced += 1;
    if (findings.length === 0) continue;

    offenders.push({ file: rel, findings });
    totalFindings += findings.length;
  }

  reportCoverage(enforced, excused);

  if (totalFindings === 0) {
    console.log(
      `✓ No hardcoded UI strings in the ${enforced} enforced .svelte files`,
    );
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

export { ignoredLines, scanMarkup, stripNonMarkup };

const invokedDirectly =
  process.argv[1] !== undefined &&
  path.resolve(process.argv[1]) === fileURLToPath(import.meta.url);

if (invokedDirectly) {
  try {
    await main();
  } catch (err) {
    console.error(err);
    process.exit(2);
  }
}
